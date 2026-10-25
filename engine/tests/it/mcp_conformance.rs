// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-575: the MCP contract conformance suite, run against the local
//! server (`typeanvil mcp` over stdio).
//!
//! Cases: `tests/mcp-conformance/cases/*.json` (format in that directory's
//! README). Schemas: `schemas/mcp/v1/`. Results are validated with the
//! independent `jsonschema` crate (the server's own validator is not used to
//! judge the server). Spec: `docs/specifications/mcp-tool-contract.spec.md`
//! § Conformance and `docs/specifications/mcp-server.spec.md`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

const TOOLS: [&str; 5] = ["render", "validate", "inspect_page", "capabilities", "explain"];
const SCHEMA_BASE: &str = "https://typeanvil.com/schemas/mcp/v1/";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn suite_dir() -> PathBuf {
    repo().join("tests/mcp-conformance")
}

fn schema_dir() -> PathBuf {
    repo().join("schemas/mcp/v1")
}

fn read_json(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .unwrap_or_else(|e| panic!("{} is not JSON: {e}", p.display()))
}

/// A registry holding every schema file under its `$id`.
fn registry() -> jsonschema::Registry<'static> {
    let mut b = jsonschema::Registry::new();
    for entry in std::fs::read_dir(schema_dir()).unwrap() {
        let path = entry.unwrap().path();
        let v = read_json(&path);
        let id = v["$id"].as_str().unwrap().to_string();
        b = b.add(id, v).unwrap();
    }
    b.prepare().unwrap()
}

/// An independent validator for `$ref` (a URI under SCHEMA_BASE or a raw
/// published schema when `schema` is given).
struct Schemas {
    registry: jsonschema::Registry<'static>,
}

impl Schemas {
    fn new() -> Schemas {
        Schemas { registry: registry() }
    }

    fn validator(&self, schema: &Value) -> jsonschema::Validator {
        jsonschema::options()
            .with_registry(&self.registry)
            .build(schema)
            .unwrap_or_else(|e| panic!("schema does not compile: {e}"))
    }

    fn by_ref(&self, uri: &str) -> jsonschema::Validator {
        self.validator(&json!({ "$ref": uri }))
    }

    fn errors(&self, v: &jsonschema::Validator, instance: &Value) -> Vec<String> {
        v.iter_errors(instance).map(|e| format!("{} at {}", e, e.instance_path())).collect()
    }
}

// --- stdio client ------------------------------------------------------------

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn spawn(cwd: &Path, args: &[&str]) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_typeanvil"))
            .arg("mcp")
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn typeanvil mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Client { child, stdin, stdout, next_id: 1 }
    }

    fn send(&mut self, msg: &Value) {
        let mut line = serde_json::to_string(msg).unwrap();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).unwrap();
        assert!(n > 0, "server closed stdout");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line}"))
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let reply = self.read();
        assert_eq!(reply["id"], json!(id), "reply id mismatch: {reply}");
        assert_eq!(reply["jsonrpc"], "2.0");
        reply
    }

    fn initialize(&mut self) -> Value {
        let r = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "conformance", "version": "1" }
            }),
        );
        self.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        r
    }

    fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({ "name": tool, "arguments": args }));
        assert!(r.get("error").is_none(), "tools/call returned a protocol error: {r}");
        r["result"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// --- fixture HTTP server -----------------------------------------------------

/// Serve `dir` over HTTP on 127.0.0.1 (GET only; 404 for a missing file).
fn http_server(dir: PathBuf) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_string();
            let file = dir.join(&path);
            let resp = match std::fs::read(&file) {
                Ok(body) if !path.contains("..") => {
                    let mut r = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    r.extend(body);
                    r
                }
                _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            };
            let _ = s.write_all(&resp);
        }
    });
    format!("http://{addr}")
}

// --- case runner -------------------------------------------------------------

fn substitute(v: &Value, base: &str) -> Value {
    match v {
        Value::String(s) => Value::String(s.replace("{{BASE_URL}}", base)),
        Value::Array(a) => Value::Array(a.iter().map(|x| substitute(x, base)).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), substitute(x, base))).collect()),
        other => other.clone(),
    }
}

