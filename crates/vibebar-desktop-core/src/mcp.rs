//! Small read-only MCP stdio surface for Desktop's existing core readers.

use std::io::{BufRead, Write};
use std::path::Path;

use agent_session_core::SessionProvider;
use serde_json::{json, Map, Value};

use crate::client_store::ClientStore;
use crate::cost::effective_model_prices;
use crate::model::ToolType;
use crate::paths::{home_directory, DataRoot};
use crate::refresh::QuotaEngine;
use crate::sessions::SessionsService;
use crate::status::StoredStatusSnapshot;

pub const PROTOCOL_VERSION: &str = "2025-06-18";
static EMPTY_PARAMS: std::sync::LazyLock<Map<String, Value>> = std::sync::LazyLock::new(Map::new);

pub struct ReadonlyMcp {
    quota: QuotaEngine,
    sessions: SessionsService,
    status_store: ClientStore,
}

impl ReadonlyMcp {
    pub fn discover() -> Self {
        let root = DataRoot::discover();
        let home = if root.is_demo() {
            root.shared()
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.shared().to_path_buf())
        } else {
            home_directory()
        };
        Self::with_home(root, home)
    }

    pub fn with_home(root: DataRoot, home: impl Into<std::path::PathBuf>) -> Self {
        let home = home.into();
        Self {
            quota: QuotaEngine::new(root.clone()),
            sessions: SessionsService::with_home(root.clone(), home.clone()),
            status_store: ClientStore::new(root),
        }
    }

    /// One newline-free JSON-RPC request. Notifications intentionally reply
    /// with nothing, including `ping` notifications.
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let request: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => return Some(error(Value::Null, -32700, "parse error")),
        };
        let Some(object) = request.as_object() else {
            return Some(error(Value::Null, -32600, "invalid request"));
        };
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error(Value::Null, -32600, "invalid request"));
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return Some(error(Value::Null, -32600, "invalid request"));
        };
        let notification = !object.contains_key("id");
        let id = object.get("id").cloned().unwrap_or(Value::Null);
        let result = self.dispatch(method, object.get("params"));
        if notification {
            return None;
        }
        Some(match result {
            Ok(result) => response(id, result),
            Err(problem) => error(id, problem.code, problem.message),
        })
    }

    fn dispatch(&self, method: &str, params: Option<&Value>) -> Result<Value, Problem> {
        match method {
            "initialize" => {
                request_params(params, &["protocolVersion", "capabilities", "clientInfo"])?;
                Ok(json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {"listChanged": false}, "resources": {"listChanged": false, "subscribe": false}},
                    "serverInfo": {"name": "vibebar-desktop", "version": crate::VERSION},
                    "instructions": "Read-only local Vibe Bar data. Quota uses providers; sessions use harnesses."
                }))
            }
            "ping" => {
                request_params(params, &[])?;
                Ok(json!({}))
            }
            "tools/list" => {
                request_params(params, &[])?;
                Ok(json!({"tools": tool_catalog()}))
            }
            "resources/list" => {
                request_params(params, &[])?;
                Ok(json!({"resources": []}))
            }
            "tools/call" => self.call_tool(params),
            _ => Err(Problem::method_not_found()),
        }
    }

    fn call_tool(&self, params: Option<&Value>) -> Result<Value, Problem> {
        let params = request_params(params, &["name", "arguments"])?;
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(Problem::invalid_params)?;
        let arguments = params.get("arguments");
        let value = match name {
            "quota.get" => {
                let arguments = object_params(arguments, &["tools"])?;
                let tools = parse_tools(arguments.get("tools"))?;
                let mut view = self.quota.cached_view();
                if let Some(tools) = tools {
                    view.accounts
                        .retain(|account| tools.contains(&account.tool));
                    view.last_updated = view
                        .accounts
                        .iter()
                        .filter(|account| !account.buckets.is_empty())
                        .map(|account| account.queried_at)
                        .reduce(f64::max);
                }
                serde_json::to_value(view).map_err(|_| Problem::internal())?
            }
            "sessions.list" => {
                let arguments = object_params(
                    arguments,
                    &["providers", "harnesses", "since", "offset", "limit"],
                )?;
                let providers = parse_session_providers(arguments.get("providers"))?;
                let harnesses = parse_harnesses(arguments.get("harnesses"))?;
                let since = parse_since(arguments.get("since"))?;
                let offset = parse_offset(arguments.get("offset"), 0)?;
                let limit = parse_limit(arguments.get("limit"), 50, 100)?;
                serde_json::to_value(self.sessions.list_filtered(
                    providers.as_deref(),
                    harnesses.as_deref(),
                    since,
                    offset,
                    limit,
                ))
                .map_err(|_| Problem::internal())?
            }
            "sessions.search" => {
                let arguments =
                    object_params(arguments, &["query", "providers", "harnesses", "limit"])?;
                let query = arguments
                    .get("query")
                    .and_then(Value::as_str)
                    .filter(|query| !query.trim().is_empty())
                    .ok_or_else(Problem::invalid_params)?;
                let providers = parse_session_providers(arguments.get("providers"))?;
                let harnesses = parse_harnesses(arguments.get("harnesses"))?;
                let limit = parse_limit(arguments.get("limit"), 20, 50)?;
                serde_json::to_value(self.sessions.search_filtered(
                    query,
                    providers.as_deref(),
                    harnesses.as_deref(),
                    limit,
                ))
                .map_err(|_| Problem::internal())?
            }
            "status.get" => {
                let arguments = object_params(arguments, &["tools"])?;
                let tools = parse_tools(arguments.get("tools"))?;
                let status = self.status_store.load_status_snapshot(now_unix());
                serde_json::to_value(status_response(status.as_ref(), tools.as_deref()))
                    .map_err(|_| Problem::internal())?
            }
            "pricing.effective" => {
                let arguments = object_params(arguments, &["provider", "model"])?;
                let provider = parse_pricing_provider(arguments.get("provider"))?;
                let model = arguments
                    .get("model")
                    .map(|value| {
                        value
                            .as_str()
                            .map(|model| model.trim().to_ascii_lowercase())
                            .ok_or_else(Problem::invalid_params)
                    })
                    .transpose()?;
                let rows = effective_model_prices()
                    .into_iter()
                    .filter(|row| provider.is_none_or(|provider| row.provider == provider))
                    .filter(|row| {
                        model
                            .as_deref()
                            .is_none_or(|model| row.model.to_ascii_lowercase().contains(model))
                    })
                    .collect::<Vec<_>>();
                json!({
                    "generatedAt": unix_now(),
                    "unit": "USD per 1M tokens",
                    "rows": rows,
                })
            }
            _ => return Err(Problem::invalid_params()),
        };
        Ok(json!({"content": [{"type": "text", "text": value.to_string()}]}))
    }
}

