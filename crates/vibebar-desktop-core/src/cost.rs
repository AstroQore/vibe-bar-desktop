//! Read-only local Codex, Claude, and Gemini CLI usage/cost.
//!
//! This first slice scans bounded JSONL inputs and keeps only an in-memory
//! snapshot. It never reads provider credentials and never writes any shared
//! Vibe Bar cost, history, pricing, or ledger store.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions as CapOpenOptions};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::ToolType;

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 1024 * 1024;
const MAX_FILES: usize = 20_000;
const MAX_FILES_PER_PROVIDER: usize = MAX_FILES / 3;
const MAX_DISCOVERED_PER_PROVIDER: usize = MAX_FILES_PER_PROVIDER * 10;
const MAX_DISCOVERY_ENTRIES: usize = 200_000;
const MAX_DISCOVERY_DEPTH: usize = 32;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const PRICING_VERSION: &str = "native-2026-06-08-v5";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostTotals {
    pub priced_cost_micros: i64,
    pub tokens: u64,
    pub requests: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DailyCost {
    pub day: String,
    pub priced_cost_micros: i64,
    pub tokens: u64,
    pub requests: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    pub tool: ToolType,
    pub model: String,
    pub priced_cost_micros: i64,
    pub tokens: u64,
    pub requests: u64,
    pub unpriced_events: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostView {
    pub today: CostTotals,
    pub last_7_days: CostTotals,
    pub last_30_days: CostTotals,
    pub all_time: CostTotals,
    pub daily: Vec<DailyCost>,
    pub models: Vec<ModelCost>,
    pub unpriced_events: u64,
    pub scanned_files: u64,
    pub malformed_lines: u64,
    pub truncated: bool,
    pub scanned_at: f64,
    pub pricing_version: String,
}

#[derive(Clone)]
pub struct CostEngine {
    home: PathBuf,
    store: crate::client_store::ClientStore,
    is_demo: bool,
    cached: Arc<RwLock<CostView>>,
    refresh_gate: Arc<std::sync::Mutex<()>>,
}

impl CostEngine {
    pub fn new(root: crate::paths::DataRoot, home: impl Into<PathBuf>) -> Self {
        let store = crate::client_store::ClientStore::new(root.clone());
        let cached = store
            .load_cost_snapshot()
            .unwrap_or_else(|| empty_view(0.0));
        Self {
            home: home.into(),
            store,
            is_demo: root.is_demo(),
            cached: Arc::new(RwLock::new(cached)),
            refresh_gate: Arc::new(std::sync::Mutex::new(())),
        }
    }

    pub fn cached(&self) -> CostView {
        self.cached
            .read()
            .map(|view| view.clone())
            .unwrap_or_else(|_| empty_view(0.0))
    }

    pub fn refresh(&self) -> Result<CostView, String> {
        let _refresh_guard = self
            .refresh_gate
            .lock()
            .map_err(|_| "cost refresh lock poisoned".to_string())?;
        let home = crate::paths::open_ambient_dir(&self.home)
            .map_err(|_| "cost scan home is not readable".to_string())?;
        let (mut codex_files, codex_truncated) = collect_provider_files(
            &self.home,
            ToolType::Codex,
            &[".codex/sessions", ".codex/archived_sessions"],
            MAX_FILES_PER_PROVIDER,
        );
        let (claude_files, claude_truncated) = collect_provider_files(
            &self.home,
            ToolType::Claude,
            &[".claude/projects", ".config/claude/projects"],
            MAX_FILES_PER_PROVIDER,
        );
        codex_files.extend(claude_files);
        let (gemini_files, gemini_truncated) = collect_gemini_chat_files(&self.home);
        codex_files.extend(gemini_files);
        let files = codex_files;
        let truncated = codex_truncated || claude_truncated || gemini_truncated;
        let codex_tier = codex_service_tier(&home);

        let mut events = Vec::new();
        let mut malformed_lines = 0;
        for source in &files {
            scan_file(
                &home,
                &self.home,
                source,
                codex_tier.as_deref(),
                &mut events,
                &mut malformed_lines,
            );
        }
        let events = deduplicate_events(events);

        let view = aggregate(
            &events,
            files.len() as u64,
            malformed_lines,
            truncated,
            now_unix(),
        );
        if let Ok(mut cached) = self.cached.write() {
            *cached = view.clone();
        }
        if !self.is_demo {
            // The snapshot is only a restart cache. A completed local scan
            // remains useful even when the private namespace is unwritable.
            let _ = self.store.save_cost_snapshot(&view);
        }
        Ok(view)
    }
}

fn empty_view(scanned_at: f64) -> CostView {
    CostView {
        scanned_at,
        pricing_version: PRICING_VERSION.to_string(),
        ..Default::default()
    }
}

#[derive(Debug)]
struct SourceFile {
    tool: ToolType,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct UsageEvent {
    tool: ToolType,
    date: f64,
    model: String,
    input: u64,
    cache_read: u64,
    output: u64,
    cache_creation: u64,
    service_tier: Option<String>,
    session_id: Option<String>,
    message_id: Option<String>,
    request_id: Option<String>,
    is_sidechain: bool,
    is_parent_path: bool,
    source_key: String,
}

impl UsageEvent {
    fn tokens(&self) -> u64 {
        self.input
            .saturating_add(self.cache_read)
            .saturating_add(self.output)
            .saturating_add(self.cache_creation)
    }
}

fn now_unix() -> f64 {
    Utc::now().timestamp_millis() as f64 / 1_000.0
}

fn collect_provider_files(
    home: &Path,
    tool: ToolType,
    roots: &[&str],
    limit: usize,
) -> (Vec<SourceFile>, bool) {
    let mut files = Vec::new();
    let mut budget = DiscoveryBudget::default();
    for root in roots {
        collect_jsonl(&home.join(root), tool, &mut files, 0, &mut budget);
        if budget.truncated {
            break;
        }
    }
    let discovery_truncated = budget.truncated;
    files.sort_by(|left, right| {
        source_mtime(&right.path)
            .cmp(&source_mtime(&left.path))
            .then_with(|| left.path.cmp(&right.path))
    });
    let truncated = discovery_truncated || files.len() > limit;
    files.truncate(limit);
    (files, truncated)
}

fn collect_gemini_chat_files(home: &Path) -> (Vec<SourceFile>, bool) {
    let gemini = home.join(".gemini");
    let tmp = gemini.join("tmp");
    if !safe_directory(&gemini) || !safe_directory(&tmp) {
        return (Vec::new(), false);
    }
    let root = tmp;
    let Ok(projects) = fs::read_dir(root) else {
        return (Vec::new(), false);
    };
    let mut files = Vec::new();
    let mut truncated = false;
    let mut budget = DiscoveryBudget::default();
    for project in projects.flatten() {
        budget.entries += 1;
        if budget.entries > MAX_DISCOVERY_ENTRIES {
            truncated = true;
            break;
        }
        let Ok(kind) = project.file_type() else {
            continue;
        };
        if kind.is_symlink() || !kind.is_dir() {
            continue;
        }
        let chats = project.path().join("chats");
        if !safe_directory(&chats) {
            continue;
        }
        let Ok(entries) = fs::read_dir(chats) else {
            continue;
        };
        for entry in entries.flatten() {
            budget.entries += 1;
            if budget.entries > MAX_DISCOVERY_ENTRIES {
                truncated = true;
                break;
            }
            if files.len() >= MAX_FILES_PER_PROVIDER {
                truncated = true;
                break;
            }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name_ok = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("session-"));
            if kind.is_symlink()
                || !kind.is_file()
                || !name_ok
                || path.extension().and_then(|e| e.to_str()) != Some("jsonl")
            {
                continue;
            }
            files.push(SourceFile {
                tool: ToolType::Gemini,
                path,
            });
        }
        if truncated {
            break;
        }
    }
    files.sort_by(|a, b| {
        source_mtime(&b.path)
            .cmp(&source_mtime(&a.path))
            .then_with(|| a.path.cmp(&b.path))
    });
    (files, truncated)
}

fn safe_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| !metadata.file_type().is_symlink() && metadata.is_dir())
        .unwrap_or(false)
}