fn check_environment(payload: &Value, case: &str) {
    let env = &payload["environment"];
    assert_eq!(env["runtime"], "local", "{case}: environment.runtime");
    assert_eq!(env["contract_version"], "1.0", "{case}: environment.contract_version");
    for k in ["engine_version", "font_bundle_version"] {
        assert!(env[k].as_str().is_some_and(|s| !s.is_empty()), "{case}: environment.{k} missing");
    }
}

fn run_checks(case: &str, payload: &Value, checks: &[Value]) {
    for c in checks {
        let ptr = c["pointer"].as_str().unwrap();
        let got = payload.pointer(ptr);
        if let Some(want) = c.get("exists") {
            assert_eq!(got.is_some(), want.as_bool().unwrap(), "{case}: {ptr} exists? payload: {payload}");
            continue;
        }
        let got = got.unwrap_or_else(|| panic!("{case}: {ptr} missing in {payload}"));
        if let Some(want) = c.get("equals") {
            let same = match (got.as_f64(), want.as_f64()) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                _ => got == want,
            };
            assert!(same, "{case}: {ptr} = {got}, want {want}");
        }
        if let Some(min) = c.get("min").and_then(Value::as_f64) {
            assert!(got.as_f64().unwrap() >= min, "{case}: {ptr} = {got} < {min}");
        }
        if let Some(max) = c.get("max").and_then(Value::as_f64) {
            assert!(got.as_f64().unwrap() <= max, "{case}: {ptr} = {got} > {max}");
        }
        if let Some(min) = c.get("min_items").and_then(Value::as_u64) {
            assert!(got.as_array().unwrap().len() as u64 >= min, "{case}: {ptr} has fewer than {min} items");
        }
        if let Some(want) = c.get("contains") {
            let ok = match got {
                Value::Array(a) => a.contains(want),
                Value::String(s) => s.contains(want.as_str().unwrap()),
                _ => false,
            };
            assert!(ok, "{case}: {ptr} = {got} does not contain {want}");
        }
    }
}

/// Every tool-schema pair: the published (listed) schema and the file
/// accept and reject the same probes (mcp-server spec S4).
fn check_listing(schemas: &Schemas, client: &mut Client) {
    let listed = client.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, TOOLS, "tools/list names");
    let meta = jsonschema::draft202012::meta::validator();
    for t in tools {
        let name = t["name"].as_str().unwrap();
        for (key, kind) in [("inputSchema", "input"), ("outputSchema", "output")] {
            let published = &t[key];
            assert!(meta.is_valid(published), "{name} {key} is not a valid 2020-12 schema");
            assert_eq!(published["type"], "object", "{name} {key} type");
            let pv = schemas.validator(published);
            let fv = schemas.by_ref(&format!("{SCHEMA_BASE}{name}.{kind}.schema.json"));
            for probe in probes(name, kind) {
                assert_eq!(
                    pv.is_valid(&probe),
                    fv.is_valid(&probe),
                    "{name} {key}: published schema and file disagree on {probe}"
                );
            }
        }
    }
}