/// Run one JSON-RPC line per stdin line, with no logging on stdout.
pub fn run_stdio() -> i32 {
    let server = ReadonlyMcp::discover();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    run_framed(&server, stdin.lock(), &mut stdout).map_or(1, |_| 0)
}

fn run_framed<R: BufRead, W: Write>(
    server: &ReadonlyMcp,
    reader: R,
    writer: &mut W,
) -> std::io::Result<()> {
    for line in reader.lines() {
        let line = line?;
        if let Some(reply) = server.handle_line(&line) {
            writeln!(writer, "{reply}")?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn tool_catalog() -> Vec<Value> {
    vec![
        tool(
            "quota.get",
            "Read cached subscription quota",
            schema(&[("tools", tools_schema())], &[]),
        ),
        tool(
            "sessions.list",
            "List local agent sessions",
            schema(
                &[
                    ("providers", session_providers_schema()),
                    ("harnesses", harnesses_schema()),
                    ("since", string_schema()),
                    ("offset", nonnegative_integer_schema()),
                    ("limit", integer_schema(1, 100)),
                ],
                &[],
            ),
        ),
        tool(
            "sessions.search",
            "Search local agent sessions",
            schema(
                &[
                    ("query", string_schema()),
                    ("providers", session_providers_schema()),
                    ("harnesses", harnesses_schema()),
                    ("limit", integer_schema(1, 50)),
                ],
                &["query"],
            ),
        ),
        tool(
            "status.get",
            "Read Desktop's last-good public provider status without refreshing the network",
            schema(&[("tools", tools_schema())], &[]),
        ),
        tool(
            "pricing.effective",
            "Read the static model prices Desktop actually uses",
            schema(
                &[
                    ("provider", pricing_provider_schema()),
                    ("model", string_schema()),
                ],
                &[],
            ),
        ),
    ]
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct McpStatusResponse {
    generated_at: f64,
    last_fetched: Option<f64>,
    companies: Vec<McpStatusCompany>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct McpStatusCompany {
    tool: ToolType,
    company: &'static str,
    indicator: String,
    description: String,
    updated_at: Option<f64>,
    is_refreshing: bool,
    error: Option<String>,
}

fn status_response(
    snapshot: Option<&StoredStatusSnapshot>,
    tools: Option<&[ToolType]>,
) -> McpStatusResponse {
    let now = now_unix();
    let Some(snapshot) = snapshot.filter(|snapshot| snapshot.valid_at(now)) else {
        return McpStatusResponse {
            generated_at: now,
            last_fetched: None,
            companies: Vec::new(),
        };
    };
    let mut companies = snapshot
        .providers
        .iter()
        .filter(|provider| {
            tools.is_none_or(|tools| {
                tools.contains(&provider.tool)
                    || (provider.tool == ToolType::Gemini && tools.contains(&ToolType::Antigravity))
            })
        })
        .map(|provider| McpStatusCompany {
            tool: provider.tool,
            company: provider.tool.hierarchy().vendor,
            indicator: provider.indicator.clone(),
            description: provider.description.clone(),
            updated_at: provider.updated_at,
            is_refreshing: false,
            error: None,
        })
        .collect::<Vec<_>>();
    companies.sort_by_key(|company| company.tool.raw_value());
    McpStatusResponse {
        generated_at: now,
        last_fetched: Some(snapshot.saved_at),
        companies,
    }
}

fn now_unix() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": input_schema})
}
fn schema(properties: &[(&str, Value)], required: &[&str]) -> Value {
    let properties: Map<String, Value> = properties
        .iter()
        .map(|(name, schema)| ((*name).to_string(), schema.clone()))
        .collect();
    json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false})
}
fn tools_schema() -> Value {
    json!({"type": "array", "items": {"type": "string", "enum": ToolType::ALL.map(|tool| tool.raw_value())}})
}
fn session_providers_schema() -> Value {
    json!({"type": "array", "items": {"type": "string", "enum": SessionProvider::ALL.map(|provider| provider.raw_value())}})
}
fn harnesses_schema() -> Value {
    json!({"type": "array", "items": {"type": "string", "enum": SESSION_HARNESSES}})
}
fn string_schema() -> Value {
    json!({"type": "string"})
}
fn integer_schema(minimum: usize, maximum: usize) -> Value {
    json!({"type": "integer", "minimum": minimum, "maximum": maximum})
}
fn pricing_provider_schema() -> Value {
    json!({"type": "string", "enum": ["codex", "claude", "gemini"]})
}

fn nonnegative_integer_schema() -> Value {
    json!({"type": "integer", "minimum": 0})
}

fn object_params<'a>(
    params: Option<&'a Value>,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, Problem> {
    let object = match params {
        Some(params) => params.as_object().ok_or_else(Problem::invalid_params)?,
        None => &EMPTY_PARAMS,
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Problem::invalid_params());
    }
    Ok(object)
}

