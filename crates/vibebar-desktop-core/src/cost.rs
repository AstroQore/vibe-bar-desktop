//! Read-only local Codex, Claude, and Gemini CLI usage/cost.
//!
//! This first slice scans bounded JSONL inputs and keeps only an in-memory
//! snapshot. It never reads provider credentials and never writes any shared
//! Vibe Bar cost, history, pricing, or ledger store.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions as CapOpenOptions};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::ToolType;

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_READ_BYTES: u64 = 256 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 1024 * 1024;
const MAX_RETAINED_MODEL_BYTES: usize = 256;
const MAX_MODEL_GROUPS_PER_HARNESS: usize = 512;
const OTHER_MODELS_LABEL: &str = "other-models";
const MAX_RAW_EVENTS: usize = 400_000;
const MAX_FILES: usize = 20_000;
const MAX_FILES_PER_PROVIDER: usize = MAX_FILES / 3;
const MAX_DISCOVERED_PER_PROVIDER: usize = MAX_FILES_PER_PROVIDER * 10;
const MAX_DISCOVERY_ENTRIES: usize = 200_000;
const MAX_DISCOVERY_DEPTH: usize = 32;
pub(crate) const PRICING_VERSION: &str = "native-2026-06-08-v5";

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
    pub harness: String,
    pub model: String,
    pub priced_cost_micros: i64,
    pub tokens: u64,
    pub requests: u64,
    pub unpriced_events: u64,
}

/// One public row from the exact static table used by Desktop's cost scan.
/// Rates use the same unit as the native `pricing.effective` surface: USD per
/// one million tokens.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveModelPricingRow {
    pub provider: ToolType,
    pub company: &'static str,
    pub sub_provider: &'static str,
    pub model: &'static str,
    pub display_label: Option<&'static str>,
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_read_per_million: Option<f64>,
    pub cache_write_per_million: Option<f64>,
    pub threshold_tokens: Option<u64>,
    pub input_above_threshold_per_million: Option<f64>,
    pub output_above_threshold_per_million: Option<f64>,
    pub cache_read_above_threshold_per_million: Option<f64>,
    pub cache_write_above_threshold_per_million: Option<f64>,
    pub fast_multiplier: Option<f64>,
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
        let (codex_files, codex_truncated) = collect_provider_files(
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
        let (gemini_files, gemini_truncated) = collect_gemini_chat_files(&self.home);
        let mut scan = scan_sources(
            &home,
            &self.home,
            [codex_files, claude_files, gemini_files],
            MAX_RAW_EVENTS,
            MAX_TOTAL_READ_BYTES,
        );
        scan.truncated |= codex_truncated || claude_truncated || gemini_truncated;
        let events = deduplicate_events(scan.events);

        let view = aggregate(
            &events,
            scan.scanned_files,
            scan.malformed_lines,
            scan.truncated,
            now_unix(),
        );
        if let Ok(mut cached) = self.cached.write() {
            *cached = view.clone();
        }
        if !self.is_demo && !view.truncated {
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
    mtime: SystemTime,
}

#[derive(Debug, Clone)]
struct UsageEvent {
    tool: ToolType,
    date: f64,
    model: String,
    input: u64,
    cache_read: u64,
    output: u64,
    cache_creation_5m: u64,
    cache_creation_1h: u64,
    service_tier: Option<String>,
    session_id: Option<String>,
    message_id: Option<String>,
    request_id: Option<String>,
    is_sidechain: bool,
    is_parent_path: bool,
    source_key: Arc<str>,
}

impl UsageEvent {
    fn tokens(&self) -> u64 {
        self.input
            .saturating_add(self.cache_read)
            .saturating_add(self.output)
            .saturating_add(self.cache_creation_5m)
            .saturating_add(self.cache_creation_1h)
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
        collect_jsonl(&home.join(root), tool, &mut files, 0, true, &mut budget);
        if budget.stop {
            break;
        }
    }
    let discovery_truncated = budget.incomplete;
    files.sort_by(|left, right| {
        right
            .mtime
            .cmp(&left.mtime)
            .then_with(|| left.path.cmp(&right.path))
    });
    let truncated = discovery_truncated || files.len() > limit;
    files.truncate(limit);
    (files, truncated)
}

fn collect_gemini_chat_files(home: &Path) -> (Vec<SourceFile>, bool) {
    let gemini = home.join(".gemini");
    let tmp = gemini.join("tmp");
    for directory in [&gemini, &tmp] {
        match checked_directory(directory) {
            DirectoryState::Ready => {}
            DirectoryState::Missing => return (Vec::new(), false),
            DirectoryState::Unusable => return (Vec::new(), true),
        }
    }
    let projects = match fs::read_dir(tmp) {
        Ok(projects) => projects,
        Err(_) => return (Vec::new(), true),
    };
    let mut files = Vec::new();
    let mut truncated = false;
    let mut budget = DiscoveryBudget::default();
    for project in projects {
        let project = match project {
            Ok(project) => project,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        budget.entries += 1;
        if budget.entries > MAX_DISCOVERY_ENTRIES {
            truncated = true;
            break;
        }
        let kind = match project.file_type() {
            Ok(kind) => kind,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        if kind.is_symlink() {
            truncated = true;
            continue;
        }
        if !kind.is_dir() {
            continue;
        }
        let chats = project.path().join("chats");
        match checked_directory(&chats) {
            DirectoryState::Ready => {}
            DirectoryState::Missing => continue,
            DirectoryState::Unusable => {
                truncated = true;
                continue;
            }
        }
        let entries = match fs::read_dir(chats) {
            Ok(entries) => entries,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    truncated = true;
                    continue;
                }
            };
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
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(_) => {
                    truncated = true;
                    continue;
                }
            };
            if kind.is_symlink() {
                truncated = true;
                continue;
            }
            let name_ok = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("session-"));
            if !kind.is_file()
                || !name_ok
                || path.extension().and_then(|e| e.to_str()) != Some("jsonl")
            {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => {
                    truncated = true;
                    continue;
                }
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                truncated = true;
                continue;
            }
            files.push(SourceFile {
                tool: ToolType::Gemini,
                path,
                mtime: metadata.modified().unwrap_or(UNIX_EPOCH),
            });
        }
        if truncated {
            break;
        }
    }
    files.sort_by(|left, right| {
        right
            .mtime
            .cmp(&left.mtime)
            .then_with(|| left.path.cmp(&right.path))
    });
    (files, truncated)
}

