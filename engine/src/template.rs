// SPDX-License-Identifier: AGPL-3.0-only

//! Data template mode (CORE-576, `docs/specifications/data-templates.spec.md`).
//!
//! A template is HTML + CSS with MiniJinja placeholders. The caller supplies
//! JSON data; an optional JSON Schema next to the template validates the data
//! before expansion. The expanded HTML then renders through the normal
//! pipeline.
//!
//! Sandbox: no loader is registered (so `include`/`import`/`extends` fail),
//! auto-escape is always HTML, and a fuel budget bounds runaway loops.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use minijinja::{AutoEscape, Environment, ErrorKind};
use serde_json::Value;

/// Fuel budget (spec rule 8).
pub const FUEL: u64 = 10_000_000;

/// One template-mode error. `pointer` is set for schema violations only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateError {
    pub code: &'static str,
    pub pointer: Option<String>,
    pub line: Option<usize>,
    pub message: String,
}

impl TemplateError {
    fn new(code: &'static str, message: String) -> Self {
        TemplateError { code, pointer: None, line: None, message }
    }
}

/// A render-stopping list of template errors (exit 5, no PDF).
#[derive(Debug)]
pub struct TemplateErrors(pub Vec<TemplateError>);

impl std::fmt::Display for TemplateErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "[{}] ", e.code)?;
            match (&e.pointer, e.line) {
                (Some(p), _) => write!(f, "{p}: ")?,
                (None, Some(l)) => write!(f, "line {l}: ")?,
                _ => {}
            }
            f.write_str(&e.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for TemplateErrors {}

impl From<TemplateError> for TemplateErrors {
    fn from(e: TemplateError) -> Self {
        TemplateErrors(vec![e])
    }
}

/// The sibling schema path: `x.tmpl.html` → `x.schema.json`; otherwise the
/// last extension is replaced (`x.html` → `x.schema.json`).
pub fn schema_path_for(template: &Path) -> PathBuf {
    let name = template.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if let Some(s) = name.strip_suffix(".tmpl.html") {
        s.to_string()
    } else {
        match name.rfind('.') {
            Some(i) if i > 0 => name[..i].to_string(),
            _ => name.clone(),
        }
    };
    template.with_file_name(format!("{stem}.schema.json"))
}

/// Load the schema: `explicit` when given, else the sibling file when it
/// exists. `Ok(None)` = no schema (no validation).
pub fn load_schema(template: &Path, explicit: Option<&Path>) -> Result<Option<Value>, TemplateErrors> {
    let path = match explicit {
        Some(p) => p.to_path_buf(),
        None => {
            let p = schema_path_for(template);
            if !p.is_file() {
                return Ok(None);
            }
            p
        }
    };
    let text = std::fs::read_to_string(&path).map_err(|e| {
        TemplateError::new("TEMPLATE_SCHEMA_INVALID", format!("reading {}: {e}", path.display()))
    })?;
    let v: Value = serde_json::from_str(&text).map_err(|e| {
        TemplateError::new("TEMPLATE_SCHEMA_INVALID", format!("{}: {e}", path.display()))
    })?;
    if !(v.is_object() || v.is_boolean()) {
        return Err(TemplateError::new(
            "TEMPLATE_SCHEMA_INVALID",
            format!("{}: a schema must be an object or a boolean", path.display()),
        )
        .into());
    }
    Ok(Some(v))
}

/// Parse the data file text (spec rule 2).
pub fn parse_data(text: &str) -> Result<Value, TemplateErrors> {
    serde_json::from_str(text).map_err(|e| {
        let mut err = TemplateError::new("TEMPLATE_DATA_INVALID_JSON", e.to_string());
        err.line = Some(e.line());
        err.into()
    })
}

/// Validate `data` against `schema`; every violation, document order, at
/// most one per pointer (spec rules 12–13).
pub fn validate_data(schema: &Value, data: &Value) -> Vec<TemplateError> {
    let mut out: Vec<(String, String)> = Vec::new();
    check(schema, data, "", &mut out);
    let mut seen: Vec<String> = Vec::new();
    let mut errs = Vec::new();
    for (ptr, msg) in out {
        if seen.contains(&ptr) {
            continue;
        }
        seen.push(ptr.clone());
        errs.push(TemplateError {
            code: "TEMPLATE_DATA_SCHEMA",
            pointer: Some(ptr),
            line: None,
            message: msg,
        });
    }
    errs
}

fn ptr_or_root(p: &str) -> String {
    if p.is_empty() {
        "/".to_string()
    } else {
        p.to_string()
    }
}

fn escape(k: &str) -> String {
    k.replace('~', "~0").replace('/', "~1")
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        "number" => v.is_number(),
        "integer" => v.as_f64().is_some_and(|f| f.fract() == 0.0),
        _ => true,
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Collecting validator. Pushes `(pointer, message)` pairs.
fn check(schema: &Value, v: &Value, ptr: &str, out: &mut Vec<(String, String)>) {
    let Some(s) = schema.as_object() else {
        if schema == &Value::Bool(false) {
            out.push((ptr_or_root(ptr), "no value is allowed here".into()));
        }
        return;
    };
    // Messages for this location, appended to `out` after the scalar checks.
    let mut here: Vec<String> = Vec::new();
    let mut fail = |m: String| here.push(m);

    if let Some(t) = s.get("type") {
        let ok = match t {
            Value::String(t) => type_ok(t, v),
            Value::Array(ts) => ts.iter().filter_map(Value::as_str).any(|t| type_ok(t, v)),
            _ => true,
        };
        if !ok {
            // A wrong type makes the remaining keywords meaningless here.
            out.push((ptr_or_root(ptr), format!("expected type {t}, got {}", type_name(v))));
            return;
        }
    }
    if let Some(Value::Array(options)) = s.get("enum") {
        if !options.contains(v) {
            fail(format!("value {v} is not one of {}", Value::Array(options.clone())));
        }
    }
    if let Some(c) = s.get("const") {
        if c != v {
            fail(format!("value must be {c}"));
        }
    }
    if let Some(n) = v.as_f64() {
        if let Some(min) = s.get("minimum").and_then(Value::as_f64) {
            if n < min {
                fail(format!("{n} is below the minimum {min}"));
            }
        }
        if let Some(max) = s.get("maximum").and_then(Value::as_f64) {
            if n > max {
                fail(format!("{n} is above the maximum {max}"));
            }
        }
    }
    if let Some(text) = v.as_str() {
        let len = text.chars().count() as u64;
        if let Some(min) = s.get("minLength").and_then(Value::as_u64) {
            if len < min {
                fail(format!("string is shorter than {min}"));
            }
        }
        if let Some(max) = s.get("maxLength").and_then(Value::as_u64) {
            if len > max {
                fail(format!("string is longer than {max}"));
            }
        }
    }
    if let Some(items) = v.as_array() {
        if let Some(min) = s.get("minItems").and_then(Value::as_u64) {
            if (items.len() as u64) < min {
                fail(format!("array has fewer than {min} items"));
            }
        }
        if let Some(max) = s.get("maxItems").and_then(Value::as_u64) {
            if (items.len() as u64) > max {
                fail(format!("array has more than {max} items"));
            }
        }
    }
    out.extend(here.into_iter().map(|m| (ptr_or_root(ptr), m)));

    if let Some(obj) = v.as_object() {
        if let Some(Value::Array(req)) = s.get("required") {
            for r in req.iter().filter_map(Value::as_str) {
                if !obj.contains_key(r) {
                    out.push((
                        format!("{ptr}/{}", escape(r)),
                        format!("missing required property `{r}`"),
                    ));
                }
            }
        }
        let props = s.get("properties").and_then(Value::as_object);
        if let Some(props) = props {
            for (k, sub) in props {
                if let Some(val) = obj.get(k) {
                    check(sub, val, &format!("{ptr}/{}", escape(k)), out);
                }
            }
        }
        if let Some(ap) = s.get("additionalProperties") {
            for (k, val) in obj {
                if props.is_some_and(|p| p.contains_key(k)) {
                    continue;
                }
                let child = format!("{ptr}/{}", escape(k));
                if ap == &Value::Bool(false) {
                    out.push((child, format!("property `{k}` is not allowed")));
                } else {
                    check(ap, val, &child, out);
                }
            }
        }
    }

    if let Some(items) = v.as_array() {
        if let Some(item_schema) = s.get("items") {
            for (i, item) in items.iter().enumerate() {
                check(item_schema, item, &format!("{ptr}/{i}"), out);
            }
        }
        if s.get("uniqueItems") == Some(&Value::Bool(true)) {
            for i in 0..items.len() {
                if items[..i].contains(&items[i]) {
                    out.push((format!("{ptr}/{i}"), "duplicate array item".into()));
                }
            }
        }
    }

    if let Some(Value::Array(branches)) = s.get("oneOf") {
        let passing = branches
            .iter()
            .filter(|b| {
                let mut tmp = Vec::new();
                check(b, v, ptr, &mut tmp);
                tmp.is_empty()
            })
            .count();
        if passing != 1 {
            out.push((
                ptr_or_root(ptr),
                format!("value must match exactly one oneOf branch, matched {passing}"),
            ));
        }
    }

    if let Some(cond) = s.get("if") {
        let mut tmp = Vec::new();
        check(cond, v, ptr, &mut tmp);
        if tmp.is_empty() {
            if let Some(then) = s.get("then") {
                check(then, v, ptr, out);
            }
        }
    }
}

/// Expand the template with `data` as context (spec rules 3–8).
pub fn expand(source: &str, data: &Value) -> Result<String, TemplateErrors> {
    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    env.set_fuel(Some(FUEL));
    // `x | fixed(2)`: a number with exactly N decimals (money, percentages).
    env.add_filter("fixed", |v: f64, digits: Option<u32>| -> String {
        format!("{:.*}", digits.unwrap_or(2).min(12) as usize, v)
    });
    // No loader: include/import/extends cannot reach the filesystem.

    let mut ctx = serde_json::Map::new();
    if let Value::Object(m) = data {
        for (k, v) in m {
            ctx.insert(k.clone(), v.clone());
        }
    }
    ctx.insert("data".to_string(), data.clone());
    let ctx = minijinja::Value::from(minijinja::value::Serde(Value::Object(ctx)));

    let to_err = |e: minijinja::Error| {
        let mut msg = e.to_string();
        if e.kind() == ErrorKind::OutOfFuel {
            msg = format!("template exceeded its fuel budget ({FUEL} instructions): {msg}");
        }
        let mut err = TemplateError::new("TEMPLATE_ERROR", msg);
        err.line = e.line();
        TemplateErrors::from(err)
    };
    let tmpl = env.template_from_str(source).map_err(to_err)?;
    tmpl.render(ctx).map_err(to_err)
}

/// JSON pointer or line rendering for stderr lines.
pub fn location(e: &TemplateError) -> String {
    match (&e.pointer, e.line) {
        (Some(p), _) => p.clone(),
        (None, Some(l)) => format!("line {l}"),
        _ => "-".to_string(),
    }
}

/// `{"schema": 1, "template_errors": [...]}` (spec rule 16).
pub fn errors_json(errors: &[TemplateError]) -> String {
    let list: Vec<Value> = errors
        .iter()
        .map(|e| {
            serde_json::json!({
                "code": e.code,
                "pointer": e.pointer,
                "line": e.line,
                "message": e.message,
            })
        })
        .collect();
    let mut s = serde_json::to_string_pretty(&serde_json::json!({
        "schema": 1,
        "template_errors": list,
    }))
    .expect("JSON serializes");
    let _ = writeln!(s);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sibling_schema_path() {
        assert_eq!(schema_path_for(Path::new("a/inv.tmpl.html")), PathBuf::from("a/inv.schema.json"));
        assert_eq!(schema_path_for(Path::new("b.html")), PathBuf::from("b.schema.json"));
    }

    #[test]
    fn collects_every_violation() {
        let s = json!({"type":"object","required":["a","b"],
            "properties":{"c":{"type":"array","items":{"type":"number","minimum":0}}}});
        let e = validate_data(&s, &json!({"c":[1,-2,"x"]}));
        let ptrs: Vec<_> = e.iter().map(|e| e.pointer.clone().unwrap()).collect();
        assert_eq!(ptrs, ["/a", "/b", "/c/1", "/c/2"]);
    }

    #[test]
    fn escapes_by_default_and_safe_opts_out() {
        let out = expand("{{ x }}|{{ x|safe }}", &json!({"x":"<b>"})).unwrap();
        assert_eq!(out, "&lt;b&gt;|<b>");
    }

    #[test]
    fn fuel_stops_runaway_loops() {
        let e = expand("{% for i in range(10000) %}{% for j in range(10000) %}x{% endfor %}{% endfor %}", &json!({})).unwrap_err();
        assert_eq!(e.0[0].code, "TEMPLATE_ERROR");
    }
}