fn request_params<'a>(
    params: Option<&'a Value>,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, Problem> {
    let object = match params {
        Some(params) => params.as_object().ok_or_else(Problem::invalid_params)?,
        None => &EMPTY_PARAMS,
    };
    if object.get("_meta").is_some_and(|meta| !meta.is_object()) {
        return Err(Problem::invalid_params());
    }
    if object
        .keys()
        .any(|key| key != "_meta" && !allowed.contains(&key.as_str()))
    {
        return Err(Problem::invalid_params());
    }
    Ok(object)
}
fn parse_tools(value: Option<&Value>) -> Result<Option<Vec<ToolType>>, Problem> {
    let Some(value) = value else { return Ok(None) };
    let array = value.as_array().ok_or_else(Problem::invalid_params)?;
    array
        .iter()
        .map(|raw| {
            raw.as_str()
                .and_then(ToolType::from_raw)
                .ok_or_else(Problem::invalid_params)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}
fn parse_pricing_provider(value: Option<&Value>) -> Result<Option<ToolType>, Problem> {
    let Some(value) = value else { return Ok(None) };
    match value.as_str() {
        Some("codex") => Ok(Some(ToolType::Codex)),
        Some("claude") => Ok(Some(ToolType::Claude)),
        Some("gemini") => Ok(Some(ToolType::Gemini)),
        _ => Err(Problem::invalid_params()),
    }
}

fn parse_session_providers(value: Option<&Value>) -> Result<Option<Vec<SessionProvider>>, Problem> {
    let Some(value) = value else { return Ok(None) };
    let array = value.as_array().ok_or_else(Problem::invalid_params)?;
    array
        .iter()
        .map(|raw| {
            raw.as_str()
                .and_then(SessionProvider::from_raw)
                .ok_or_else(Problem::invalid_params)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}
fn parse_harnesses(value: Option<&Value>) -> Result<Option<Vec<String>>, Problem> {
    let Some(value) = value else { return Ok(None) };
    let array = value.as_array().ok_or_else(Problem::invalid_params)?;
    array
        .iter()
        .map(|raw| {
            let raw = raw.as_str().ok_or_else(Problem::invalid_params)?;
            SESSION_HARNESSES
                .contains(&raw)
                .then(|| raw.to_string())
                .ok_or_else(Problem::invalid_params)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}
fn parse_since(value: Option<&Value>) -> Result<Option<i64>, Problem> {
    let Some(value) = value else { return Ok(None) };
    let raw = value.as_str().ok_or_else(Problem::invalid_params)?.trim();
    if raw.is_empty() {
        return Err(Problem::invalid_params());
    }
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|date| {
            Some(
                date.timestamp()
                    .saturating_add(i64::from(date.timestamp_subsec_nanos() > 0)),
            )
        })
        .map_err(|_| Problem::invalid_params())
}
fn parse_limit(value: Option<&Value>, default: usize, maximum: usize) -> Result<usize, Problem> {
    let Some(value) = value else {
        return Ok(default);
    };
    let limit = value
        .as_u64()
        .and_then(|limit| usize::try_from(limit).ok())
        .ok_or_else(Problem::invalid_params)?;
    if limit == 0 || limit > maximum {
        return Err(Problem::invalid_params());
    }
    Ok(limit)
}
fn parse_offset(value: Option<&Value>, default: usize) -> Result<usize, Problem> {
    let Some(value) = value else {
        return Ok(default);
    };
    value
        .as_u64()
        .and_then(|offset| usize::try_from(offset).ok())
        .ok_or_else(Problem::invalid_params)
}

const SESSION_HARNESSES: [&str; 9] = [
    "codex",
    "chatgptWork",
    "claudeCode",
    "claudeCowork",
    "geminiCLI",
    "antigravity",
    "grokBuild",
    "cursor",
    "grokBot",
];

struct Problem {
    code: i64,
    message: &'static str,
}
impl Problem {
    fn invalid_params() -> Self {
        Self {
            code: -32602,
            message: "invalid params",
        }
    }
    fn method_not_found() -> Self {
        Self {
            code: -32601,
            message: "method not found",
        }
    }
    fn internal() -> Self {
        Self {
            code: -32603,
            message: "internal error",
        }
    }
}
fn response(id: Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}
fn error(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn server(temp: &tempfile::TempDir) -> ReadonlyMcp {
        let root = DataRoot::at(temp.path().join(".vibebar"));
        let sessions = temp.path().join(".codex/sessions/2026/01/01");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("rollout-2026-01-01T00-00-00-0199aaaa-1111-2222-3333-444455556666.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"0199aaaa-1111-2222-3333-444455556666","session_id":"0299aaaa-1111-2222-3333-444455556666","originator":"Codex Desktop","source":{"subagent":{"other":"guardian"}}}}"#,
        )
        .unwrap();
        ReadonlyMcp::with_home(root, temp.path())
    }

    fn tool_payload(server: &ReadonlyMcp, name: &str, arguments: Value) -> Value {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        });
        let reply = server.handle_line(&request.to_string()).unwrap();
        let response: Value = serde_json::from_str(&reply).unwrap();
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("a successful text tool result");
        serde_json::from_str(text).unwrap()
    }

    fn tool_error_code(server: &ReadonlyMcp, name: &str, arguments: Value) -> i64 {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        });
        let reply = server.handle_line(&request.to_string()).unwrap();
        serde_json::from_str::<Value>(&reply).unwrap()["error"]["code"]
            .as_i64()
            .unwrap()
    }

    #[test]
    fn catalog_is_read_only_and_schemas_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let server = server(&temp);
        let reply = server
            .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
            .unwrap();
        let value: Value = serde_json::from_str(&reply).unwrap();
        let tools = value["result"]["tools"].as_array().unwrap();
        assert_eq!(
            tools
                .iter()
                .filter_map(|tool| tool["name"].as_str())
                .collect::<Vec<_>>(),
            vec![
                "quota.get",
                "sessions.list",
                "sessions.search",
                "status.get",
                "pricing.effective"
            ]
        );
        assert!(tools
            .iter()
            .all(|tool| tool["inputSchema"]["additionalProperties"] == false));
        let rejected = server.handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"quota.get","arguments":{"force":true}}}"#).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&rejected).unwrap()["error"]["code"],
            -32602
        );
    }

    #[test]
    fn session_catalog_matches_the_native_supported_filters() {
        let temp = tempfile::tempdir().unwrap();
        let server = server(&temp);
        let reply = server
            .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
            .unwrap();
        let value: Value = serde_json::from_str(&reply).unwrap();
        let tools = value["result"]["tools"].as_array().unwrap();
        let list = tools
            .iter()
            .find(|tool| tool["name"] == "sessions.list")
            .unwrap();
        let search = tools
            .iter()
            .find(|tool| tool["name"] == "sessions.search")
            .unwrap();
        let list_properties = list["inputSchema"]["properties"].as_object().unwrap();
        for name in ["providers", "harnesses", "since", "offset", "limit"] {
            assert!(list_properties.contains_key(name));
        }
        assert_eq!(list_properties["offset"]["minimum"], 0);
        assert!(list_properties["offset"].get("maximum").is_none());
        let harnesses = list_properties["harnesses"]["items"]["enum"]
            .as_array()
            .unwrap();
        assert!(harnesses.contains(&json!("chatgptWork")));
        assert!(!harnesses.contains(&json!("ChatGPT Work")));
        let search_properties = search["inputSchema"]["properties"].as_object().unwrap();
        assert!(search_properties.contains_key("providers"));
        assert!(search_properties.contains_key("harnesses"));
        assert!(!search_properties.contains_key("since"));
        assert!(!search_properties.contains_key("offset"));
    }

    #[test]
    fn session_tools_apply_filters_paging_and_dates_without_exposing_paths() {
        let temp = tempfile::tempdir().unwrap();
        let server = server(&temp);

        let codex = tool_payload(
            &server,
            "sessions.list",
            json!({"providers": ["codex"], "harnesses": ["codex"], "since": "1970-01-01T00:00:00Z"}),
        );
        assert_eq!(codex["rows"].as_array().unwrap().len(), 1);
        assert!(codex["rows"][0].get("sourcePath").is_none());
        assert!(codex["indexNote"]
            .as_str()
            .unwrap()
            .contains("newest 400 sessions per provider"));
        assert_eq!(
            codex["rows"][0]["providerVariant"],
            "auto-review:0299aaaa-1111-2222-3333-444455556666"
        );
        assert!(
            tool_payload(&server, "sessions.list", json!({"providers": ["claude"]}))["rows"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let deep_page = tool_payload(&server, "sessions.list", json!({"offset": 1_000_001}));
        assert!(deep_page["rows"].as_array().unwrap().is_empty());
        assert!(deep_page["indexNote"]
            .as_str()
            .unwrap()
            .contains("filter matches beyond that bound are omitted"));
        assert!(tool_payload(
            &server,
            "sessions.list",
            json!({"since": "2999-01-01T00:00:00Z"}),
        )["rows"]
            .as_array()
            .unwrap()
            .is_empty());

        let search = tool_payload(
            &server,
            "sessions.search",
            json!({"query": "0199aaaa", "providers": ["codex"], "harnesses": ["codex"]}),
        );
        assert_eq!(search["rows"].as_array().unwrap().len(), 1);
        assert!(search["indexNote"]
            .as_str()
            .unwrap()
            .contains("newest 400 sessions per provider"));
        assert!(tool_payload(
            &server,
            "sessions.search",
            json!({"query": "0199aaaa", "harnesses": ["chatgptWork"]}),
        )["rows"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn unsupported_or_malformed_session_filters_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let server = server(&temp);
        for (name, arguments) in [
            ("sessions.list", json!({"since": "2026-01-01"})),
            ("sessions.list", json!({"providers": ["future"]})),
            ("sessions.list", json!({"harnesses": ["Claude Code"]})),
            ("sessions.list", json!({"offset": -1})),
            (
                "sessions.search",
                json!({"query": "needle", "since": "2026-01-01"}),
            ),
        ] {
            assert_eq!(tool_error_code(&server, name, arguments), -32602);
        }
    }

    #[test]
    fn fractional_since_boundaries_round_up_to_stored_seconds() {
        let exact = json!("2026-08-30T12:00:00Z");
        let fractional = json!("2026-08-30T12:00:00.500Z");
        let exact = parse_since(Some(&exact))
            .ok()
            .flatten()
            .expect("valid exact boundary");
        let fractional = parse_since(Some(&fractional))
            .ok()
            .flatten()
            .expect("valid fractional boundary");
        assert_eq!(fractional, exact + 1);
    }

    #[test]
    fn malformed_requests_and_unknown_methods_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let server = server(&temp);
        for (request, code) in [
            ("not-json", -32700),
            (
                r#"{"jsonrpc":"2.0","id":1,"method":"future.method"}"#,
                -32601,
            ),
            (
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"future.tool","arguments":{}}}"#,
                -32602,
            ),
        ] {
            let reply = server.handle_line(request).unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&reply).unwrap()["error"]["code"],
                code
            );
        }
    }

    #[test]
    fn request_metadata_is_accepted_but_tool_arguments_stay_closed() {
        let temp = tempfile::tempdir().unwrap();
        let server = server(&temp);
        let accepted = server.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"quota.get","arguments":{},"_meta":{"progressToken":"p"}}}"#).unwrap();
        assert!(serde_json::from_str::<Value>(&accepted).unwrap()["result"].is_object());
        let rejected = server.handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"quota.get","arguments":{"_meta":{}}}}"#).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&rejected).unwrap()["error"]["code"],
            -32602
        );
        let malformed = server
            .handle_line(r#"{"jsonrpc":"2.0","id":3,"method":"ping","params":{"_meta":"bad"}}"#)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&malformed).unwrap()["error"]["code"],
            -32602
        );
    }

    #[test]
    fn quota_filter_recomputes_last_updated() {
        let temp = tempfile::tempdir().unwrap();
        let root = DataRoot::at(temp.path().join(".vibebar"));
        let store = crate::client_store::ClientStore::new(root.clone());
        for (tool, at) in [
            (ToolType::Codex, 1_788_038_400.0),
            (ToolType::Claude, 1_788_038_500.0),
        ] {
            store
                .save_quota(&crate::model::AccountQuota {
                    account_id: format!("{}-test", tool.raw_value()),
                    tool,
                    buckets: vec![crate::model::QuotaBucket::new(
                        "weekly", "Weekly", "wk", 25.0, None, None, None,
                    )],
                    plan: None,
                    queried_at: at,
                    origin: crate::model::QuotaOrigin::Live,
                    error: None,
                })
                .unwrap();
        }
        let server = ReadonlyMcp::with_home(root, temp.path());
        let reply = server.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"quota.get","arguments":{"tools":["codex"]}}}"#).unwrap();
        let response: Value = serde_json::from_str(&reply).unwrap();
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let view: Value = serde_json::from_str(text).unwrap();
        assert_eq!(view["lastUpdated"], 1_788_038_400.0);
        assert_eq!(view["accounts"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn pricing_effective_returns_only_the_static_cost_table_and_filters_it() {
        let temp = tempfile::tempdir().unwrap();
        let server = server(&temp);
        let reply = server.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"pricing.effective","arguments":{}}}"#).unwrap();
        let response: Value = serde_json::from_str(&reply).unwrap();
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let view: Value = serde_json::from_str(text).unwrap();
        assert_eq!(view["unit"], "USD per 1M tokens");
        assert!(view["generatedAt"].as_f64().unwrap() > 0.0);
        assert_eq!(view["rows"].as_array().unwrap().len(), 41);

        let gpt = view["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["model"] == "gpt-5.4")
            .unwrap();
        assert_eq!(gpt["provider"], "codex");
        assert_eq!(gpt["company"], "OpenAI");
        assert_eq!(gpt["subProvider"], "ChatGPT Agentic");
        assert_eq!(gpt["inputPerMillion"], 2.5);
        assert_eq!(gpt["outputPerMillion"], 15.0);
        assert_eq!(gpt["cacheReadPerMillion"], 0.25);
        assert_eq!(gpt["fastMultiplier"], 2.0);

        let sonnet = view["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["model"] == "claude-sonnet-4-5")
            .unwrap();
        assert_eq!(sonnet["thresholdTokens"], 200_000);
        assert_eq!(sonnet["inputAboveThresholdPerMillion"], 6.0);
        assert_eq!(sonnet["outputAboveThresholdPerMillion"], 22.5);
        assert_eq!(sonnet["cacheReadAboveThresholdPerMillion"], 0.6);
        assert_eq!(sonnet["cacheWriteAboveThresholdPerMillion"], 7.5);

        let gemini = view["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["model"] == "gemini-2.5-pro")
            .unwrap();
        assert_eq!(gemini["provider"], "gemini");
        assert_eq!(gemini["thresholdTokens"], 200_000);
        assert_eq!(gemini["cacheReadAboveThresholdPerMillion"], 0.625);

        let filtered = server.handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"pricing.effective","arguments":{"provider":"claude","model":"OPUS-4-7"}}}"#).unwrap();
        let response: Value = serde_json::from_str(&filtered).unwrap();
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let view: Value = serde_json::from_str(text).unwrap();
        let rows = view["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["model"], "claude-opus-4-7");
        assert_eq!(rows[0]["cacheWritePerMillion"], 6.25);
    }

    #[test]
    fn pricing_effective_rejects_unknown_providers_and_extra_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let server = server(&temp);
        for arguments in [r#"{"provider":"cursor"}"#, r#"{"refresh":true}"#] {
            let request = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"pricing.effective","arguments":{arguments}}}}}"#
            );
            let reply = server.handle_line(&request).unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&reply).unwrap()["error"]["code"],
                -32602
            );
        }
    }

    #[test]
    fn two_line_framing_and_notification_are_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let server = server(&temp);
        let input = Cursor::new("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"sessions.list\",\"arguments\":{\"limit\":1}}}\n");
        let mut output = Vec::new();
        run_framed(&server, input, &mut output).unwrap();
        let replies: Vec<_> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(replies.len(), 2);
        assert_eq!(
            serde_json::from_str::<Value>(&replies[0]).unwrap()["result"]["protocolVersion"],
            PROTOCOL_VERSION
        );
        let text = serde_json::from_str::<Value>(&replies[1]).unwrap()["result"]["content"][0]
            ["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap()["rows"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(server
            .handle_line(r#"{"jsonrpc":"2.0","method":"ping"}"#)
            .is_none());
    }

    #[test]
    fn read_only_tools_do_not_create_a_shared_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = DataRoot::at(temp.path().join(".vibebar"));
        let server = ReadonlyMcp::with_home(root.clone(), temp.path());
        for name in [
            "quota.get",
            "sessions.list",
            "status.get",
            "pricing.effective",
        ] {
            let line = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{name}","arguments":{{}}}}}}"#
            );
            assert!(server.handle_line(&line).is_some());
        }
        assert!(!root.shared().exists());
    }

    #[test]
    fn status_get_reads_only_the_fresh_private_snapshot_and_maps_antigravity() {
        let temp = tempfile::tempdir().unwrap();
        let root = DataRoot::at(temp.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        std::fs::write(root.service_status_file(), "native-must-not-change").unwrap();
        let before = std::fs::read(root.service_status_file()).unwrap();
        let now = now_unix();
        crate::client_store::ClientStore::new(root.clone())
            .save_status_snapshot(&crate::status::StoredStatusSnapshot {
                schema_version: crate::status::STATUS_SNAPSHOT_SCHEMA_VERSION,
                saved_at: now,
                providers: vec![crate::status::StoredProviderStatus {
                    tool: ToolType::Gemini,
                    indicator: "minor".into(),
                    description: "Synthetic Google AI issue".into(),
                    updated_at: Some(now - 1.0),
                    incidents: vec![],
                }],
            })
            .unwrap();
        let server = ReadonlyMcp::with_home(root.clone(), temp.path());
        let reply = server.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"status.get","arguments":{"tools":["antigravity"]},"_meta":{"progressToken":"p"}}}"#).unwrap();
        let response: Value = serde_json::from_str(&reply).unwrap();
        let value: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert!(value["lastFetched"].as_f64().unwrap() > 0.0);
        assert_eq!(value["companies"].as_array().unwrap().len(), 1);
        assert_eq!(value["companies"][0]["tool"], "gemini");
        assert_eq!(value["companies"][0]["company"], "Google AI");
        assert_eq!(value["companies"][0]["isRefreshing"], false);
        assert_eq!(value["companies"][0]["error"], Value::Null);
        assert_eq!(std::fs::read(root.service_status_file()).unwrap(), before);
    }

    #[test]
    fn status_get_reloads_a_snapshot_created_after_the_stdio_server_started() {
        let temp = tempfile::tempdir().unwrap();
        let root = DataRoot::at(temp.path().join(".vibebar"));
        let server = ReadonlyMcp::with_home(root.clone(), temp.path());
        let now = now_unix();
        crate::client_store::ClientStore::new(root)
            .save_status_snapshot(&crate::status::StoredStatusSnapshot {
                schema_version: crate::status::STATUS_SNAPSHOT_SCHEMA_VERSION,
                saved_at: now,
                providers: vec![crate::status::StoredProviderStatus {
                    tool: ToolType::Claude,
                    indicator: "none".into(),
                    description: "Synthetic recovery".into(),
                    updated_at: Some(now),
                    incidents: vec![],
                }],
            })
            .unwrap();

        let reply = server.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"status.get","arguments":{}}}"#).unwrap();
        let response: Value = serde_json::from_str(&reply).unwrap();
        let value: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(value["companies"][0]["tool"], "claude");
    }
}