#[derive(Default)]
struct DiscoveryBudget {
    entries: usize,
    truncated: bool,
}

fn source_mtime(path: &Path) -> u128 {
    fs::symlink_metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn codex_service_tier(home: &Dir) -> Option<String> {
    let directory = crate::paths::open_dir_nofollow(home, Path::new(".codex")).ok()?;
    let mut options = CapOpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = directory
        .open_with(Path::new("config.toml"), &options)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    for raw_line in text.lines() {
        let setting = raw_line.split('#').next().unwrap_or("");
        let Some((key, value)) = setting.split_once('=') else {
            continue;
        };
        if key.trim() != "service_tier" {
            continue;
        }
        let value = value.trim().trim_matches(['\'', '"']);
        if matches!(value, "fast" | "priority") {
            return Some(value.to_string());
        }
    }
    None
}

fn collect_jsonl(
    root: &Path,
    tool: ToolType,
    out: &mut Vec<SourceFile>,
    depth: usize,
    budget: &mut DiscoveryBudget,
) {
    if depth > MAX_DISCOVERY_DEPTH
        || out.len() >= MAX_DISCOVERED_PER_PROVIDER
        || budget.entries >= MAX_DISCOVERY_ENTRIES
    {
        budget.truncated = true;
        return;
    }
    let Ok(root_metadata) = fs::symlink_metadata(root) else {
        return;
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        budget.entries += 1;
        if out.len() >= MAX_DISCOVERED_PER_PROVIDER || budget.entries > MAX_DISCOVERY_ENTRIES {
            budget.truncated = true;
            return;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_jsonl(&path, tool, out, depth + 1, budget);
            if budget.truncated {
                return;
            }
        } else if file_type.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
        {
            out.push(SourceFile { tool, path });
        }
    }
}

fn scan_file(
    home: &Dir,
    home_path: &Path,
    source: &SourceFile,
    codex_tier: Option<&str>,
    events: &mut Vec<UsageEvent>,
    malformed_lines: &mut u64,
) {
    let Ok(metadata) = fs::symlink_metadata(&source.path) else {
        return;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return;
    }
    let Ok(file) = open_source_file(home, home_path, source) else {
        return;
    };
    let Ok(open_metadata) = file.metadata() else {
        return;
    };
    if !open_metadata.is_file() || open_metadata.len() > MAX_FILE_BYTES {
        return;
    }
    let fallback_time = open_metadata
        .modified()
        .ok()
        .map(|modified| modified.into_std())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64());
    let mut bytes = Vec::with_capacity(open_metadata.len() as usize);
    if file
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_FILE_BYTES
    {
        return;
    }

    let mut codex_previous = (0_u64, 0_u64, 0_u64);
    let mut codex_model = "gpt-5".to_string();
    let mut gemini_session_id = None;
    for line in bytes.split(|byte| *byte == b'\n') {
        if line.is_empty() || line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if line.len() > MAX_LINE_BYTES {
            *malformed_lines += 1;
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            *malformed_lines += 1;
            continue;
        };
        match source.tool {
            ToolType::Codex => parse_codex(
                &value,
                fallback_time,
                &mut codex_model,
                &mut codex_previous,
                codex_tier,
                &source.path,
                events,
            ),
            ToolType::Claude => parse_claude(&value, fallback_time, &source.path, events),
            ToolType::Gemini => parse_gemini(
                &value,
                fallback_time,
                &source.path,
                &mut gemini_session_id,
                events,
            ),
            _ => {}
        }
    }
}