enum DirectoryState {
    Ready,
    Missing,
    Unusable,
}

fn checked_directory(path: &Path) -> DirectoryState {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_dir() => {
            DirectoryState::Ready
        }
        Ok(_) => DirectoryState::Unusable,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DirectoryState::Missing,
        Err(_) => DirectoryState::Unusable,
    }
}

fn interleave_provider_files<const N: usize>(groups: [Vec<SourceFile>; N]) -> Vec<SourceFile> {
    let mut files = Vec::with_capacity(groups.iter().map(Vec::len).sum());
    let mut iterators = groups.map(Vec::into_iter);
    loop {
        let before = files.len();
        for iterator in &mut iterators {
            if let Some(file) = iterator.next() {
                files.push(file);
            }
        }
        if files.len() == before {
            return files;
        }
    }
}

fn provider_event_limits<const N: usize>(
    groups: &[Vec<SourceFile>; N],
    max_events: usize,
) -> HashMap<ToolType, usize> {
    let mut providers = Vec::new();
    for tool in groups
        .iter()
        .filter_map(|files| files.first().map(|source| source.tool))
    {
        if !providers.contains(&tool) {
            providers.push(tool);
        }
    }
    let mut limits = HashMap::new();
    if providers.is_empty() {
        return limits;
    }
    let base = max_events / providers.len();
    let remainder = max_events % providers.len();
    for (index, tool) in providers.into_iter().enumerate() {
        limits.insert(tool, base + usize::from(index < remainder));
    }
    limits
}

#[derive(Default)]
struct RawScan {
    events: Vec<UsageEvent>,
    scanned_files: u64,
    malformed_lines: u64,
    truncated: bool,
}

fn scan_sources<const N: usize>(
    home: &Dir,
    home_path: &Path,
    groups: [Vec<SourceFile>; N],
    max_events: usize,
    max_read_bytes: u64,
) -> RawScan {
    let provider_limits = provider_event_limits(&groups, max_events);
    let files = interleave_provider_files(groups);
    let mut provider_counts = HashMap::<ToolType, usize>::new();
    let mut remaining_bytes = max_read_bytes;
    let mut scan = RawScan::default();

    for source in &files {
        if scan.events.len() >= max_events || remaining_bytes == 0 {
            scan.truncated = true;
            break;
        }
        let provider_limit = provider_limits.get(&source.tool).copied().unwrap_or(0);
        let provider_count = provider_counts.get(&source.tool).copied().unwrap_or(0);
        if provider_count >= provider_limit {
            scan.truncated = true;
            continue;
        }
        let event_limit = scan
            .events
            .len()
            .saturating_add(provider_limit - provider_count)
            .min(max_events);
        let before = scan.events.len();
        let result = scan_file(
            home,
            home_path,
            source,
            &mut scan.events,
            &mut scan.malformed_lines,
            event_limit,
            &mut remaining_bytes,
        );
        *provider_counts.entry(source.tool).or_default() += scan.events.len() - before;
        match result {
            ScanFileResult::Scanned => scan.scanned_files += 1,
            ScanFileResult::Skipped | ScanFileResult::TooLarge => scan.truncated = true,
            ScanFileResult::EventLimit => scan.truncated = true,
            ScanFileResult::ByteLimit => {
                scan.truncated = true;
                break;
            }
        }
        if remaining_bytes == 0 {
            scan.truncated = true;
            break;
        }
    }
    if scan.malformed_lines > 0 {
        scan.truncated = true;
    }
    scan
}

#[derive(Default)]
struct DiscoveryBudget {
    entries: usize,
    incomplete: bool,
    stop: bool,
}

impl DiscoveryBudget {
    fn mark_incomplete(&mut self) {
        self.incomplete = true;
    }

    fn exhaust(&mut self) {
        self.mark_incomplete();
        self.stop = true;
    }
}

fn collect_jsonl(
    root: &Path,
    tool: ToolType,
    out: &mut Vec<SourceFile>,
    depth: usize,
    optional_root: bool,
    budget: &mut DiscoveryBudget,
) {
    if depth > MAX_DISCOVERY_DEPTH
        || out.len() >= MAX_DISCOVERED_PER_PROVIDER
        || budget.entries >= MAX_DISCOVERY_ENTRIES
    {
        budget.exhaust();
        return;
    }
    let root_metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) => {
            mark_discovery_error(&error, optional_root, budget);
            return;
        }
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        budget.mark_incomplete();
        return;
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            mark_discovery_error(&error, false, budget);
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                mark_discovery_error(&error, false, budget);
                continue;
            }
        };
        budget.entries += 1;
        if out.len() >= MAX_DISCOVERED_PER_PROVIDER || budget.entries > MAX_DISCOVERY_ENTRIES {
            budget.exhaust();
            return;
        }
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                mark_discovery_error(&error, false, budget);
                continue;
            }
        };
        if file_type.is_symlink() {
            budget.mark_incomplete();
            continue;
        }
        if file_type.is_dir() {
            collect_jsonl(&path, tool, out, depth + 1, false, budget);
            if budget.stop {
                return;
            }
        } else if file_type.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
        {
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    mark_discovery_error(&error, false, budget);
                    continue;
                }
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                budget.mark_incomplete();
                continue;
            }
            out.push(SourceFile {
                tool,
                path,
                mtime: metadata.modified().unwrap_or(UNIX_EPOCH),
            });
        }
    }
}

