//! Small read-only MCP stdio surface for Desktop's existing core readers.

use std::io::{BufRead, Write};
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::model::ToolType;
use crate::paths::{home_directory, DataRoot};
use crate::refresh::QuotaEngine;
use crate::sessions::SessionsService;

pub const PROTOCOL_VERSION: &str = "2025-06-18";
static EMPTY_PARAMS: std::sync::LazyLock<Map<String, Value>> = std::sync::LazyLock::new(Map::new);

pub struct ReadonlyMcp {
    quota: QuotaEngine,
    sessions: SessionsService,
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
                        .filter(|account| account.error.is_none())
                        .map(|account| account.queried_at)
                        .reduce(f64::max);
                }
                serde_json::to_value(view).map_err(|_| Problem::internal())?
            }
            "sessions.list" => {
                let arguments = object_params(arguments, &["limit"])?;
                let limit = parse_limit(arguments.get("limit"), 50, 100)?;
                serde_json::to_value(self.sessions.list(limit)).map_err(|_| Problem::internal())?
            }
            "sessions.search" => {
                let arguments = object_params(arguments, &["query", "limit"])?;
                let query = arguments
                    .get("query")
                    .and_then(Value::as_str)
                    .filter(|query| !query.trim().is_empty())
                    .ok_or_else(Problem::invalid_params)?;
                let limit = parse_limit(arguments.get("limit"), 20, 50)?;
                serde_json::to_value(self.sessions.search(query, limit))
                    .map_err(|_| Problem::internal())?
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
            schema(&[("limit", integer_schema(1, 100))], &[]),
        ),
        tool(
            "sessions.search",
            "Search local agent sessions",
            schema(
                &[("query", string_schema()), ("limit", integer_schema(1, 50))],
                &["query"],
            ),
        ),
    ]
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
fn string_schema() -> Value {
    json!({"type": "string"})
}
fn integer_schema(minimum: usize, maximum: usize) -> Value {
    json!({"type": "integer", "minimum": minimum, "maximum": maximum})
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
fn parse_limit(value: Option<&Value>, default: usize, maximum: usize) -> Result<usize, Problem> {
    let Some(value) = value else {
        return Ok(default);
    };
    let limit = value.as_u64().ok_or_else(Problem::invalid_params)? as usize;
    if limit == 0 || limit > maximum {
        return Err(Problem::invalid_params());
    }
    Ok(limit)
}

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
            r#"{"type":"session_meta","payload":{"id":"0199aaaa-1111-2222-3333-444455556666"}}"#,
        )
        .unwrap();
        ReadonlyMcp::with_home(root, temp.path())
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
            vec!["quota.get", "sessions.list", "sessions.search"]
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
        for name in ["quota.get", "sessions.list"] {
            let line = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{name}","arguments":{{}}}}}}"#
            );
            assert!(server.handle_line(&line).is_some());
        }
        assert!(!root.shared().exists());
    }
}
