// SPDX-License-Identifier: AGPL-3.0-only

//! Tool errors and the contract's input forms: `ContentSource` (path,
//! content, url), assets, path roots and URL fetch.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use serde_json::{json, Map, Value};

use super::schema::{self, Kind};

/// A contract tool error (`common.schema.json#/$defs/Error`).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
    pub field: Option<String>,
}

impl ToolError {
    pub fn new(code: &'static str, message: &str) -> ToolError {
        ToolError { code, message: message.to_string(), field: None }
    }

    pub fn at(code: &'static str, field: &str, message: &str) -> ToolError {
        ToolError { code, message: message.to_string(), field: Some(field.to_string()) }
    }

    /// `retryable` per the contract's error table.
    pub fn retryable(&self) -> bool {
        matches!(self.code, "URL_FETCH_FAILED" | "LIMIT_TIMEOUT" | "RATE_LIMITED" | "INTERNAL")
    }

    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("code".into(), json!(self.code));
        m.insert("message".into(), json!(self.message));
        m.insert("retryable".into(), json!(self.retryable()));
        if let Some(f) = &self.field {
            m.insert("field".into(), json!(f));
        }
        m.insert("environment".into(), super::environment());
        Value::Object(m)
    }
}

pub type ToolResult<T> = Result<T, ToolError>;

/// Make a path absolute against the process working directory.
pub fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf())
    }
}

/// Lexically normalize `.` and `..` (for a path that may not exist yet).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Check the input rules that run BEFORE schema validation (contract Input
/// forms Behavior 4): each `ContentSource` has exactly one of path, content,
/// url, and each asset exactly one of path, url, content_base64.
pub fn check_sources(args: &Value) -> ToolResult<()> {
    let Some(obj) = args.as_object() else { return Ok(()) };
    let one = |v: &Value, ptr: &str, keys: &[&str]| -> ToolResult<()> {
        let Some(o) = v.as_object() else { return Ok(()) };
        let n = keys.iter().filter(|k| o.contains_key(**k)).count();
        if n == 1 {
            Ok(())
        } else {
            Err(ToolError::at(
                "INPUT_SOURCE_AMBIGUOUS",
                ptr,
                &format!("give exactly one of {} (found {n})", keys.join(", ")),
            ))
        }
    };
    const DOC: [&str; 3] = ["path", "content", "url"];
    if let Some(d) = obj.get("document") {
        one(d, "/document", &DOC)?;
    }
    if let Some(Value::Array(sheets)) = obj.get("stylesheets") {
        for (i, s) in sheets.iter().enumerate() {
            one(s, &format!("/stylesheets/{i}"), &DOC)?;
        }
    }
    if let Some(Value::Object(assets)) = obj.get("assets") {
        for (k, a) in assets {
            one(a, &format!("/assets/{}", schema::escape(k)), &["path", "url", "content_base64"])?;
        }
    }
    Ok(())
}

/// Validate the arguments against the tool's published input schema.
pub fn check_schema(tool: &str, args: &Value) -> ToolResult<()> {
    match schema::validate(&schema::published(tool, Kind::Input), args) {
        None => Ok(()),
        Some(v) => Err(ToolError::at("INVALID_INPUT", &v.pointer, &v.message)),
    }
}

/// A loaded document or stylesheet.
pub struct Loaded {
    pub text: String,
    /// The real file for `path` inputs; `None` for content and url.
    pub file: Option<PathBuf>,
    pub markdown: bool,
}

/// Path policy and fetch helpers for one call.
pub struct Access<'a> {
    pub roots: &'a [PathBuf],
}