fn parse_gemini(
    value: &Value,
    fallback_time: Option<f64>,
    source_path: &Path,
    session_id: &mut Option<String>,
    events: &mut Vec<UsageEvent>,
) {
    if let Some(value) = value.get("sessionId").and_then(Value::as_str) {
        if session_id.is_none() {
            *session_id = Some(value.to_string());
        }
    }
    if value.get("type").and_then(Value::as_str) != Some("gemini") {
        return;
    }
    let Some(tokens) = value.get("tokens").and_then(Value::as_object) else {
        return;
    };
    let input_total = number_map(tokens, "input");
    let cache_read = number_map(tokens, "cached");
    let output = number_map(tokens, "output")
        .saturating_add(number_map(tokens, "thoughts"))
        .saturating_add(number_map(tokens, "tool"));
    if input_total == 0 && cache_read == 0 && output == 0 {
        return;
    }
    let Some(date) = timestamp(value).or(fallback_time) else {
        return;
    };
    events.push(UsageEvent {
        tool: ToolType::Gemini,
        date,
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or("gemini-unknown")
            .to_string(),
        input: input_total.saturating_sub(cache_read),
        cache_read,
        output,
        cache_creation: 0,
        service_tier: None,
        session_id: session_id.clone(),
        message_id: value.get("id").and_then(Value::as_str).map(str::to_string),
        request_id: None,
        is_sidechain: false,
        is_parent_path: true,
        source_key: source_path.to_string_lossy().into_owned(),
    });
}

fn open_source_file(
    home: &Dir,
    home_path: &Path,
    source: &SourceFile,
) -> std::io::Result<cap_std::fs::File> {
    let relative = source.path.strip_prefix(home_path).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "source escaped scan home",
        )
    })?;
    let mut components = crate::paths::normal_components(relative)?;
    let leaf = components.pop().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "source has no filename",
        )
    })?;
    let mut directory = home.try_clone()?;
    for component in components {
        directory = cap_fs_ext::DirExt::open_dir_nofollow(&directory, component)?;
    }
    let mut options = CapOpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    directory.open_with(Path::new(leaf), &options)
}

fn parse_codex(
    value: &Value,
    fallback_time: Option<f64>,
    current_model: &mut String,
    previous: &mut (u64, u64, u64),
    service_tier: Option<&str>,
    source_path: &Path,
    events: &mut Vec<UsageEvent>,
) {
    if let Some(model) = value
        .get("payload")
        .and_then(|payload| payload.get("model"))
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("payload")
                .and_then(|payload| payload.get("info"))
                .and_then(|info| info.get("model").or_else(|| info.get("model_name")))
                .and_then(Value::as_str)
        })
        .or_else(|| value.get("model").and_then(Value::as_str))
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        *current_model = model.to_string();
    }

    if value.get("type").and_then(Value::as_str) != Some("event_msg") {
        return;
    }
    let Some(payload) = value.get("payload") else {
        return;
    };
    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
        return;
    }
    let Some(info) = payload.get("info") else {
        return;
    };

    let (total_input, cached, output) = if let Some(total) = info.get("total_token_usage") {
        let next = (
            number(total, "input_tokens"),
            number(total, "cached_input_tokens").max(number(total, "cache_read_input_tokens")),
            number(total, "output_tokens"),
        );
        let delta = (
            next.0.saturating_sub(previous.0),
            next.1.saturating_sub(previous.1),
            next.2.saturating_sub(previous.2),
        );
        *previous = next;
        delta
    } else if let Some(last) = info.get("last_token_usage") {
        let delta = (
            number(last, "input_tokens"),
            number(last, "cached_input_tokens").max(number(last, "cache_read_input_tokens")),
            number(last, "output_tokens"),
        );
        previous.0 = previous.0.saturating_add(delta.0);
        previous.1 = previous.1.saturating_add(delta.1);
        previous.2 = previous.2.saturating_add(delta.2);
        delta
    } else {
        return;
    };
    let input = total_input.saturating_sub(cached);
    if input == 0 && cached == 0 && output == 0 {
        return;
    }
    let Some(date) = timestamp(value).or(fallback_time) else {
        return;
    };
    events.push(UsageEvent {
        tool: ToolType::Codex,
        date,
        model: current_model.clone(),
        input,
        cache_read: cached,
        output,
        cache_creation: 0,
        service_tier: service_tier.map(str::to_string),
        session_id: None,
        message_id: None,
        request_id: None,
        is_sidechain: false,
        is_parent_path: true,
        source_key: source_path.to_string_lossy().into_owned(),
    });
}