/// Valid and invalid instances that exercise each schema.
fn probes(tool: &str, kind: &str) -> Vec<Value> {
    let env = json!({ "contract_version": "1.0", "engine_version": "x", "font_bundle_version": "y", "runtime": "local" });
    let mut v = vec![json!({}), json!(null), json!({ "nope": 1 })];
    match (tool, kind) {
        (_, "input") => {
            v.push(json!({ "document": { "content": "x" } }));
            v.push(json!({ "document": { "content": "x", "url": "http://a/" } }));
            v.push(json!({ "document": { "path": "a.html", "format": "markdown" } }));
            v.push(json!({ "document": { "content": "x", "extra": 1 } }));
            v.push(json!({ "document": { "content": "x" }, "options": { "page_width": "5in", "tagged": true } }));
            v.push(json!({ "document": { "content": "x" }, "options": { "bogus": 1 } }));
            v.push(json!({ "document": { "content": "x" }, "assets": { "a.png": { "content_base64": "AA==" } } }));
            v.push(json!({ "document": { "content": "x" }, "assets": { "a.png": { "path": "a", "url": "http://a/" } } }));
            v.push(json!({ "document": { "content": "x" }, "page": 1, "selector": "p" }));
            v.push(json!({ "document": { "content": "x" }, "page": 0 }));
            v.push(json!({ "document": { "content": "x" }, "output": { "mode": "path" } }));
            v.push(json!({ "document": { "content": "x" }, "output": { "mode": "path", "path": "o.pdf" } }));
            v.push(json!({ "section": "fonts" }));
            v.push(json!({ "section": "everything" }));
        }
        ("render", _) => {
            v.push(json!({ "environment": env, "page_count": 1, "byte_size": 3, "sha256": "a".repeat(64), "artifact": { "kind": "bytes", "data_base64": "AA==" }, "diagnostics": [] }));
            v.push(json!({ "environment": env, "page_count": 1, "byte_size": 3, "sha256": "A".repeat(64), "artifact": { "kind": "bytes", "data_base64": "AA==" }, "diagnostics": [] }));
            v.push(json!({ "environment": env, "page_count": 1, "byte_size": 3, "sha256": "a".repeat(64), "artifact": { "kind": "link" }, "diagnostics": [] }));
        }
        ("capabilities", _) => {
            v.push(json!({ "environment": env, "tools": ["render"], "input_forms": { "path": true, "content": true, "url": true }, "output_modes": ["bytes"], "limits": { "max_pages": null } }));
            v.push(json!({ "environment": env, "tools": ["render"], "input_forms": { "path": true, "content": true, "url": true }, "output_modes": ["bytes"], "limits": { "max_pages": "many" } }));
        }
        _ => {
            v.push(json!({ "environment": env }));
            v.push(json!({ "environment": { "runtime": "elsewhere" } }));
        }
    }
    v
}

#[test]
fn schema_files_are_valid_and_complete() {
    let meta = jsonschema::draft202012::meta::validator();
    let mut seen = 0;
    for entry in std::fs::read_dir(schema_dir()).unwrap() {
        let path = entry.unwrap().path();
        let v = read_json(&path);
        assert!(meta.is_valid(&v), "{} fails 2020-12 meta-validation", path.display());
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(v["$id"], json!(format!("{SCHEMA_BASE}{name}")), "{name} $id");
        seen += 1;
    }
    assert_eq!(seen, 11, "five tools x (input, output) + common");
    for t in TOOLS {
        for kind in ["input", "output"] {
            assert!(schema_dir().join(format!("{t}.{kind}.schema.json")).is_file());
        }
    }
}

