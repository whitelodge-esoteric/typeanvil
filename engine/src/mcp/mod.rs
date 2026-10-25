// SPDX-License-Identifier: AGPL-3.0-only

//! Local MCP server (`typeanvil mcp`) and JSON request mode
//! (`typeanvil call <tool>`), CORE-575.
//!
//! Implements the tool contract of `docs/specifications/mcp-tool-contract.spec.md`
//! (schemas in `schemas/mcp/v1/`). Server behavior beyond the contract is in
//! `docs/specifications/mcp-server.spec.md`.
//!
//! Transport: MCP stdio — one JSON-RPC 2.0 message per line on stdin, one per
//! line on stdout. Nothing else is written to stdout. Requests run one at a
//! time, in arrival order.

mod input;
mod raster;
pub mod schema;
mod tools;

use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde_json::{json, Map, Value};

pub use input::ToolError;

/// Contract version this server implements.
/// 1.1 (CORE-588) added the `templates` tool, `data`/`schema` inputs on
/// `render`/`validate`, and the `TEMPLATE_*` error codes.
pub const CONTRACT_VERSION: &str = "1.1";

/// MCP protocol revisions this server speaks, newest first. The server
/// answers `initialize` with the client's revision when it is listed here,
/// else with the newest.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// The six contract tools, in contract order.
pub const TOOL_NAMES: [&str; 6] = ["render", "validate", "inspect_page", "capabilities", "explain", "templates"];

/// One local server. Stateless between calls (contract Behavior 12): the
/// only state is the configured path roots.
pub struct Server {
    /// Readable/writable path roots (`--root`). Empty = no restriction.
    roots: Vec<PathBuf>,
}

impl Server {
    pub fn new(roots: Vec<PathBuf>) -> Server {
        let roots = roots
            .into_iter()
            .map(|r| input::absolute(&r))
            .map(|r| std::fs::canonicalize(&r).unwrap_or(r))
            .collect();
        Server { roots }
    }

    /// Serve MCP over a line-delimited JSON-RPC stream until EOF.
    pub fn serve(&self, reader: impl BufRead, mut writer: impl Write) -> std::io::Result<()> {
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(reply) = self.handle_line(&line) {
                writer.write_all(serde_json::to_string(&reply).expect("serialize").as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
        }
        Ok(())
    }

    /// Handle one JSON-RPC message (a request, a notification or a batch).
    /// Returns the reply, or `None` for notifications.
    pub fn handle_line(&self, line: &str) -> Option<Value> {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => return Some(rpc_error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        match msg {
            Value::Array(items) => {
                let replies: Vec<Value> = items.iter().filter_map(|m| self.handle_message(m)).collect();
                if replies.is_empty() {
                    None
                } else {
                    Some(Value::Array(replies))
                }
            }
            m => self.handle_message(&m),
        }
    }

    fn handle_message(&self, msg: &Value) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            return Some(rpc_error(Value::Null, -32600, "invalid request"));
        };
        let id = obj.get("id").cloned();
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            // A response from the client (we never send requests) or junk.
            return id.map(|id| rpc_error(id, -32600, "invalid request: no method"));
        };
        // Notifications (no id) never get a reply.
        let id = id?;
        let params = obj.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_descriptors() })),
            "tools/call" => self.tools_call(&params),
            "resources/list" => Ok(json!({ "resources": [{
                "uri": crate::guide::RESOURCE_URI,
                "name": "authoring-guide",
                "title": "TypeAnvil authoring guide",
                "description": "How to write HTML that renders well in TypeAnvil. Generated from the capability manifest.",
                "mimeType": "text/markdown",
            }] })),
            "resources/read" => {
                let uri = params.get("uri").and_then(Value::as_str).unwrap_or_default();
                if uri == crate::guide::RESOURCE_URI {
                    Ok(json!({ "contents": [{
                        "uri": uri,
                        "mimeType": "text/markdown",
                        "text": crate::guide::body(),
                    }] }))
                } else {
                    Err((-32002, format!("resource not found: {uri}")))
                }
            }
            "prompts/list" => Ok(json!({ "prompts": [{
                "name": crate::guide::PROMPT_NAME,
                "title": "Write a document for TypeAnvil",
                "description": "Load the authoring guide before writing an HTML document that renders well to PDF.",
                "arguments": [{ "name": "task", "description": "What the document should contain.", "required": false }],
            }] })),
            "prompts/get" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                if name != crate::guide::PROMPT_NAME {
                    Err((-32602, format!("unknown prompt: {name}")))
                } else {
                    let task = params
                        .get("arguments")
                        .and_then(|a| a.get("task"))
                        .and_then(Value::as_str)
                        .unwrap_or("the document the user asked for");
                    let text = format!(
                        "{}\n\n---\n\nTask: write {task}. Follow the guide above, then call `validate` and fix every finding before `render`.",
                        crate::guide::body()
                    );
                    Ok(json!({
                        "description": "TypeAnvil authoring guide",
                        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }],
                    }))
                }
            }
            other => Err((-32601, format!("method not found: {other}"))),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => rpc_error(id, code, &message),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "listChanged": false },
                "prompts": { "listChanged": false },
            },
            "serverInfo": {
                // Contract "Naming": the server name is always `typeanvil`.
                "name": "typeanvil",
                "version": engine_version(),
            },
            "instructions": "TypeAnvil renders HTML or Markdown to paged PDF. Call `capabilities` first; use `validate` and `inspect_page` to check layout before `render`.",
        })
    }

    fn tools_call(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((-32602, "tools/call needs a string `name`".to_string()))?;
        if !TOOL_NAMES.contains(&name) {
            // Contract: an unknown tool is a protocol error.
            return Err((-32602, format!("unknown tool: {name}")));
        }
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let (payload, is_error) = self.call(name, args);
        let text = serde_json::to_string(&payload).expect("serialize");
        let mut out = Map::new();
        out.insert("content".into(), json!([{ "type": "text", "text": text }]));
        out.insert("structuredContent".into(), payload);
        out.insert("isError".into(), Value::Bool(is_error));
        Ok(Value::Object(out))
    }

    /// Run one tool on already-parsed arguments. Returns the result object
    /// (success output or `Error` payload) and whether it is an error.
    pub fn call(&self, tool: &str, args: Value) -> (Value, bool) {
        let ctx = tools::Ctx { roots: &self.roots };
        // A panic inside the engine is a server fault (INTERNAL), not a dead
        // server: the next call must still work.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tools::dispatch(&ctx, tool, args)
        }));
        match outcome {
            Ok(Ok(v)) => (v, false),
            Ok(Err(e)) => (e.to_json(), true),
            Err(_) => (
                ToolError::new("INTERNAL", "the engine panicked while handling this call").to_json(),
                true,
            ),
        }
    }

    /// JSON request mode: `body` is the tool's input object as JSON text.
    pub fn call_json(&self, tool: &str, body: &str) -> (Value, bool) {
        if !TOOL_NAMES.contains(&tool) {
            return (
                ToolError::new("INVALID_INPUT", &format!("unknown tool `{tool}`")).to_json(),
                true,
            );
        }
        let trimmed = body.trim();
        let args = if trimmed.is_empty() {
            json!({})
        } else {
            match serde_json::from_str::<Value>(trimmed) {
                Ok(v) => v,
                Err(e) => {
                    return (
                        ToolError::new("INVALID_INPUT", &format!("request is not valid JSON: {e}"))
                            .to_json(),
                        true,
                    )
                }
            }
        };
        self.call(tool, args)
    }
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// `tools/list` entries: name, description and the published schemas.
pub fn tool_descriptors() -> Vec<Value> {
    TOOL_NAMES
        .iter()
        .map(|&name| {
            json!({
                "name": name,
                "title": tool_title(name),
                "description": tool_description(name),
                "inputSchema": schema::published(name, schema::Kind::Input),
                "outputSchema": schema::published(name, schema::Kind::Output),
            })
        })
        .collect()
}

