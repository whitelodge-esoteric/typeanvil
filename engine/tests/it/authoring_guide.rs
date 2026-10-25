// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-579: agent authoring guide (`docs/specifications/agent-authoring-guide.spec.md`).
//!
//! The committed docs page and skill file must equal the generated guide.
//! Regenerate with `UPDATE_GUIDE=1 cargo test --test it authoring_guide`.

use std::path::PathBuf;

use serde_json::json;
use typeanvil::capabilities::{manifest, Status};
use typeanvil::diagnostics::analyze;
use typeanvil::guide;
use typeanvil::mcp::Server;

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(rel)
}

fn check_committed(rel: &str, expected: String) {
    let path = repo_file(rel);
    if std::env::var("UPDATE_GUIDE").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    let actual = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        actual == expected,
        "{rel} is out of date with the capability manifest. Regenerate: \
         UPDATE_GUIDE=1 cargo test --test it authoring_guide"
    );
}

/// AC1/AC2: the docs page is generated and current.
#[test]
fn docs_page_is_current() {
    check_committed(guide::DOCS_PAGE_PATH, guide::docs_page());
}

/// AC1/AC2: the skill file is generated and current.
#[test]
fn skill_file_is_current() {
    check_committed(guide::SKILL_PATH, guide::skill_file());
}

/// AC3: every supported property and every unsupported entry of the manifest
/// appears in the guide.
#[test]
fn guide_lists_every_manifest_entry() {
    let body = guide::body();
    for c in manifest() {
        match (c.kind, c.status) {
            ("property", _) | ("at-rule", Status::Unsupported) => {
                assert!(body.contains(&format!("`{}`", c.name)) || body.contains(&format!("`@{}`", c.name)), "{} missing", c.id());
            }
            _ => {}
        }
    }
}

/// AC4: the guide's own example stylesheet draws no diagnostics.
#[test]
fn skeleton_css_has_no_diagnostics() {
    let body = guide::body();
    // The skeleton's tag stands alone on a line; the workflow text mentions it inline.
    let start = body.find("<style>\n").expect("skeleton <style>") + "<style>".len();
    let end = body[start..].find("</style>").expect("</style>") + start;
    let css = &body[start..end];
    let events = analyze(css);
    assert!(events.is_empty(), "skeleton css flagged: {:?}", events.iter().map(|e| e.message.clone()).collect::<Vec<_>>());
}

fn server() -> Server {
    Server::new(vec![])
}

/// AC5: the MCP server lists and serves the guide as a resource and a prompt.
#[test]
fn mcp_exposes_resource_and_prompt() {
    let s = server();
    let r = s.handle_line(&json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}).to_string()).unwrap();
    assert_eq!(r["result"]["resources"][0]["uri"], guide::RESOURCE_URI);
    let r = s
        .handle_line(&json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":guide::RESOURCE_URI}}).to_string())
        .unwrap();
    assert_eq!(r["result"]["contents"][0]["text"], guide::body());
    let r = s.handle_line(&json!({"jsonrpc":"2.0","id":3,"method":"prompts/list"}).to_string()).unwrap();
    assert_eq!(r["result"]["prompts"][0]["name"], guide::PROMPT_NAME);
    let r = s
        .handle_line(&json!({"jsonrpc":"2.0","id":4,"method":"prompts/get","params":{"name":guide::PROMPT_NAME,"arguments":{"task":"a 2-page report"}}}).to_string())
        .unwrap();
    let text = r["result"]["messages"][0]["content"]["text"].as_str().unwrap();
    assert!(text.contains("a 2-page report") && text.contains("# Writing HTML for TypeAnvil"));
    let r = s
        .handle_line(&json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"typeanvil://nope"}}).to_string())
        .unwrap();
    assert_eq!(r["error"]["code"], -32002);
}