impl Access<'_> {
    /// Resolve a caller path: absolute, inside the roots, existing.
    pub fn read_path(&self, raw: &str, ptr: &str) -> ToolResult<(PathBuf, Vec<u8>)> {
        let abs = absolute(Path::new(raw));
        let real = std::fs::canonicalize(&abs);
        let checked = real.as_ref().map(|p| p.clone()).unwrap_or_else(|_| normalize(&abs));
        self.check_root(&checked, ptr)?;
        let real = real.map_err(|_| {
            ToolError::at("PATH_NOT_FOUND", ptr, &format!("no such file: {}", abs.display()))
        })?;
        let bytes = std::fs::read(&real).map_err(|e| {
            ToolError::at("PATH_NOT_FOUND", ptr, &format!("cannot read {}: {e}", real.display()))
        })?;
        Ok((real, bytes))
    }

    /// `PATH_OUTSIDE_ROOT` unless `p` lies inside a configured root.
    pub fn check_root(&self, p: &Path, ptr: &str) -> ToolResult<()> {
        if self.roots.is_empty() || self.roots.iter().any(|r| p.starts_with(r)) {
            Ok(())
        } else {
            Err(ToolError::at(
                "PATH_OUTSIDE_ROOT",
                ptr,
                &format!("{} is outside the configured roots", p.display()),
            ))
        }
    }

    /// The absolute output path for `output.mode: path`, inside the roots.
    pub fn output_path(&self, raw: &str) -> ToolResult<PathBuf> {
        let abs = normalize(&absolute(Path::new(raw)));
        let parent = abs.parent().map(Path::to_path_buf).unwrap_or_default();
        let real_parent = std::fs::canonicalize(&parent).map_err(|_| {
            ToolError::at(
                "PATH_NOT_FOUND",
                "/output/path",
                &format!("output directory does not exist: {}", parent.display()),
            )
        })?;
        let target = real_parent.join(abs.file_name().unwrap_or_default());
        self.check_root(&target, "/output/path")?;
        Ok(target)
    }

    /// Load one `ContentSource` (already checked for exactly one key).
    pub fn load(&self, src: &Value, ptr: &str) -> ToolResult<Loaded> {
        let format = src.get("format").and_then(Value::as_str);
        let (bytes, file, name) = if let Some(p) = src.get("path").and_then(Value::as_str) {
            let (real, bytes) = self.read_path(p, &format!("{ptr}/path"))?;
            let name = real.to_string_lossy().into_owned();
            (bytes, Some(real), Some(name))
        } else if let Some(c) = src.get("content").and_then(Value::as_str) {
            (c.as_bytes().to_vec(), None, None)
        } else {
            let url = src.get("url").and_then(Value::as_str).unwrap_or_default();
            let bytes = fetch(url, &format!("{ptr}/url"))?;
            (bytes, None, Some(url_path(url).to_string()))
        };
        let text = String::from_utf8(bytes).map_err(|_| {
            ToolError::at("PARSE_FAILED", ptr, "the input is not valid UTF-8 text")
        })?;
        let markdown = match format {
            Some("markdown") => true,
            Some(_) => false,
            None => name.as_deref().is_some_and(crate::pipeline::is_markdown_path),
        };
        Ok(Loaded { text, file, markdown })
    }

    /// The bytes of one asset source.
    pub fn asset_bytes(&self, src: &Value, ptr: &str) -> ToolResult<Vec<u8>> {
        if let Some(p) = src.get("path").and_then(Value::as_str) {
            Ok(self.read_path(p, &format!("{ptr}/path"))?.1)
        } else if let Some(u) = src.get("url").and_then(Value::as_str) {
            fetch(u, &format!("{ptr}/url"))
        } else {
            let b64 = src.get("content_base64").and_then(Value::as_str).unwrap_or_default();
            base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|e| {
                    ToolError::at("INVALID_INPUT", &format!("{ptr}/content_base64"), &format!("not valid base64: {e}"))
                })
        }
    }
}

/// The path part of a URL (query and fragment removed), for format inference.
fn url_path(url: &str) -> &str {
    let no_frag = url.split('#').next().unwrap_or(url);
    no_frag.split('?').next().unwrap_or(no_frag)
}

/// Fetch an `http`/`https` URL with the system `curl` (no TLS stack in the
/// engine). Other schemes are refused by policy (`URL_BLOCKED`). A failure,
/// non-2xx status or a 30-second timeout is `URL_FETCH_FAILED`.
pub fn fetch(url: &str, ptr: &str) -> ToolResult<Vec<u8>> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(ToolError::at(
            "URL_BLOCKED",
            ptr,
            "only http and https URLs are fetched",
        ));
    }
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--proto", "=http,https", "--max-time", "30", "--", url])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| {
            ToolError::at("URL_FETCH_FAILED", ptr, &format!("cannot run curl to fetch the URL: {e}"))
        })?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(ToolError::at(
            "URL_FETCH_FAILED",
            ptr,
            &format!("fetching {url} failed: {why}"),
        ));
    }
    Ok(out.stdout)
}

/// A per-call scratch directory, removed on drop. Holds materialized assets
/// and stands in as the directory of inline / URL documents, so a relative
/// reference with no `base_url` resolves to nothing (Input forms Behavior 6).
pub struct Scratch {
    pub dir: PathBuf,
}

impl Scratch {
    pub fn new() -> ToolResult<Scratch> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("typeanvil-mcp-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| {
            ToolError::new("INTERNAL", &format!("cannot create a scratch directory: {e}"))
        })?;
        let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        Ok(Scratch { dir })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Resolve `base_url` to a local directory: an absolute path or a `file://`
/// URL. Remote bases cannot serve local reads; they resolve to nothing.
pub fn base_dir(base: &str) -> Option<PathBuf> {
    if let Some(rest) = base.strip_prefix("file://") {
        let p = PathBuf::from(percent_decode(rest));
        return Some(if base.ends_with('/') { p } else { p.parent()?.to_path_buf() });
    }
    let p = Path::new(base);
    if p.is_absolute() {
        return Some(if base.ends_with('/') || p.is_dir() { p.to_path_buf() } else { p.parent()?.to_path_buf() });
    }
    None
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