fn mark_discovery_error(
    error: &std::io::Error,
    optional_root: bool,
    budget: &mut DiscoveryBudget,
) {
    if error.kind() != std::io::ErrorKind::NotFound || !optional_root {
        budget.mark_incomplete();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanFileResult {
    Scanned,
    Skipped,
    TooLarge,
    EventLimit,
    ByteLimit,
}

fn scan_file(
    home: &Dir,
    home_path: &Path,
    source: &SourceFile,
    events: &mut Vec<UsageEvent>,
    malformed_lines: &mut u64,
    event_limit: usize,
    remaining_bytes: &mut u64,
) -> ScanFileResult {
    if *remaining_bytes == 0 {
        return ScanFileResult::ByteLimit;
    }
    let Ok(metadata) = fs::symlink_metadata(&source.path) else {
        return ScanFileResult::Skipped;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return ScanFileResult::Skipped;
    }
    if metadata.len() > MAX_FILE_BYTES {
        return ScanFileResult::TooLarge;
    }
    if metadata.len() > *remaining_bytes {
        return ScanFileResult::ByteLimit;
    }
    let Ok(file) = open_source_file(home, home_path, source) else {
        return ScanFileResult::Skipped;
    };
    let Ok(open_metadata) = file.metadata() else {
        return ScanFileResult::Skipped;
    };
    if !open_metadata.is_file() {
        return ScanFileResult::Skipped;
    }
    if open_metadata.len() > MAX_FILE_BYTES {
        return ScanFileResult::TooLarge;
    }
    if open_metadata.len() > *remaining_bytes {
        return ScanFileResult::ByteLimit;
    }
    let gemini_fallback_time = if source.tool == ToolType::Gemini {
        open_metadata
            .modified()
            .ok()
            .map(|modified| modified.into_std())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs_f64())
    } else {
        None
    };
    let mut bytes = Vec::with_capacity(open_metadata.len() as usize);
    let read_result = file
        .take((MAX_FILE_BYTES + 1).min(*remaining_bytes))
        .read_to_end(&mut bytes);
    *remaining_bytes = remaining_bytes.saturating_sub(bytes.len() as u64);
    if read_result.is_err() {
        return ScanFileResult::Skipped;
    }
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return ScanFileResult::TooLarge;
    }

    let mut codex_previous = (0_u64, 0_u64, 0_u64);
    let mut codex_model = "codex-unknown".to_string();
    let source_key: Arc<str> = source.path.to_string_lossy().into_owned().into();
    let mut gemini_session_id = None;
    let capacity = event_limit.saturating_sub(events.len());
    let mut retained_events = VecDeque::new();
    let mut parsed_events = Vec::with_capacity(1);
    let mut dropped_events = false;
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
        parsed_events.clear();
        match source.tool {
            ToolType::Codex => parse_codex(
                &value,
                &mut codex_model,
                &mut codex_previous,
                &source_key,
                &mut parsed_events,
            ),
            ToolType::Claude => {
                parse_claude(&value, &source.path, &source_key, &mut parsed_events)
            }
            ToolType::Gemini => parse_gemini(
                &value,
                gemini_fallback_time,
                &source_key,
                &mut gemini_session_id,
                &mut parsed_events,
            ),
            _ => {}
        }
        for event in parsed_events.drain(..) {
            if retained_events.len() == capacity {
                retained_events.pop_front();
                dropped_events = true;
            }
            if capacity > 0 {
                retained_events.push_back(event);
            }
        }
    }
    events.extend(retained_events);
    if dropped_events {
        ScanFileResult::EventLimit
    } else {
        ScanFileResult::Scanned
    }
}