fn parse_claude(
    value: &Value,
    fallback_time: Option<f64>,
    source_path: &Path,
    events: &mut Vec<UsageEvent>,
) {
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    let Some(message) = value.get("message") else {
        return;
    };
    let Some(usage) = message.get("usage") else {
        return;
    };
    let input = number(usage, "input_tokens");
    let cache_read = number(usage, "cache_read_input_tokens");
    let cache_creation = number(usage, "cache_creation_input_tokens");
    let output = number(usage, "output_tokens");
    if input == 0 && cache_read == 0 && cache_creation == 0 && output == 0 {
        return;
    }
    let Some(date) = timestamp(value).or(fallback_time) else {
        return;
    };
    events.push(UsageEvent {
        tool: ToolType::Claude,
        date,
        model: message
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .unwrap_or("claude-sonnet-4-5")
            .to_string(),
        input,
        cache_read,
        output,
        cache_creation,
        service_tier: usage
            .get("speed")
            .or_else(|| usage.get("service_tier"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|tier| !tier.is_empty())
            .map(str::to_string),
        session_id: value
            .get("sessionId")
            .or_else(|| value.get("session_id"))
            .or_else(|| {
                value
                    .get("metadata")
                    .and_then(|metadata| metadata.get("sessionId"))
            })
            .or_else(|| {
                message
                    .get("metadata")
                    .and_then(|metadata| metadata.get("sessionId"))
            })
            .and_then(Value::as_str)
            .map(str::to_string),
        message_id: message
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string),
        request_id: value
            .get("requestId")
            .or_else(|| value.get("request_id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        is_sidechain: bool_value(value.get("isSidechain")),
        is_parent_path: !source_path
            .components()
            .any(|component| component.as_os_str() == "subagents"),
        source_key: source_path.to_string_lossy().into_owned(),
    });
}

fn bool_value(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_i64().is_some_and(|value| value != 0),
        Some(Value::String(value)) => {
            matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1")
        }
        _ => false,
    }
}

fn deduplicate_events(events: Vec<UsageEvent>) -> Vec<UsageEvent> {
    let mut keyed = BTreeMap::new();
    let mut unkeyed = Vec::new();
    for event in events {
        let key = if event.tool == ToolType::Gemini {
            event
                .session_id
                .as_deref()
                .zip(event.message_id.as_deref())
                .map(|(session, message)| format!("gemini\0{session}\0{message}"))
        } else if event.tool == ToolType::Claude {
            event
                .session_id
                .as_deref()
                .zip(event.message_id.as_deref())
                .zip(event.request_id.as_deref())
                .map(|((session, message), request)| {
                    format!("claude\0{session}\0{message}\0{request}")
                })
        } else {
            None
        };
        let Some(key) = key else {
            unkeyed.push(event);
            continue;
        };
        if keyed
            .get(&key)
            .is_none_or(|existing| claude_event_wins(&event, existing))
        {
            keyed.insert(key, event);
        }
    }
    keyed.into_values().chain(unkeyed).collect()
}

fn claude_event_wins(candidate: &UsageEvent, existing: &UsageEvent) -> bool {
    if candidate.is_sidechain != existing.is_sidechain {
        return !candidate.is_sidechain;
    }
    if candidate.is_parent_path != existing.is_parent_path {
        return candidate.is_parent_path;
    }
    if candidate.source_key == existing.source_key {
        return true;
    }
    candidate.source_key < existing.source_key
}

fn number(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|number| {
            number
                .as_u64()
                .or_else(|| number.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .unwrap_or(0)
}

fn number_map(value: &serde_json::Map<String, Value>, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|number| {
            number
                .as_u64()
                .or_else(|| number.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .unwrap_or(0)
}

fn timestamp(value: &Value) -> Option<f64> {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
        .map(|timestamp| timestamp.timestamp_millis() as f64 / 1_000.0)
}

#[derive(Clone, Copy)]
struct ModelPricing {
    input: f64,
    output: f64,
    cache_read: Option<f64>,
    cache_creation: Option<f64>,
    threshold: Option<u64>,
    input_above: Option<f64>,
    output_above: Option<f64>,
    cache_read_above: Option<f64>,
    cache_creation_above: Option<f64>,
    fast_multiplier: Option<f64>,
}

impl ModelPricing {
    const fn simple(input: f64, output: f64, cache_read: Option<f64>) -> Self {
        Self {
            input,
            output,
            cache_read,
            cache_creation: None,
            threshold: None,
            input_above: None,
            output_above: None,
            cache_read_above: None,
            cache_creation_above: None,
            fast_multiplier: None,
        }
    }

    const fn claude(input: f64, output: f64, creation: f64, read: f64) -> Self {
        Self {
            input,
            output,
            cache_read: Some(read),
            cache_creation: Some(creation),
            threshold: None,
            input_above: None,
            output_above: None,
            cache_read_above: None,
            cache_creation_above: None,
            fast_multiplier: None,
        }
    }

    const fn threshold(
        input: f64,
        output: f64,
        read: f64,
        threshold: u64,
        input_above: f64,
        output_above: f64,
        read_above: f64,
    ) -> Self {
        Self {
            input,
            output,
            cache_read: Some(read),
            cache_creation: None,
            threshold: Some(threshold),
            input_above: Some(input_above),
            output_above: Some(output_above),
            cache_read_above: Some(read_above),
            cache_creation_above: None,
            fast_multiplier: None,
        }
    }
}

fn priced_cost_micros(event: &UsageEvent) -> Option<i64> {
    let pricing = match event.tool {
        ToolType::Codex => codex_pricing(&event.model)?,
        ToolType::Claude => claude_pricing(&event.model)?,
        ToolType::Gemini => gemini_pricing(&event.model)?,
        _ => return None,
    };
    let cache_read_rate = pricing.cache_read.unwrap_or(pricing.input);
    let cache_creation_rate = pricing.cache_creation.unwrap_or(pricing.input);
    let mut micros = tiered(
        event.input,
        pricing.input,
        pricing.input_above,
        pricing.threshold,
    ) + tiered(
        event.output,
        pricing.output,
        pricing.output_above,
        pricing.threshold,
    ) + tiered(
        event.cache_read,
        cache_read_rate,
        pricing.cache_read_above.or(pricing.input_above),
        pricing.threshold,
    ) + tiered(
        event.cache_creation,
        cache_creation_rate,
        pricing.cache_creation_above.or(pricing.input_above),
        pricing.threshold,
    );
    if event
        .service_tier
        .as_deref()
        .is_some_and(|tier| matches!(tier.to_ascii_lowercase().as_str(), "fast" | "priority"))
    {
        micros *= pricing.fast_multiplier.unwrap_or(1.0).max(1.0);
    }
    Some(micros.round() as i64)
}

fn gemini_pricing(raw: &str) -> Option<ModelPricing> {
    match raw.trim() {
        "gemini-2.5-pro" => Some(ModelPricing::threshold(
            1.25, 10.0, 0.31, 200_000, 2.5, 15.0, 0.625,
        )),
        "gemini-2.5-flash" => Some(ModelPricing::simple(0.3, 2.5, Some(0.075))),
        "gemini-2.5-flash-lite" => Some(ModelPricing::simple(0.1, 0.4, Some(0.025))),
        "gemini-3-pro" | "gemini-3-pro-preview" => Some(ModelPricing::threshold(
            2.0, 12.0, 0.5, 200_000, 4.0, 18.0, 1.0,
        )),
        "gemini-3-flash" => Some(ModelPricing::simple(0.35, 2.8, Some(0.0875))),
        "gemini-3-flash-lite" => Some(ModelPricing::simple(0.125, 0.5, Some(0.031))),
        _ => None,
    }
}

fn tiered(tokens: u64, base: f64, above: Option<f64>, threshold: Option<u64>) -> f64 {
    match (threshold, above) {
        (Some(threshold), Some(above)) => {
            let below = tokens.min(threshold);
            let over = tokens.saturating_sub(threshold);
            below as f64 * base + over as f64 * above
        }
        _ => tokens as f64 * base,
    }
}

fn codex_pricing(raw: &str) -> Option<ModelPricing> {
    let model = raw.trim().strip_prefix("openai/").unwrap_or(raw.trim());
    codex_pricing_exact(model)
        .or_else(|| strip_codex_date_suffix(model).and_then(codex_pricing_exact))
}

fn codex_pricing_exact(model: &str) -> Option<ModelPricing> {
    let mut pricing = match model {
        "gpt-5" | "gpt-5-codex" | "gpt-5.1" | "gpt-5.1-codex" | "gpt-5.1-codex-max" => {
            ModelPricing::simple(1.25, 10.0, Some(0.125))
        }
        "gpt-5-mini" | "gpt-5.1-codex-mini" => ModelPricing::simple(0.25, 2.0, Some(0.025)),
        "gpt-5-nano" => ModelPricing::simple(0.05, 0.4, Some(0.005)),
        "gpt-5-pro" => ModelPricing::simple(15.0, 120.0, None),
        "gpt-5.2" | "gpt-5.2-codex" => ModelPricing::simple(1.75, 14.0, Some(0.175)),
        "gpt-5.2-pro" => ModelPricing::simple(21.0, 168.0, None),
        "gpt-5.3-codex" => ModelPricing::simple(1.75, 14.0, Some(0.175)),
        "gpt-5.3-codex-spark" => ModelPricing::simple(0.0, 0.0, Some(0.0)),
        "gpt-5.4" => ModelPricing::simple(2.5, 15.0, Some(0.25)),
        "gpt-5.4-mini" => ModelPricing::simple(0.75, 4.5, Some(0.075)),
        "gpt-5.4-nano" => ModelPricing::simple(0.2, 1.25, Some(0.02)),
        "gpt-5.4-pro" | "gpt-5.5-pro" => ModelPricing::simple(30.0, 180.0, None),
        "gpt-5.5" => ModelPricing::simple(5.0, 30.0, Some(0.5)),
        _ => return None,
    };
    pricing.fast_multiplier = match model {
        "gpt-5.3-codex" | "gpt-5.4" => Some(2.0),
        "gpt-5.5" => Some(2.5),
        _ => None,
    };
    Some(pricing)
}

fn strip_codex_date_suffix(model: &str) -> Option<&str> {
    let (base, suffix) = model.rsplit_once('-')?;
    if suffix.len() == 2 {
        let (base, month) = base.rsplit_once('-')?;
        let (base, year) = base.rsplit_once('-')?;
        if year.len() == 4
            && month.len() == 2
            && year.bytes().all(|byte| byte.is_ascii_digit())
            && month.bytes().all(|byte| byte.is_ascii_digit())
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Some(base);
        }
    }
    None
}

fn claude_pricing(raw: &str) -> Option<ModelPricing> {
    let model = normalize_claude_model(raw);
    let mut pricing = match model.as_str() {
        "claude-haiku-4-5" | "claude-haiku-4-5-20251001" => {
            ModelPricing::claude(1.0, 5.0, 1.25, 0.1)
        }
        "claude-opus-4-5" | "claude-opus-4-5-20251101" => {
            ModelPricing::claude(5.0, 25.0, 6.25, 0.5)
        }
        "claude-opus-4-6" | "claude-opus-4-6-20260205" | "claude-opus-4-7" | "claude-opus-4-8" => {
            ModelPricing::claude(5.0, 25.0, 6.25, 0.5)
        }
        "claude-sonnet-4-5" | "claude-sonnet-4-5-20250929" | "claude-sonnet-4-20250514" => {
            let mut pricing = ModelPricing::claude(3.0, 15.0, 3.75, 0.3);
            pricing.threshold = Some(200_000);
            pricing.input_above = Some(6.0);
            pricing.output_above = Some(22.5);
            pricing.cache_creation_above = Some(7.5);
            pricing.cache_read_above = Some(0.6);
            pricing
        }
        "claude-sonnet-4-6" => ModelPricing::claude(3.0, 15.0, 3.75, 0.3),
        "claude-opus-4-20250514" | "claude-opus-4-1" => {
            ModelPricing::claude(15.0, 75.0, 18.75, 1.5)
        }
        _ => return None,
    };
    pricing.fast_multiplier = match model.as_str() {
        "claude-opus-4-6" | "claude-opus-4-6-20260205" | "claude-opus-4-7" => Some(6.0),
        "claude-opus-4-8" => Some(2.0),
        _ => None,
    };
    Some(pricing)
}

fn normalize_claude_model(raw: &str) -> String {
    let mut model = raw.trim().to_string();
    if let Some(stripped) = model.strip_prefix("anthropic.") {
        model = stripped.to_string();
    }
    if let Some(tail) = model.rsplit('.').next() {
        if tail.starts_with("claude-") {
            model = tail.to_string();
        }
    }
    if let Some(index) = model.rfind("-v") {
        let suffix = &model[index + 2..];
        if suffix.split_once(':').is_some_and(|(left, right)| {
            !left.is_empty()
                && !right.is_empty()
                && left.bytes().all(|byte| byte.is_ascii_digit())
                && right.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            model.truncate(index);
        }
    }
    if model.len() > 9 && model.as_bytes()[model.len() - 9] == b'-' {
        let suffix = &model[model.len() - 8..];
        if suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            let base = &model[..model.len() - 9];
            if claude_pricing_exact_base(base) {
                model = base.to_string();
            }
        }
    }
    model
}

fn claude_pricing_exact_base(model: &str) -> bool {
    matches!(
        model,
        "claude-haiku-4-5" | "claude-opus-4-5" | "claude-opus-4-6" | "claude-sonnet-4-5"
    )
}

fn aggregate(
    events: &[UsageEvent],
    scanned_files: u64,
    malformed_lines: u64,
    truncated: bool,
    scanned_at: f64,
) -> CostView {
    let Some(today) = local_day(scanned_at) else {
        return CostView {
            scanned_files,
            malformed_lines,
            truncated,
            scanned_at,
            pricing_version: PRICING_VERSION.to_string(),
            ..Default::default()
        };
    };
    let mut today_totals = CostTotals::default();
    let mut week_totals = CostTotals::default();
    let mut month_totals = CostTotals::default();
    let mut all_totals = CostTotals::default();
    let mut daily: BTreeMap<String, CostTotals> = BTreeMap::new();
    let mut models: HashMap<(ToolType, String), ModelCost> = HashMap::new();
    let mut unpriced_events = 0_u64;

    for event in events {
        if !event.date.is_finite() || event.date <= 0.0 || event.date > scanned_at {
            continue;
        }
        let Some(day) = local_day(event.date) else {
            continue;
        };
        let age_days = today.signed_duration_since(day).num_days();
        if age_days < 0 {
            continue;
        }
        let tokens = event.tokens();
        let cost = priced_cost_micros(event);

        add_totals(&mut all_totals, tokens, cost);
        if age_days == 0 {
            add_totals(&mut today_totals, tokens, cost);
        }
        if age_days < 7 {
            add_totals(&mut week_totals, tokens, cost);
        }
        if age_days < 30 {
            add_totals(&mut month_totals, tokens, cost);
        }
        add_totals(daily.entry(day.to_string()).or_default(), tokens, cost);

        let model = models
            .entry((event.tool, event.model.clone()))
            .or_insert_with(|| ModelCost {
                tool: event.tool,
                model: event.model.clone(),
                priced_cost_micros: 0,
                tokens: 0,
                requests: 0,
                unpriced_events: 0,
            });
        model.tokens = model.tokens.saturating_add(tokens);
        model.requests = model.requests.saturating_add(1);
        match cost {
            Some(cost) => {
                model.priced_cost_micros = model.priced_cost_micros.saturating_add(cost);
            }
            None => {
                model.unpriced_events = model.unpriced_events.saturating_add(1);
                unpriced_events = unpriced_events.saturating_add(1);
            }
        }
    }

    let mut models = models.into_values().collect::<Vec<_>>();
    models.sort_by(|left, right| {
        right
            .priced_cost_micros
            .cmp(&left.priced_cost_micros)
            .then_with(|| right.tokens.cmp(&left.tokens))
            .then_with(|| left.tool.raw_value().cmp(right.tool.raw_value()))
            .then_with(|| left.model.cmp(&right.model))
    });
    CostView {
        today: today_totals,
        last_7_days: week_totals,
        last_30_days: month_totals,
        all_time: all_totals,
        daily: daily
            .into_iter()
            .map(|(day, totals)| DailyCost {
                day,
                priced_cost_micros: totals.priced_cost_micros,
                tokens: totals.tokens,
                requests: totals.requests,
            })
            .collect(),
        models,
        unpriced_events,
        scanned_files,
        malformed_lines,
        truncated,
        scanned_at,
        pricing_version: PRICING_VERSION.to_string(),
    }
}

fn local_day(timestamp: f64) -> Option<chrono::NaiveDate> {
    DateTime::<Utc>::from_timestamp_millis((timestamp * 1_000.0).round() as i64)
        .map(|date| date.with_timezone(&Local).date_naive())
}

fn add_totals(totals: &mut CostTotals, tokens: u64, cost: Option<i64>) {
    totals.tokens = totals.tokens.saturating_add(tokens);
    totals.requests = totals.requests.saturating_add(1);
    if let Some(cost) = cost {
        totals.priced_cost_micros = totals.priced_cost_micros.saturating_add(cost);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_store::ClientStore;
    use crate::paths::DataRoot;

    fn rfc3339(timestamp: f64) -> String {
        DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
            .unwrap()
            .to_rfc3339()
    }

    fn write_jsonl(path: &Path, lines: &[Value]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(path, body).unwrap();
    }

    #[test]
    fn scans_codex_cumulative_deltas_model_and_cache_without_double_counting() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        fs::create_dir_all(home.path().join(".codex")).unwrap();
        fs::write(
            home.path().join(".codex/config.toml"),
            "service_tier = \"fast\" # synthetic\n",
        )
        .unwrap();
        write_jsonl(
            &home.path().join(".codex/sessions/2026/session.jsonl"),
            &[
                serde_json::json!({"type":"turn_context","payload":{"model":"gpt-5.4"}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-10.0),"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30}}}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-5.0),"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":150,"cached_input_tokens":30,"output_tokens":50}}}}),
            ],
        );
        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert_eq!(view.scanned_files, 1);
        assert_eq!(view.all_time.requests, 2);
        assert_eq!(view.all_time.tokens, 200);
        assert_eq!(view.today.tokens, 200);
        assert_eq!(view.models[0].model, "gpt-5.4");
        assert_eq!(view.unpriced_events, 0);
        assert_eq!(view.all_time.priced_cost_micros, 2_115);
    }

    #[test]
    fn scans_claude_cache_fast_tier_and_deduplicates_keyed_rows() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        let keyed = serde_json::json!({
            "type":"assistant","timestamp":rfc3339(scanned_at-10.0),"sessionId":"session-1","requestId":"req-1",
            "message":{"id":"msg-1","model":"claude-haiku-4-5","usage":{
                "input_tokens":10,"cache_read_input_tokens":2,
                "cache_creation_input_tokens":3,"output_tokens":4}}
        });
        let fast = serde_json::json!({
            "type":"assistant","timestamp":rfc3339(scanned_at-5.0),
            "message":{"model":"claude-opus-4-6","usage":{
                "input_tokens":1,"output_tokens":1,"speed":"fast"}}
        });
        write_jsonl(
            &home.path().join(".claude/projects/project/session.jsonl"),
            &[keyed.clone(), keyed, fast],
        );
        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert_eq!(view.all_time.requests, 2);
        assert_eq!(view.all_time.tokens, 21);
        assert_eq!(view.models.len(), 2);
        assert_eq!(view.all_time.priced_cost_micros, 214);
        let opus = view
            .models
            .iter()
            .find(|model| model.model == "claude-opus-4-6")
            .unwrap();
        assert_eq!(opus.priced_cost_micros, 180);
    }

    #[test]
    fn unknown_future_malformed_and_oversized_lines_are_honest() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        let path = home.path().join(".claude/projects/project/session.jsonl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let unknown = serde_json::json!({
            "type":"assistant","timestamp":rfc3339(scanned_at-1.0),
            "message":{"model":"future-model","usage":{"input_tokens":2,"output_tokens":3}}
        });
        let future = serde_json::json!({
            "type":"assistant","timestamp":rfc3339(scanned_at+3600.0),
            "message":{"model":"claude-haiku-4-5","usage":{"input_tokens":100,"output_tokens":100}}
        });
        let mut body = format!("{}\nnot-json\n{}\n", unknown, future).into_bytes();
        body.extend(std::iter::repeat_n(b'x', MAX_LINE_BYTES + 1));
        fs::write(&path, body).unwrap();
        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert_eq!(view.all_time.requests, 1);
        assert_eq!(view.all_time.tokens, 5);
        assert_eq!(view.unpriced_events, 1);
        assert_eq!(view.malformed_lines, 2);
    }

    #[test]
    fn local_calendar_windows_do_not_treat_all_history_as_today() {
        let scanned_at = now_unix();
        let today = local_day(scanned_at).unwrap();
        let local_timestamp = |days_ago: u64| {
            if days_ago == 0 {
                return scanned_at - 1.0;
            }
            let day = today - chrono::Days::new(days_ago);
            day.and_hms_opt(12, 0, 0)
                .and_then(|time| time.and_local_timezone(Local).earliest())
                .unwrap()
                .timestamp() as f64
        };
        let event = |days_ago| UsageEvent {
            tool: ToolType::Codex,
            date: local_timestamp(days_ago),
            model: "gpt-5".into(),
            input: 1,
            cache_read: 0,
            output: 0,
            cache_creation: 0,
            service_tier: None,
            session_id: None,
            message_id: None,
            request_id: None,
            is_sidechain: false,
            is_parent_path: true,
            source_key: String::new(),
        };
        let view = aggregate(&[event(0), event(8), event(31)], 3, 0, false, scanned_at);
        assert_eq!(view.today.requests, 1);
        assert_eq!(view.last_7_days.requests, 1);
        assert_eq!(view.last_30_days.requests, 2);
        assert_eq!(view.all_time.requests, 3);
    }

    #[test]
    fn empty_scan_is_read_only_and_returns_an_explicit_scanned_view() {
        let home = tempfile::tempdir().unwrap();
        let before = fs::read_dir(home.path()).unwrap().count();
        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        let after = fs::read_dir(home.path()).unwrap().count();
        assert_eq!(before, after);
        assert_eq!(view.scanned_files, 0);
        assert_eq!(view.all_time.requests, 0);
        assert!(view.scanned_at > 0.0);
        assert_eq!(view.pricing_version, PRICING_VERSION);
    }

    #[test]
    fn completed_scan_persists_an_aggregate_snapshot_for_a_new_engine() {
        let home = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(home.path().join(".vibebar"));
        let view = CostEngine::new(root.clone(), home.path())
            .refresh()
            .unwrap();
        assert!(root.client_cost_snapshot_file().is_file());
        assert!(view.scanned_at > 0.0);
        let reloaded = CostEngine::new(root, home.path()).cached();
        assert_eq!(reloaded, view);
    }

    #[test]
    fn failed_scan_keeps_the_last_private_snapshot_unchanged() {
        let home = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(home.path().join(".vibebar"));
        let view = CostEngine::new(root.clone(), home.path())
            .refresh()
            .unwrap();
        let before = fs::read(root.client_cost_snapshot_file()).unwrap();
        let missing = home.path().join("missing-home");
        assert!(CostEngine::new(root.clone(), missing).refresh().is_err());
        assert_eq!(fs::read(root.client_cost_snapshot_file()).unwrap(), before);
        assert_eq!(ClientStore::new(root).load_cost_snapshot(), Some(view));
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_write_failure_keeps_the_completed_scan_in_memory() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(home.path().join(".vibebar"));
        fs::create_dir_all(root.shared()).unwrap();
        let outside = home.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.shared().join("client")).unwrap();

        let engine = CostEngine::new(root, home.path());
        let view = engine.refresh().unwrap();
        assert!(view.scanned_at > 0.0);
        assert_eq!(engine.cached(), view);
        assert!(fs::read_dir(outside).unwrap().next().is_none());
    }

    #[test]
    fn sonnet_threshold_uses_the_published_above_threshold_rate() {
        let event = UsageEvent {
            tool: ToolType::Claude,
            date: now_unix() - 1.0,
            model: "claude-sonnet-4-5".into(),
            input: 200_001,
            cache_read: 0,
            output: 0,
            cache_creation: 0,
            service_tier: None,
            session_id: None,
            message_id: None,
            request_id: None,
            is_sidechain: false,
            is_parent_path: true,
            source_key: String::new(),
        };
        assert_eq!(priced_cost_micros(&event), Some(600_006));
    }

    #[test]
    fn claude_dedupe_keeps_sessions_separate_and_prefers_parent_rows() {
        let event = |session: &str, sidechain: bool, parent: bool, input: u64| UsageEvent {
            tool: ToolType::Claude,
            date: now_unix() - 1.0,
            model: "claude-haiku-4-5".into(),
            input,
            cache_read: 0,
            output: 0,
            cache_creation: 0,
            service_tier: None,
            session_id: Some(session.into()),
            message_id: Some("message".into()),
            request_id: Some("request".into()),
            is_sidechain: sidechain,
            is_parent_path: parent,
            source_key: if parent { "parent" } else { "subagents/child" }.into(),
        };
        let values = deduplicate_events(vec![
            event("one", true, false, 999),
            event("one", false, true, 10),
            event("two", false, true, 20),
        ]);
        assert_eq!(values.len(), 2);
        assert_eq!(values.iter().map(|value| value.input).sum::<u64>(), 30);
    }

    #[test]
    fn provider_file_caps_are_fair_and_report_truncation() {
        let home = tempfile::tempdir().unwrap();
        for relative in [
            ".codex/sessions/one.jsonl",
            ".codex/sessions/two.jsonl",
            ".claude/projects/one.jsonl",
        ] {
            let path = home.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        let (codex, codex_truncated) =
            collect_provider_files(home.path(), ToolType::Codex, &[".codex/sessions"], 1);
        let (claude, claude_truncated) =
            collect_provider_files(home.path(), ToolType::Claude, &[".claude/projects"], 1);
        assert_eq!(codex.len(), 1);
        assert!(codex_truncated);
        assert_eq!(claude.len(), 1);
        assert!(!claude_truncated);
    }

    #[test]
    fn deeply_nested_empty_trees_stop_at_the_discovery_budget() {
        let home = tempfile::tempdir().unwrap();
        let mut directory = home.path().join(".codex/sessions");
        for index in 0..=MAX_DISCOVERY_DEPTH {
            directory.push(format!("level-{index}"));
        }
        fs::create_dir_all(directory).unwrap();
        let (files, truncated) =
            collect_provider_files(home.path(), ToolType::Codex, &[".codex/sessions"], 1);
        assert!(files.is_empty());
        assert!(truncated);
    }

    #[test]
    fn scans_gemini_chat_history_deduplicates_and_prices() {
        let home = tempfile::tempdir().unwrap();
        let path = home
            .path()
            .join(".gemini/tmp/project/chats/session-one.jsonl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let ts = now_unix() - 1.0;
        write_jsonl(
            &path,
            &[
                serde_json::json!({"sessionId":"s"}),
                serde_json::json!({"type":"gemini","id":"m","model":"gemini-2.5-flash","timestamp":rfc3339(ts),"tokens":{"input":100,"cached":20,"output":30,"thoughts":4,"tool":6}}),
                serde_json::json!({"type":"gemini","id":"m","model":"gemini-2.5-flash","timestamp":rfc3339(ts),"tokens":{"input":100,"cached":20,"output":30,"thoughts":4,"tool":6}}),
                serde_json::json!({"type":"gemini","model":"unknown-gemini","timestamp":rfc3339(ts),"tokens":{"input":2,"output":3}}),
            ],
        );
        let view = CostEngine::new(home.path()).refresh().unwrap();
        assert_eq!(view.scanned_files, 1);
        assert_eq!(view.all_time.requests, 2);
        assert_eq!(view.all_time.tokens, 145);
        assert_eq!(view.unpriced_events, 1);
        assert!(view.all_time.priced_cost_micros > 0);
    }

    #[test]
    fn gemini_pro_threshold_uses_above_threshold_rate() {
        let event = UsageEvent {
            tool: ToolType::Gemini,
            date: now_unix() - 1.0,
            model: "gemini-2.5-pro".into(),
            input: 200_001,
            cache_read: 0,
            output: 0,
            cache_creation: 0,
            service_tier: None,
            session_id: None,
            message_id: None,
            request_id: None,
            is_sidechain: false,
            is_parent_path: true,
            source_key: String::new(),
        };
        assert_eq!(priced_cost_micros(&event), Some(250_003));
    }

    #[test]
    fn gemini_invalid_timestamp_uses_mtime_and_telemetry_is_not_scanned() {
        let home = tempfile::tempdir().unwrap();
        let chat = home.path().join(".gemini/tmp/p/chats/session-one.jsonl");
        fs::create_dir_all(chat.parent().unwrap()).unwrap();
        write_jsonl(
            &chat,
            &[
                serde_json::json!({"type":"gemini","model":"gemini-3-flash","timestamp":"bad","tokens":{"input":10,"output":5}}),
            ],
        );
        fs::write(
            home.path().join(".gemini/telemetry.log"),
            serde_json::json!({"type":"gemini","tokens":{"input":999,"output":999}}).to_string(),
        )
        .unwrap();
        let view = CostEngine::new(home.path()).refresh().unwrap();
        assert_eq!(view.scanned_files, 1);
        assert_eq!(view.all_time.requests, 1);
    }

    #[test]
    fn gemini_chat_file_budget_and_symlinks_are_safe() {
        let home = tempfile::tempdir().unwrap();
        let chats = home.path().join(".gemini/tmp/p/chats");
        fs::create_dir_all(&chats).unwrap();
        fs::write(
            chats.join("session-large.jsonl"),
            vec![b'x'; (MAX_FILE_BYTES + 1) as usize],
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            chats.join("session-large.jsonl"),
            chats.join("session-link.jsonl"),
        )
        .unwrap();
        let (files, truncated) = collect_gemini_chat_files(home.path());
        assert_eq!(files.len(), 1);
        assert!(!truncated);
        let view = CostEngine::new(home.path()).refresh().unwrap();
        assert_eq!(view.all_time.requests, 0);
    }

    #[cfg(unix)]
    #[test]
    fn gemini_symlinked_ancestor_is_not_followed() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let chats = outside.path().join("p/chats");
        fs::create_dir_all(&chats).unwrap();
        fs::write(chats.join("session-outside.jsonl"), "{}\n").unwrap();
        fs::create_dir_all(home.path().join(".gemini")).unwrap();
        std::os::unix::fs::symlink(outside.path(), home.path().join(".gemini/tmp")).unwrap();
        let (files, truncated) = collect_gemini_chat_files(home.path());
        assert!(files.is_empty());
        assert!(!truncated);
    }
}