/// The error-code enum and the spec's error table list the same codes.
#[test]
fn error_codes_match_spec_table() {
    let common = read_json(&schema_dir().join("common.schema.json"));
    let mut enum_codes: Vec<String> = common["$defs"]["ErrorCode"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect();
    let spec = std::fs::read_to_string(repo().join("docs/specifications/mcp-tool-contract.spec.md")).unwrap();
    let table = spec.split("### Error codes").nth(1).unwrap().split("### Behavior").next().unwrap();
    let mut spec_codes: Vec<String> = table
        .lines()
        .filter_map(|l| l.strip_prefix("| `"))
        .filter_map(|l| l.split('`').next())
        .map(str::to_string)
        .collect();
    enum_codes.sort();
    spec_codes.sort();
    assert_eq!(enum_codes, spec_codes);
}

/// The whole case suite against one `typeanvil mcp` process.
#[test]
fn conformance_cases_pass_against_local_server() {
    let schemas = Schemas::new();
    let error_v = schemas.by_ref(&format!("{SCHEMA_BASE}common.schema.json#/$defs/Error"));
    let base = http_server(suite_dir().join("fixtures"));

    let mut client = Client::spawn(&suite_dir(), &[]);
    let init = client.initialize();
    assert_eq!(init["result"]["serverInfo"]["name"], "typeanvil", "initialize serverInfo.name");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    let mut cases: Vec<PathBuf> = std::fs::read_dir(suite_dir().join("cases"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    cases.sort();
    assert!(cases.len() >= 20, "expected the minimum case set, found {}", cases.len());

    let mut ran = Vec::new();
    let mut limit_cases = Vec::new();
    for path in &cases {
        let case = read_json(path);
        let id = case["id"].as_str().unwrap().to_string();
        assert_eq!(
            path.file_stem().unwrap().to_string_lossy(),
            id,
            "case file name must equal its id"
        );
        if id.starts_with("limit-") {
            limit_cases.push(id.clone());
        }
        let runtimes: Vec<&str> = case["runtimes"].as_array().unwrap().iter().map(|r| r.as_str().unwrap()).collect();
        if !runtimes.contains(&"local") {
            continue;
        }
        let tool = case["tool"].as_str().unwrap();
        if tool == "tools/list" {
            check_listing(&schemas, &mut client);
            ran.push(id);
            continue;
        }
        assert!(TOOLS.contains(&tool), "{id}: unknown tool {tool}");
        let input = substitute(&case["input"], &base);
        let expect = &case["expect"];
        let repeat = case.get("repeat").and_then(Value::as_u64).unwrap_or(1);
        let mut payloads = Vec::new();
        for _ in 0..repeat {
            let result = client.call(tool, input.clone());
            let payload = result["structuredContent"].clone();
            // content[0] carries the same object as JSON text.
            let text = result["content"][0]["text"].as_str().expect("content[0].text");
            assert_eq!(result["content"][0]["type"], "text");
            assert_eq!(serde_json::from_str::<Value>(text).unwrap(), payload, "{id}: content[0] != structuredContent");
            check_environment(&payload, &id);
            let is_error = result["isError"].as_bool().unwrap_or(false);
            if let Some(code) = expect.get("error") {
                assert!(is_error, "{id}: expected error {code}, got {payload}");
                assert_eq!(&payload["code"], code, "{id}: error code; payload {payload}");
                let errs = schemas.errors(&error_v, &payload);
                assert!(errs.is_empty(), "{id}: error payload fails Error schema: {errs:?}");
            } else {
                assert!(!is_error, "{id}: unexpected error {payload}");
                let out_v = schemas.by_ref(&format!("{SCHEMA_BASE}{tool}.output.schema.json"));
                let errs = schemas.errors(&out_v, &payload);
                assert!(errs.is_empty(), "{id}: result fails {tool} output schema: {errs:?}");
            }
            if let Some(Value::Array(checks)) = expect.get("checks") {
                run_checks(&id, &payload, checks);
            }
            payloads.push(payload);
        }
        if let Some(Value::Array(ptrs)) = expect.get("same_across_repeats") {
            for p in ptrs {
                let p = p.as_str().unwrap();
                let first = payloads[0].pointer(p);
                for other in &payloads[1..] {
                    assert_eq!(first, other.pointer(p), "{id}: {p} differs across repeats");
                }
            }
        }
        if expect.get("capabilities_agree") == Some(&json!(true)) {
            let caps = &payloads[0];
            // input_forms.path equals whether a path input works.
            let probe = client.call("render", json!({ "document": { "path": "fixtures/simple.html" } }));
            let path_works = !probe["isError"].as_bool().unwrap_or(false);
            assert_eq!(caps["input_forms"]["path"], json!(path_works), "{id}: input_forms.path");
            // Every non-null limit has a limit-<name> case.
            if let Some(limits) = caps["limits"].as_object() {
                for (k, v) in limits {
                    if !v.is_null() {
                        assert!(limit_cases.contains(&format!("limit-{k}")) || cases.iter().any(|c| c.ends_with(format!("limit-{k}.json"))), "{id}: no case for limit {k}");
                    }
                }
            }
        }
        ran.push(id);
    }
    assert!(ran.len() >= 20, "ran only {} local cases: {ran:?}", ran.len());
}

/// Protocol-level behavior: unknown tool and unknown method are JSON-RPC
/// errors; a malformed line gets a parse error; a notification gets nothing.
#[test]
fn protocol_errors_are_jsonrpc_errors() {
    let mut client = Client::spawn(&suite_dir(), &[]);
    client.initialize();
    let r = client.request("tools/call", json!({ "name": "rasterize", "arguments": {} }));
    assert_eq!(r["error"]["code"], -32602, "unknown tool: {r}");
    let r = client.request("sampling/createMessage", json!({}));
    assert_eq!(r["error"]["code"], -32601, "unknown method: {r}");
    client.stdin.write_all(b"{not json\n").unwrap();
    client.stdin.flush().unwrap();
    let r = client.read();
    assert_eq!(r["error"]["code"], -32700, "parse error: {r}");
    // The server is still alive after a bad line.
    let r = client.request("ping", json!({}));
    assert_eq!(r["result"], json!({}));
}

/// Contract Behavior 4: `render` via MCP gives the same PDF bytes as the
/// CLI `render` command for the same input and options.
#[test]
fn mcp_render_bytes_equal_cli_render() {
    use base64::Engine as _;
    let dir = tempfile::tempdir().unwrap();
    let doc = dir.path().join("doc.html");
    std::fs::write(
        &doc,
        "<!DOCTYPE html><html><head><title>Same bytes</title><style>h1{color:#335}</style></head>\
         <body><h1>Heading</h1><p>Paragraph with <b>bold</b> text.</p>\
         <p style=\"break-before: page\">Second page.</p></body></html>",
    )
    .unwrap();
    for tagged in [false, true] {
        let cli_pdf = dir.path().join(format!("cli-{tagged}.pdf"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_typeanvil"));
        cmd.args(["render", doc.to_str().unwrap(), "--page-width", "5in", "--page-height", "3in"])
            .args(["--margin-top", "0.25in", "--margin-left", "0.5in", "--title", "T", "--author", "A"]);
        if !tagged {
            cmd.arg("--no-tagged");
        }
        let st = cmd.args(["-o", cli_pdf.to_str().unwrap()]).status().unwrap();
        assert!(st.success());
        let cli_bytes = std::fs::read(&cli_pdf).unwrap();

        let mut client = Client::spawn(dir.path(), &[]);
        client.initialize();
        let r = client.call(
            "render",
            json!({
                "document": { "path": "doc.html" },
                "options": {
                    "page_width": "5in", "page_height": "3in",
                    "margin_top": "0.25in", "margin_left": "0.5in",
                    "title": "T", "author": "A", "tagged": tagged
                }
            }),
        );
        assert_eq!(r["isError"], false, "{r}");
        let sc = &r["structuredContent"];
        let mcp_bytes = base64::engine::general_purpose::STANDARD
            .decode(sc["artifact"]["data_base64"].as_str().unwrap())
            .unwrap();
        assert_eq!(mcp_bytes.len(), cli_bytes.len(), "tagged={tagged}: byte length");
        assert!(mcp_bytes == cli_bytes, "tagged={tagged}: MCP render bytes differ from the CLI");
        assert_eq!(sc["byte_size"], json!(cli_bytes.len()));
        assert_eq!(sc["page_count"], 2);

        // output.mode = path writes the same bytes and returns an absolute path.
        let r = client.call(
            "render",
            json!({
                "document": { "path": "doc.html" },
                "options": {
                    "page_width": "5in", "page_height": "3in",
                    "margin_top": "0.25in", "margin_left": "0.5in",
                    "title": "T", "author": "A", "tagged": tagged
                },
                "output": { "mode": "path", "path": "out.pdf" }
            }),
        );
        let written = r["structuredContent"]["artifact"]["path"].as_str().unwrap();
        assert!(Path::new(written).is_absolute(), "artifact.path must be absolute: {written}");
        assert!(std::fs::read(written).unwrap() == cli_bytes, "tagged={tagged}: path artifact bytes");
        assert_eq!(r["structuredContent"]["sha256"], sc["sha256"]);
    }
}

/// JSON request mode: one request on stdin, one result on stdout, same
/// shapes as the MCP tool; exit 0 on success and 1 on a tool error.
#[test]
fn call_mode_matches_mcp_shapes() {
    let schemas = Schemas::new();
    let run = |tool: &str, body: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_typeanvil"))
            .args(["call", tool])
            .current_dir(suite_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(body.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        assert_eq!(text.lines().count(), 1, "exactly one JSON line on stdout: {text}");
        (out.status.code(), serde_json::from_str::<Value>(&text).unwrap())
    };
    let (code, v) = run("validate", r#"{"document":{"path":"fixtures/two-pages.html"},"assertions":[{"kind":"page_count","equals":2}]}"#);
    assert_eq!(code, Some(0));
    assert_eq!(v["ok"], true);
    let out_v = schemas.by_ref(&format!("{SCHEMA_BASE}validate.output.schema.json"));
    assert!(schemas.errors(&out_v, &v).is_empty(), "{v}");

    let (code, v) = run("inspect_page", r#"{"document":{"content":"<p>x</p>"},"page":3}"#);
    assert_eq!(code, Some(1));
    assert_eq!(v["code"], "PAGE_OUT_OF_RANGE");
    let err_v = schemas.by_ref(&format!("{SCHEMA_BASE}common.schema.json#/$defs/Error"));
    assert!(schemas.errors(&err_v, &v).is_empty(), "{v}");

    let (code, v) = run("render", "{not json");
    assert_eq!(code, Some(1));
    assert_eq!(v["code"], "INVALID_INPUT");

    let (code, v) = run("capabilities", "");
    assert_eq!(code, Some(0), "empty stdin is an empty request object");
    assert_eq!(v["input_forms"]["path"], true);
}

/// `--root` restricts readable paths: a path outside every root is
/// `PATH_OUTSIDE_ROOT`, one inside renders.
#[test]
fn roots_restrict_paths() {
    let inside = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(inside.path().join("in.html"), "<p>in</p>").unwrap();
    std::fs::write(outside.path().join("out.html"), "<p>out</p>").unwrap();
    let mut client = Client::spawn(inside.path(), &["--root", inside.path().to_str().unwrap()]);
    client.initialize();
    let r = client.call("render", json!({ "document": { "path": "in.html" } }));
    assert_eq!(r["isError"], false, "{r}");
    let r = client.call(
        "render",
        json!({ "document": { "path": outside.path().join("out.html").to_str().unwrap() } }),
    );
    assert_eq!(r["structuredContent"]["code"], "PATH_OUTSIDE_ROOT", "{r}");
    assert_eq!(r["structuredContent"]["field"], "/document/path");
    let r = client.call("render", json!({ "document": { "path": "../x/../../etc/passwd" } }));
    assert_eq!(r["structuredContent"]["code"], "PATH_OUTSIDE_ROOT", "{r}");
    let r = client.call(
        "render",
        json!({ "document": { "content": "<p>x</p>" }, "output": { "mode": "path", "path": outside.path().join("o.pdf").to_str().unwrap() } }),
    );
    assert_eq!(r["structuredContent"]["code"], "PATH_OUTSIDE_ROOT", "{r}");
    assert!(!outside.path().join("o.pdf").exists());
}

/// The inspect_page PNG is a real raster of the page: white background with
/// dark text pixels where the text box is.
#[test]
fn inspect_page_png_shows_page_content() {
    use base64::Engine as _;
    let mut client = Client::spawn(&suite_dir(), &[]);
    client.initialize();
    let r = client.call(
        "inspect_page",
        json!({
            "document": { "content": "<body style=\"margin:0\"><div style=\"background:#000;width:72pt;height:72pt\"></div></body>" },
            "options": { "page_width": "2in", "page_height": "2in" },
            "page": 1,
            "image": { "dpi": 72 }
        }),
    );
    assert_eq!(r["isError"], false, "{r}");
    let img = &r["structuredContent"]["image"];
    assert_eq!(img["width_px"], 144);
    assert_eq!(img["height_px"], 144);
    let png = base64::engine::general_purpose::STANDARD.decode(img["data_base64"].as_str().unwrap()).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
    let boxes = r["structuredContent"]["boxes"].as_array().unwrap();
    let bg = boxes.iter().find(|b| b["kind"] == "background").expect("a background box");
    assert_eq!(bg["rect"], json!({ "x": 0.0, "y": 0.0, "width": 72.0, "height": 72.0 }));
}