fn parse_gemini(
    value: &Value,
    fallback_time: Option<f64>,
    source_key: &Arc<str>,
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
        cache_creation_5m: 0,
        cache_creation_1h: 0,
        service_tier: None,
        session_id: session_id.clone(),
        message_id: value.get("id").and_then(Value::as_str).map(str::to_string),
        request_id: None,
        is_sidechain: false,
        is_parent_path: true,
        source_key: Arc::clone(source_key),
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
    current_model: &mut String,
    previous: &mut (u64, u64, u64),
    source_key: &Arc<str>,
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
    {
        let model = model.trim();
        if !model.is_empty() {
            *current_model = if model.len() <= MAX_RETAINED_MODEL_BYTES {
                model.to_string()
            } else {
                "codex-unknown".to_string()
            };
        }
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
    let Some(date) = timestamp(value) else {
        return;
    };
    events.push(UsageEvent {
        tool: ToolType::Codex,
        date,
        model: current_model.clone(),
        input,
        cache_read: cached,
        output,
        cache_creation_5m: 0,
        cache_creation_1h: 0,
        service_tier: explicit_service_tier(value, payload, info),
        session_id: None,
        message_id: None,
        request_id: None,
        is_sidechain: false,
        is_parent_path: true,
        source_key: Arc::clone(source_key),
    });
}

fn explicit_service_tier(value: &Value, payload: &Value, info: &Value) -> Option<String> {
    [value, payload, info]
        .into_iter()
        .find_map(|object| {
            object
                .get("speed")
                .or_else(|| object.get("service_tier"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|tier| !tier.is_empty())
        .map(str::to_string)
}

fn parse_claude(
    value: &Value,
    source_path: &Path,
    source_key: &Arc<str>,
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
    let (cache_creation_5m, cache_creation_1h) = claude_cache_creation(usage);
    let output = number(usage, "output_tokens");
    if input == 0
        && cache_read == 0
        && cache_creation_5m == 0
        && cache_creation_1h == 0
        && output == 0
    {
        return;
    }
    let Some(date) = timestamp(value) else {
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
            .unwrap_or("claude-unknown")
            .to_string(),
        input,
        cache_read,
        output,
        cache_creation_5m,
        cache_creation_1h,
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
        source_key: Arc::clone(source_key),
    });
}

fn claude_cache_creation(usage: &Value) -> (u64, u64) {
    let total = number(usage, "cache_creation_input_tokens");
    let Some(breakdown) = usage
        .get("cache_creation")
        .filter(|value| value.is_object())
    else {
        return (total, 0);
    };
    let five_minutes = number(breakdown, "ephemeral_5m_input_tokens");
    let one_hour = number(breakdown, "ephemeral_1h_input_tokens");
    let breakdown_total = five_minutes.saturating_add(one_hour);
    if breakdown_total > 0 && (total == 0 || breakdown_total == total) {
        (five_minutes, one_hour)
    } else {
        (total, 0)
    }
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

    const fn above_threshold(
        mut self,
        threshold: u64,
        input: f64,
        output: f64,
        cache_read: f64,
        cache_creation: f64,
    ) -> Self {
        self.threshold = Some(threshold);
        self.input_above = Some(input);
        self.output_above = Some(output);
        self.cache_read_above = Some(cache_read);
        self.cache_creation_above = Some(cache_creation);
        self
    }

    const fn fast(mut self, multiplier: f64) -> Self {
        self.fast_multiplier = Some(multiplier);
        self
    }
}

type PricingEntry = (&'static str, ModelPricing);

const CODEX_STANDARD: ModelPricing = ModelPricing::simple(1.25, 10.0, Some(0.125));
const CODEX_MINI: ModelPricing = ModelPricing::simple(0.25, 2.0, Some(0.025));
const CODEX_52: ModelPricing = ModelPricing::simple(1.75, 14.0, Some(0.175));
const CODEX_PRO: ModelPricing = ModelPricing::simple(30.0, 180.0, None);
const CLAUDE_HAIKU: ModelPricing = ModelPricing::claude(1.0, 5.0, 1.25, 0.1);
const CLAUDE_OPUS: ModelPricing = ModelPricing::claude(5.0, 25.0, 6.25, 0.5);
const CLAUDE_SONNET: ModelPricing = ModelPricing::claude(3.0, 15.0, 3.75, 0.3);
const CLAUDE_LEGACY_OPUS: ModelPricing = ModelPricing::claude(15.0, 75.0, 18.75, 1.5);

const CODEX_PRICES: &[PricingEntry] = &[
    ("gpt-5", CODEX_STANDARD),
    ("gpt-5-codex", CODEX_STANDARD),
    ("gpt-5-mini", CODEX_MINI),
    ("gpt-5-nano", ModelPricing::simple(0.05, 0.4, Some(0.005))),
    ("gpt-5-pro", ModelPricing::simple(15.0, 120.0, None)),
    ("gpt-5.1", CODEX_STANDARD),
    ("gpt-5.1-codex", CODEX_STANDARD),
    ("gpt-5.1-codex-max", CODEX_STANDARD),
    ("gpt-5.1-codex-mini", CODEX_MINI),
    ("gpt-5.2", CODEX_52),
    ("gpt-5.2-codex", CODEX_52),
    ("gpt-5.2-pro", ModelPricing::simple(21.0, 168.0, None)),
    ("gpt-5.3-codex", CODEX_52.fast(2.0)),
    (
        "gpt-5.3-codex-spark",
        ModelPricing::simple(0.0, 0.0, Some(0.0)),
    ),
    (
        "gpt-5.4",
        ModelPricing::simple(2.5, 15.0, Some(0.25)).fast(2.0),
    ),
    ("gpt-5.4-mini", ModelPricing::simple(0.75, 4.5, Some(0.075))),
    ("gpt-5.4-nano", ModelPricing::simple(0.2, 1.25, Some(0.02))),
    ("gpt-5.4-pro", CODEX_PRO),
    (
        "gpt-5.5",
        ModelPricing::simple(5.0, 30.0, Some(0.5)).fast(2.5),
    ),
    ("gpt-5.5-pro", CODEX_PRO),
];

const CLAUDE_PRICES: &[PricingEntry] = &[
    ("claude-haiku-4-5", CLAUDE_HAIKU),
    ("claude-haiku-4-5-20251001", CLAUDE_HAIKU),
    ("claude-opus-4-1", CLAUDE_LEGACY_OPUS),
    ("claude-opus-4-20250514", CLAUDE_LEGACY_OPUS),
    ("claude-opus-4-5", CLAUDE_OPUS),
    ("claude-opus-4-5-20251101", CLAUDE_OPUS),
    ("claude-opus-4-6", CLAUDE_OPUS.fast(6.0)),
    ("claude-opus-4-6-20260205", CLAUDE_OPUS.fast(6.0)),
    ("claude-opus-4-7", CLAUDE_OPUS.fast(6.0)),
    ("claude-opus-4-8", CLAUDE_OPUS.fast(2.0)),
    (
        "claude-sonnet-4-20250514",
        CLAUDE_SONNET.above_threshold(200_000, 6.0, 22.5, 0.6, 7.5),
    ),
    (
        "claude-sonnet-4-5",
        CLAUDE_SONNET.above_threshold(200_000, 6.0, 22.5, 0.6, 7.5),
    ),
    (
        "claude-sonnet-4-5-20250929",
        CLAUDE_SONNET.above_threshold(200_000, 6.0, 22.5, 0.6, 7.5),
    ),
    ("claude-sonnet-4-6", CLAUDE_SONNET),
];

const GEMINI_PRICES: &[PricingEntry] = &[
    (
        "gemini-2.5-pro",
        ModelPricing::simple(1.25, 10.0, Some(0.31))
            .above_threshold(200_000, 2.5, 15.0, 0.625, 2.5),
    ),
    ("gemini-2.5-flash", ModelPricing::simple(0.3, 2.5, Some(0.075))),
    (
        "gemini-2.5-flash-lite",
        ModelPricing::simple(0.1, 0.4, Some(0.025)),
    ),
    (
        "gemini-3-pro",
        ModelPricing::simple(2.0, 12.0, Some(0.5))
            .above_threshold(200_000, 4.0, 18.0, 1.0, 4.0),
    ),
    (
        "gemini-3-pro-preview",
        ModelPricing::simple(2.0, 12.0, Some(0.5))
            .above_threshold(200_000, 4.0, 18.0, 1.0, 4.0),
    ),
    ("gemini-3-flash", ModelPricing::simple(0.35, 2.8, Some(0.0875))),
    (
        "gemini-3-flash-lite",
        ModelPricing::simple(0.125, 0.5, Some(0.031)),
    ),
];

/// Desktop has no pricing cache, remote merge, or user overrides yet. This
/// therefore exposes only the public static rows that `priced_cost_micros`
/// can actually select.
pub fn effective_model_prices() -> Vec<EffectiveModelPricingRow> {
    CODEX_PRICES
        .iter()
        .map(|entry| public_pricing_row(ToolType::Codex, entry))
        .chain(
            CLAUDE_PRICES
                .iter()
                .map(|entry| public_pricing_row(ToolType::Claude, entry)),
        )
        .chain(
            GEMINI_PRICES
                .iter()
                .map(|entry| public_pricing_row(ToolType::Gemini, entry)),
        )
        .collect()
}

fn public_pricing_row(
    provider: ToolType,
    (model, pricing): &PricingEntry,
) -> EffectiveModelPricingRow {
    let hierarchy = provider.hierarchy();
    EffectiveModelPricingRow {
        provider,
        company: hierarchy.vendor,
        sub_provider: hierarchy.product,
        model,
        display_label: (*model == "gpt-5.3-codex-spark").then_some("Research Preview"),
        input_per_million: pricing.input,
        output_per_million: pricing.output,
        cache_read_per_million: pricing.cache_read,
        cache_write_per_million: pricing.cache_creation,
        threshold_tokens: pricing.threshold,
        input_above_threshold_per_million: pricing.input_above,
        output_above_threshold_per_million: pricing.output_above,
        cache_read_above_threshold_per_million: pricing.cache_read_above,
        cache_write_above_threshold_per_million: pricing.cache_creation_above,
        fast_multiplier: pricing.fast_multiplier,
    }
}

fn priced_cost_micros(event: &UsageEvent) -> Option<i64> {
    let pricing = match event.tool {
        ToolType::Codex => codex_pricing(&event.model)?,
        ToolType::Claude => claude_pricing(&event.model)?,
        ToolType::Gemini => gemini_pricing(&event.model)?,
        _ => return None,
    };
    let above_threshold = pricing.threshold.is_some_and(|threshold| {
        event
            .input
            .saturating_add(event.cache_read)
            .saturating_add(event.cache_creation_5m)
            .saturating_add(event.cache_creation_1h)
            > threshold
    });
    let rate = |base: f64, above: Option<f64>| {
        if above_threshold {
            above.unwrap_or(base)
        } else {
            base
        }
    };
    let mut micros = event.input as f64 * rate(pricing.input, pricing.input_above)
        + event.output as f64 * rate(pricing.output, pricing.output_above)
        + event.cache_read as f64
            * rate(
                pricing.cache_read.unwrap_or(pricing.input),
                pricing.cache_read_above.or(pricing.input_above),
            )
        + event.cache_creation_5m as f64
            * rate(
                pricing.cache_creation.unwrap_or(pricing.input),
                pricing.cache_creation_above.or(pricing.input_above),
            )
        + event.cache_creation_1h as f64 * 2.0 * rate(pricing.input, pricing.input_above);
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
    GEMINI_PRICES
        .iter()
        .find(|(candidate, _)| *candidate == raw.trim())
        .map(|(_, pricing)| *pricing)
}

fn codex_pricing(raw: &str) -> Option<ModelPricing> {
    let model = raw.trim().strip_prefix("openai/").unwrap_or(raw.trim());
    codex_pricing_exact(model)
        .or_else(|| strip_codex_date_suffix(model).and_then(codex_pricing_exact))
}

fn codex_pricing_exact(model: &str) -> Option<ModelPricing> {
    CODEX_PRICES
        .iter()
        .find(|(candidate, _)| *candidate == model)
        .map(|(_, pricing)| *pricing)
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
    CLAUDE_PRICES
        .iter()
        .find(|(candidate, _)| *candidate == model)
        .map(|(_, pricing)| *pricing)
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
        "claude-haiku-4-5"
            | "claude-opus-4-1"
            | "claude-opus-4-5"
            | "claude-opus-4-6"
            | "claude-sonnet-4-5"
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
    let mut models: HashMap<ToolType, HashMap<String, ModelCost>> = HashMap::new();
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

        let models_for_harness = models.entry(event.tool).or_default();
        let model_label = if event.model.len() <= MAX_RETAINED_MODEL_BYTES {
            event.model.as_str()
        } else {
            OTHER_MODELS_LABEL
        };
        let model_label = if model_label == OTHER_MODELS_LABEL
            || !models_for_harness.contains_key(model_label)
                && models_for_harness.len() >= MAX_MODEL_GROUPS_PER_HARNESS - 1
        {
            OTHER_MODELS_LABEL
        } else {
            model_label
        };
        let model = models_for_harness
            .entry(model_label.to_string())
            .or_insert_with(|| ModelCost {
                harness: harness_name(event.tool).to_string(),
                model: model_label.to_string(),
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

    let mut models = models
        .into_values()
        .flat_map(HashMap::into_values)
        .collect::<Vec<_>>();
    models.sort_by(|left, right| {
        right
            .priced_cost_micros
            .cmp(&left.priced_cost_micros)
            .then_with(|| right.tokens.cmp(&left.tokens))
            .then_with(|| left.harness.cmp(&right.harness))
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

pub(crate) fn is_same_local_day(left: f64, right: f64) -> bool {
    local_day(left)
        .zip(local_day(right))
        .is_some_and(|(left, right)| left == right)
}

fn add_totals(totals: &mut CostTotals, tokens: u64, cost: Option<i64>) {
    totals.tokens = totals.tokens.saturating_add(tokens);
    totals.requests = totals.requests.saturating_add(1);
    if let Some(cost) = cost {
        totals.priced_cost_micros = totals.priced_cost_micros.saturating_add(cost);
    }
}

fn harness_name(tool: ToolType) -> &'static str {
    match tool {
        ToolType::Codex => "Codex",
        ToolType::Claude => "Claude Code",
        ToolType::Gemini => "Gemini CLI",
        _ => tool.raw_value(),
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
    fn scans_codex_deltas_and_uses_only_event_level_service_tier() {
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
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-5.0),"payload":{"type":"token_count","service_tier":"priority","info":{"total_token_usage":{"input_tokens":150,"cached_input_tokens":30,"output_tokens":50}}}}),
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
        assert_eq!(view.models[0].harness, "Codex");
        assert_eq!(view.unpriced_events, 0);
        // The first request is standard despite the current config; only the
        // second request carries an explicit priority marker in its event.
        assert_eq!(view.all_time.priced_cost_micros, 1_460);
    }

    #[test]
    fn codex_without_an_explicit_model_stays_unpriced() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        write_jsonl(
            &home.path().join(".codex/sessions/session.jsonl"),
            &[serde_json::json!({
                "type":"event_msg","timestamp":rfc3339(scanned_at-1.0),
                "payload":{"type":"token_count","info":{"total_token_usage":{
                    "input_tokens":7,"output_tokens":3
                }}}
            })],
        );

        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert_eq!(view.all_time.requests, 1);
        assert_eq!(view.all_time.tokens, 10);
        assert_eq!(view.unpriced_events, 1);
        assert_eq!(view.models[0].model, "codex-unknown");
        assert_eq!(view.models[0].priced_cost_micros, 0);
    }

    #[test]
    fn oversized_codex_model_is_not_retained_for_each_event() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        let oversized_model = "x".repeat(MAX_LINE_BYTES - 1_024);
        write_jsonl(
            &home.path().join(".codex/sessions/session.jsonl"),
            &[
                serde_json::json!({"type":"turn_context","payload":{"model":oversized_model}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-3.0),"payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1}}}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-2.0),"payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1}}}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-1.0),"payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1}}}}),
            ],
        );

        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert_eq!(view.all_time.requests, 3);
        assert_eq!(view.unpriced_events, 3);
        assert_eq!(view.models.len(), 1);
        assert_eq!(view.models[0].model, "codex-unknown");
        assert!(view.models[0].model.len() <= MAX_RETAINED_MODEL_BYTES);
    }

    #[test]
    fn model_groups_are_bounded_and_overflow_totals_are_retained() {
        let scanned_at = now_unix();
        let event = |model| UsageEvent {
            tool: ToolType::Claude,
            date: scanned_at - 1.0,
            model,
            input: 1,
            cache_read: 0,
            output: 0,
            cache_creation_5m: 0,
            cache_creation_1h: 0,
            service_tier: None,
            session_id: None,
            message_id: None,
            request_id: None,
            is_sidechain: false,
            is_parent_path: true,
            source_key: Arc::from(""),
        };
        let mut events = (0..MAX_MODEL_GROUPS_PER_HARNESS + 2)
            .map(|index| event(format!("custom-model-{index}")))
            .collect::<Vec<_>>();
        events.push(event("x".repeat(MAX_RETAINED_MODEL_BYTES + 1)));

        let view = aggregate(&events, 1, 0, false, scanned_at);
        assert_eq!(view.models.len(), MAX_MODEL_GROUPS_PER_HARNESS);
        assert!(view
            .models
            .iter()
            .all(|model| model.model.len() <= MAX_RETAINED_MODEL_BYTES));
        let overflow = view
            .models
            .iter()
            .find(|model| model.harness == "Claude Code" && model.model == OTHER_MODELS_LABEL)
            .unwrap();
        assert_eq!(overflow.requests, 4);
        assert_eq!(overflow.tokens, 4);
        assert_eq!(overflow.unpriced_events, 4);
        assert_eq!(view.all_time.requests, events.len() as u64);
        assert_eq!(view.all_time.tokens, events.len() as u64);
        assert_eq!(view.today.requests, events.len() as u64);
        assert_eq!(view.daily[0].requests, events.len() as u64);
        assert_eq!(view.unpriced_events, events.len() as u64);
    }

    #[test]
    fn codex_and_claude_skip_missing_or_invalid_record_timestamps() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        write_jsonl(
            &home.path().join(".codex/sessions/session.jsonl"),
            &[
                serde_json::json!({"type":"turn_context","payload":{"model":"gpt-5"}}),
                serde_json::json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10}}}}),
                serde_json::json!({"type":"event_msg","timestamp":"invalid","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20}}}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-1.0),"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":30}}}}),
            ],
        );
        write_jsonl(
            &home.path().join(".claude/projects/project/session.jsonl"),
            &[
                serde_json::json!({"type":"assistant","message":{"model":"claude-haiku-4-5","usage":{"input_tokens":100}}}),
                serde_json::json!({"type":"assistant","timestamp":"invalid","message":{"model":"claude-haiku-4-5","usage":{"output_tokens":100}}}),
                serde_json::json!({"type":"assistant","timestamp":rfc3339(scanned_at-1.0),"message":{"model":"claude-haiku-4-5","usage":{"input_tokens":2}}}),
            ],
        );

        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert_eq!(view.scanned_files, 2);
        assert_eq!(view.all_time.requests, 2);
        assert_eq!(view.all_time.tokens, 12);
        assert_eq!(view.malformed_lines, 0);
        assert!(!view.truncated);
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
        assert_eq!(opus.harness, "Claude Code");
        assert_eq!(opus.priced_cost_micros, 180);
    }

    #[test]
    fn claude_cache_creation_breakdown_prices_one_hour_at_double_input() {
        let value = serde_json::json!({
            "type":"assistant","timestamp":rfc3339(now_unix()-1.0),
            "message":{"model":"claude-haiku-4-5","usage":{
                "input_tokens":10,"cache_read_input_tokens":10,
                "cache_creation_input_tokens":14,"output_tokens":4,
                "cache_creation":{
                    "ephemeral_5m_input_tokens":8,
                    "ephemeral_1h_input_tokens":6
                }
            }}
        });
        let mut events = Vec::new();
        let source_path = Path::new("/Users/example/.claude/projects/session.jsonl");
        let source_key: Arc<str> = source_path.to_string_lossy().into_owned().into();
        parse_claude(
            &value,
            source_path,
            &source_key,
            &mut events,
        );

        assert_eq!(events.len(), 1);
        assert!(Arc::ptr_eq(&events[0].source_key, &source_key));
        assert_eq!(events[0].cache_creation_5m, 8);
        assert_eq!(events[0].cache_creation_1h, 6);
        assert_eq!(events[0].tokens(), 38);
        assert_eq!(priced_cost_micros(&events[0]), Some(53));
        assert_eq!(
            claude_cache_creation(&serde_json::json!({
                "cache_creation": {
                    "ephemeral_5m_input_tokens": 3,
                    "ephemeral_1h_input_tokens": 2
                }
            })),
            (3, 2)
        );
    }

    #[test]
    fn missing_or_empty_claude_model_stays_unpriced() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        write_jsonl(
            &home.path().join(".claude/projects/project/session.jsonl"),
            &[
                serde_json::json!({
                    "type":"assistant","timestamp":rfc3339(scanned_at-2.0),
                    "message":{"usage":{"input_tokens":2}}
                }),
                serde_json::json!({
                    "type":"assistant","timestamp":rfc3339(scanned_at-1.0),
                    "message":{"model":"  ","usage":{"output_tokens":3}}
                }),
            ],
        );

        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert_eq!(view.all_time.requests, 2);
        assert_eq!(view.all_time.tokens, 5);
        assert_eq!(view.unpriced_events, 2);
        assert_eq!(view.models.len(), 1);
        assert_eq!(view.models[0].model, "claude-unknown");
        assert_eq!(view.models[0].priced_cost_micros, 0);
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
        assert!(view.truncated);
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
            cache_creation_5m: 0,
            cache_creation_1h: 0,
            service_tier: None,
            session_id: None,
            message_id: None,
            request_id: None,
            is_sidechain: false,
            is_parent_path: true,
            source_key: Arc::from(""),
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
        assert!(!view.truncated);
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
            cache_creation_5m: 0,
            cache_creation_1h: 0,
            service_tier: None,
            session_id: None,
            message_id: None,
            request_id: None,
            is_sidechain: false,
            is_parent_path: true,
            source_key: Arc::from(""),
        };
        assert_eq!(priced_cost_micros(&event), Some(1_200_006));
    }

    #[test]
    fn claude_snapshot_suffixes_normalize_only_for_supported_aliases() {
        let supported = "claude-opus-4-1-20250805";
        assert_eq!(normalize_claude_model(supported), "claude-opus-4-1");
        assert!(claude_pricing(supported).is_some());

        let unsupported = "claude-future-9-9-20250805";
        assert_eq!(normalize_claude_model(unsupported), unsupported);
        assert!(claude_pricing(unsupported).is_none());
    }

    #[test]
    fn sonnet_split_cache_threshold_selects_one_rate_for_the_whole_request() {
        let event = |cache_read, cache_creation_5m, cache_creation_1h| UsageEvent {
            tool: ToolType::Claude,
            date: now_unix() - 1.0,
            model: "claude-sonnet-4-5".into(),
            input: 1,
            cache_read,
            output: 2,
            cache_creation_5m,
            cache_creation_1h,
            service_tier: None,
            session_id: None,
            message_id: None,
            request_id: None,
            is_sidechain: false,
            is_parent_path: true,
            source_key: Arc::from(""),
        };

        // Exactly 200k input-context tokens stays on the base rates.
        assert_eq!(
            priced_cost_micros(&event(99_999, 50_000, 50_000)),
            Some(517_533)
        );
        // Crossing the threshold via split cache usage prices every column,
        // including output, at the published above-threshold rate.
        assert_eq!(
            priced_cost_micros(&event(100_000, 50_000, 50_000)),
            Some(1_035_051)
        );
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
            cache_creation_5m: 0,
            cache_creation_1h: 0,
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
    fn provider_budgets_and_file_order_support_three_providers() {
        let source = |tool, name| SourceFile {
            tool,
            path: PathBuf::from(name),
            mtime: UNIX_EPOCH,
        };
        let groups = [
            vec![
                source(ToolType::Codex, "codex-1"),
                source(ToolType::Codex, "codex-2"),
            ],
            vec![source(ToolType::Claude, "claude-1")],
            vec![source(ToolType::Gemini, "gemini-1")],
        ];
        let limits = provider_event_limits(&groups, 5);
        assert_eq!(limits[&ToolType::Codex], 2);
        assert_eq!(limits[&ToolType::Claude], 2);
        assert_eq!(limits[&ToolType::Gemini], 1);

        let files = interleave_provider_files(groups);
        let first_round = files
            .iter()
            .take(3)
            .map(|source| source.tool)
            .collect::<Vec<_>>();
        assert_eq!(
            first_round,
            vec![ToolType::Codex, ToolType::Claude, ToolType::Gemini]
        );
        assert_eq!(files[3].path, PathBuf::from("codex-2"));
    }

    #[test]
    fn provider_event_cap_does_not_starve_claude_after_large_codex_file() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        let codex = home.path().join(".codex/sessions/session.jsonl");
        let claude = home.path().join(".claude/projects/project/session.jsonl");
        write_jsonl(
            &codex,
            &[
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-4.0),"payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1}}}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-3.0),"payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1}}}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-2.0),"payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1}}}}),
            ],
        );
        write_jsonl(
            &claude,
            &[
                serde_json::json!({"type":"assistant","timestamp":rfc3339(scanned_at-2.0),"message":{"model":"claude-haiku-4-5","usage":{"input_tokens":1}}}),
                serde_json::json!({"type":"assistant","timestamp":rfc3339(scanned_at-1.0),"message":{"model":"claude-haiku-4-5","usage":{"input_tokens":1}}}),
            ],
        );
        let directory = crate::paths::open_ambient_dir(home.path()).unwrap();
        let scan = scan_sources(
            &directory,
            home.path(),
            [
                vec![SourceFile {
                    tool: ToolType::Codex,
                    path: codex,
                    mtime: UNIX_EPOCH,
                }],
                vec![SourceFile {
                    tool: ToolType::Claude,
                    path: claude,
                    mtime: UNIX_EPOCH,
                }],
            ],
            4,
            1024 * 1024,
        );

        assert!(scan.truncated);
        assert_eq!(scan.events.len(), 4);
        assert_eq!(
            scan.events
                .iter()
                .filter(|event| event.tool == ToolType::Codex)
                .count(),
            2
        );
        assert_eq!(
            scan.events
                .iter()
                .filter(|event| event.tool == ToolType::Claude)
                .count(),
            2
        );
    }

    #[test]
    fn zero_event_byte_budget_stops_before_the_next_file() {
        let home = tempfile::tempdir().unwrap();
        let first = home.path().join(".claude/projects/first.jsonl");
        let second = home.path().join(".claude/projects/second.jsonl");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::write(&first, "     ").unwrap();
        fs::write(&second, "    ").unwrap();
        let directory = crate::paths::open_ambient_dir(home.path()).unwrap();

        let scan = scan_sources(
            &directory,
            home.path(),
            [vec![
                SourceFile {
                    tool: ToolType::Claude,
                    path: first,
                    mtime: UNIX_EPOCH,
                },
                SourceFile {
                    tool: ToolType::Claude,
                    path: second,
                    mtime: UNIX_EPOCH,
                },
            ]],
            10,
            5,
        );

        assert!(scan.truncated);
        assert_eq!(scan.scanned_files, 1);
        assert!(scan.events.is_empty());
        assert_eq!(scan.malformed_lines, 0);
    }

    #[test]
    fn newest_events_from_a_long_lived_file_fill_the_provider_capacity() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        let path = home.path().join(".codex/sessions/session.jsonl");
        write_jsonl(
            &path,
            &[
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-3.0*86_400.0),"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100}}}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-2.0),"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":102}}}}),
                serde_json::json!({"type":"event_msg","timestamp":rfc3339(scanned_at-1.0),"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":105}}}}),
            ],
        );
        let directory = crate::paths::open_ambient_dir(home.path()).unwrap();
        let scan = scan_sources(
            &directory,
            home.path(),
            [vec![SourceFile {
                tool: ToolType::Codex,
                path,
                mtime: UNIX_EPOCH,
            }]],
            2,
            1024 * 1024,
        );

        assert!(scan.truncated);
        assert_eq!(scan.events.len(), 2);
        let view = aggregate(
            &scan.events,
            scan.scanned_files,
            scan.malformed_lines,
            scan.truncated,
            scanned_at,
        );
        assert_eq!(view.today.requests, 2);
        assert_eq!(view.today.tokens, 5);
        assert_eq!(view.all_time.requests, 2);
        assert_eq!(view.all_time.tokens, 5);
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
        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert_eq!(view.scanned_files, 1);
        assert_eq!(view.all_time.requests, 2);
        assert_eq!(view.all_time.tokens, 145);
        assert_eq!(view.unpriced_events, 1);
        assert!(view.all_time.priced_cost_micros > 0);
        assert!(view.models.iter().all(|model| model.harness == "Gemini CLI"));
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
            cache_creation_5m: 0,
            cache_creation_1h: 0,
            service_tier: None,
            session_id: None,
            message_id: None,
            request_id: None,
            is_sidechain: false,
            is_parent_path: true,
            source_key: Arc::from(""),
        };
        assert_eq!(priced_cost_micros(&event), Some(500_003));
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
        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
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
        assert!(truncated);
        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert_eq!(view.all_time.requests, 0);
        assert_eq!(view.scanned_files, 0);
        assert!(view.truncated);
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
        assert!(truncated);
    }

    #[test]
    fn discovery_missing_roots_are_empty_but_other_io_errors_fail_closed() {
        let home = tempfile::tempdir().unwrap();
        let (files, truncated) =
            collect_provider_files(home.path(), ToolType::Codex, &[".codex/missing"], 1);
        assert!(files.is_empty());
        assert!(!truncated);

        let mut files = Vec::new();
        let mut budget = DiscoveryBudget::default();
        collect_jsonl(
            &home.path().join(".codex/missing"),
            ToolType::Codex,
            &mut files,
            1,
            false,
            &mut budget,
        );
        assert!(budget.incomplete);

        fs::write(home.path().join(".codex"), "not a directory").unwrap();
        let (files, truncated) =
            collect_provider_files(home.path(), ToolType::Codex, &[".codex/sessions"], 1);
        assert!(files.is_empty());
        assert!(truncated);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_discovery_root_is_truncated() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".codex")).unwrap();
        symlink(outside.path(), home.path().join(".codex/sessions")).unwrap();

        let (files, truncated) =
            collect_provider_files(home.path(), ToolType::Codex, &[".codex/sessions"], 1);
        assert!(files.is_empty());
        assert!(truncated);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_project_marks_truncated_but_keeps_a_valid_sibling() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let projects = home.path().join(".claude/projects");
        fs::create_dir_all(&projects).unwrap();
        symlink(outside.path(), projects.join("linked-project")).unwrap();
        let scanned_at = now_unix();
        write_jsonl(
            &projects.join("safe/session.jsonl"),
            &[serde_json::json!({
                "type":"assistant","timestamp":rfc3339(scanned_at-1.0),
                "message":{"model":"claude-haiku-4-5","usage":{"input_tokens":1}}
            })],
        );

        let root = DataRoot::at_non_demo(home.path().join(".vibebar"));
        let view = CostEngine::new(root.clone(), home.path()).refresh().unwrap();
        assert!(view.truncated);
        assert_eq!(view.scanned_files, 1);
        assert_eq!(view.all_time.requests, 1);
        assert!(!root.client_cost_snapshot_file().exists());
    }

    #[test]
    fn oversized_sparse_file_is_truncated_and_not_counted_as_scanned() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        let directory = home.path().join(".claude/projects/project");
        fs::create_dir_all(&directory).unwrap();
        fs::File::create(directory.join("too-large.jsonl"))
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        write_jsonl(
            &directory.join("readable.jsonl"),
            &[serde_json::json!({
                "type":"assistant","timestamp":rfc3339(scanned_at-1.0),
                "message":{"model":"claude-haiku-4-5","usage":{"input_tokens":1}}
            })],
        );

        let view = CostEngine::new(DataRoot::at(home.path().join(".vibebar")), home.path())
            .refresh()
            .unwrap();
        assert!(view.truncated);
        assert_eq!(view.scanned_files, 1);
        assert_eq!(view.all_time.requests, 1);
    }

    #[test]
    fn source_outside_the_scan_home_returns_skipped() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("outside.jsonl");
        fs::write(&path, "{}\n").unwrap();
        let directory = crate::paths::open_ambient_dir(home.path()).unwrap();
        let mut events = Vec::new();
        let mut malformed_lines = 0;
        let mut remaining_bytes = MAX_TOTAL_READ_BYTES;

        let result = scan_file(
            &directory,
            home.path(),
            &SourceFile {
                tool: ToolType::Claude,
                path,
                mtime: UNIX_EPOCH,
            },
            &mut events,
            &mut malformed_lines,
            MAX_RAW_EVENTS,
            &mut remaining_bytes,
        );

        assert_eq!(result, ScanFileResult::Skipped);
        assert!(events.is_empty());
    }

    #[test]
    fn raw_event_limit_stops_inside_a_single_file() {
        let home = tempfile::tempdir().unwrap();
        let scanned_at = now_unix();
        let path = home.path().join(".claude/projects/project/session.jsonl");
        write_jsonl(
            &path,
            &[
                serde_json::json!({"type":"assistant","timestamp":rfc3339(scanned_at-3.0),"message":{"model":"claude-haiku-4-5","usage":{"input_tokens":1}}}),
                serde_json::json!({"type":"assistant","timestamp":rfc3339(scanned_at-2.0),"message":{"model":"claude-haiku-4-5","usage":{"input_tokens":2}}}),
                serde_json::json!({"type":"assistant","timestamp":rfc3339(scanned_at-1.0),"message":{"model":"claude-haiku-4-5","usage":{"input_tokens":4}}}),
            ],
        );
        let directory = crate::paths::open_ambient_dir(home.path()).unwrap();
        let mut events = Vec::new();
        let mut malformed_lines = 0;
        let mut remaining_bytes = MAX_TOTAL_READ_BYTES;

        let result = scan_file(
            &directory,
            home.path(),
            &SourceFile {
                tool: ToolType::Claude,
                path,
                mtime: UNIX_EPOCH,
            },
            &mut events,
            &mut malformed_lines,
            2,
            &mut remaining_bytes,
        );

        assert_eq!(result, ScanFileResult::EventLimit);
        assert_eq!(events.len(), 2);
        assert_eq!(events.iter().map(|event| event.input).sum::<u64>(), 6);
    }
}