fn tool_title(name: &str) -> &'static str {
    match name {
        "render" => "Render to PDF",
        "validate" => "Validate layout",
        "inspect_page" => "Inspect one page",
        "capabilities" => "Server capabilities",
        "templates" => "Template data schema",
        _ => "Explain an element",
    }
}

fn tool_description(name: &str) -> &'static str {
    match name {
        "render" => "Render an HTML or Markdown document to PDF. The document is a path, inline content or a URL. With `data` (JSON), the document is a MiniJinja HTML template: the data is validated against the template's schema, then expanded. Returns the PDF as base64 bytes (default) or writes it to output.path, plus page_count, sha256 and diagnostics.",
        "templates" => "Return the data JSON Schema of a data template (its sibling <name>.schema.json, or null when it has none). Call it before writing `data` for render.",
        "validate" => "Lay the document out without returning a PDF. Returns diagnostics, the fonts used and the result of each assertion (page_count, no_overflow, no_diagnostics, text_on_page, element_on_page, fonts_embedded). ok is false when an assertion fails.",
        "inspect_page" => "Return the layout boxes of one page (points, origin top-left) and a PNG of that page.",
        "capabilities" => "Report what this server supports: tools, input forms, output modes, limits, fonts and the CSS support guide.",
        _ => "Return the computed style and page placements of the elements that match a CSS selector.",
    }
}

/// `environment` block carried by every result and error.
pub fn environment() -> Value {
    json!({
        "contract_version": CONTRACT_VERSION,
        "engine_version": engine_version(),
        "font_bundle_version": crate::fonts::bundle_fingerprint(),
        "runtime": "local",
    })
}

/// The engine release version. An unstamped development build (the
/// placeholder crate version `0.0.1`) appends `+<short commit>` when the
/// build saw one, else `+dev` (contract Environment Behavior 1).
pub fn engine_version() -> String {
    let v = env!("CARGO_PKG_VERSION");
    if v != "0.0.1" {
        return v.to_string();
    }
    match option_env!("TYPEANVIL_COMMIT") {
        Some(c) if !c.is_empty() => format!("{v}+{c}"),
        _ => format!("{v}+dev"),
    }
}
