// SPDX-License-Identifier: AGPL-3.0-only

//! The contract schemas (`schemas/mcp/v1/`), embedded at build time, and a
//! small JSON Schema 2020-12 validator for the keywords those files use.
//!
//! Published schemas are DEREFERENCED: every `common.schema.json#/$defs/X`
//! (and `#/$defs/X` inside common) reference is replaced by the definition
//! it names. MCP clients receive one self-contained schema per tool and never
//! need to fetch `common.schema.json`. Dereferencing does not change what a
//! schema accepts (mcp-server spec, Behavior S4).

use serde_json::{Map, Value};

/// Input or output schema of a tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Input,
    Output,
}

const COMMON: &str = include_str!("../../../schemas/mcp/v1/common.schema.json");

/// The raw schema file text for one tool.
pub fn raw(tool: &str, kind: Kind) -> &'static str {
    match (tool, kind) {
        ("render", Kind::Input) => include_str!("../../../schemas/mcp/v1/render.input.schema.json"),
        ("render", Kind::Output) => include_str!("../../../schemas/mcp/v1/render.output.schema.json"),
        ("validate", Kind::Input) => include_str!("../../../schemas/mcp/v1/validate.input.schema.json"),
        ("validate", Kind::Output) => include_str!("../../../schemas/mcp/v1/validate.output.schema.json"),
        ("inspect_page", Kind::Input) => {
            include_str!("../../../schemas/mcp/v1/inspect_page.input.schema.json")
        }
        ("inspect_page", Kind::Output) => {
            include_str!("../../../schemas/mcp/v1/inspect_page.output.schema.json")
        }
        ("capabilities", Kind::Input) => {
            include_str!("../../../schemas/mcp/v1/capabilities.input.schema.json")
        }
        ("capabilities", Kind::Output) => {
            include_str!("../../../schemas/mcp/v1/capabilities.output.schema.json")
        }
        ("explain", Kind::Input) => include_str!("../../../schemas/mcp/v1/explain.input.schema.json"),
        ("explain", Kind::Output) => include_str!("../../../schemas/mcp/v1/explain.output.schema.json"),
        ("templates", Kind::Input) => include_str!("../../../schemas/mcp/v1/templates.input.schema.json"),
        ("templates", Kind::Output) => include_str!("../../../schemas/mcp/v1/templates.output.schema.json"),
        _ => panic!("no schema for tool `{tool}`"),
    }
}

fn common() -> Value {
    serde_json::from_str(COMMON).expect("common.schema.json parses")
}

/// The published (dereferenced) schema for one tool.
pub fn published(tool: &str, kind: Kind) -> Value {
    let file: Value = serde_json::from_str(raw(tool, kind)).expect("schema file parses");
    deref(&file, &common(), false)
}

/// The dereferenced `common.schema.json#/$defs/Error` schema.
pub fn error_schema() -> Value {
    let c = common();
    deref(&c["$defs"]["Error"], &c, true)
}

/// Replace `$ref`s that point into common.schema.json. `in_common` makes
/// local `#/$defs/X` refs resolve against common too.
pub fn deref(v: &Value, common: &Value, in_common: bool) -> Value {
    match v {
        Value::Object(m) => {
            if let Some(Value::String(r)) = m.get("$ref") {
                let name = r
                    .strip_prefix("common.schema.json#/$defs/")
                    .or_else(|| if in_common { r.strip_prefix("#/$defs/") } else { None });
                if let Some(name) = name {
                    let def = common["$defs"]
                        .get(name)
                        .unwrap_or_else(|| panic!("unknown $ref {r}"));
                    let mut out = match deref(def, common, true) {
                        Value::Object(d) => d,
                        other => return other,
                    };
                    // Sibling keywords next to $ref (none today) are kept.
                    for (k, val) in m {
                        if k != "$ref" {
                            out.insert(k.clone(), deref(val, common, in_common));
                        }
                    }
                    return Value::Object(out);
                }
            }
            let mut out = Map::new();
            for (k, val) in m {
                out.insert(k.clone(), deref(val, common, in_common));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| deref(x, common, in_common)).collect()),
        other => other.clone(),
    }
}

/// One validation failure: the JSON pointer of the instance and a reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub pointer: String,
    pub message: String,
}

/// Validate `instance` against a dereferenced schema. Returns the first
/// violation found in document order (`None` = valid).
///
/// Supported keywords (all the contract files use): `type`, `enum`, `const`,
/// `properties`, `required`, `additionalProperties` (bool or schema),
/// `unevaluatedProperties: false` (properties evaluated by `properties` and
/// a passing `oneOf` branch), `items`, `uniqueItems`, `minimum`, `maximum`,
/// `minLength`, `pattern` (the `^[0-9a-f]{64}$` and `MAJOR.MINOR` forms),
/// `oneOf`, `if`/`then`. Annotations (`format`, `default`, `description`,
/// `contentEncoding`) are ignored, as 2020-12 allows.
pub fn validate(schema: &Value, instance: &Value) -> Option<Violation> {
    check(schema, instance, "").err()
}

fn fail(pointer: &str, message: String) -> Result<Vec<String>, Violation> {
    Err(Violation {
        pointer: if pointer.is_empty() { "/".to_string() } else { pointer.to_string() },
        message,
    })
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

/// Check one schema; on success returns the property names it evaluated
/// (for `unevaluatedProperties`).
fn check(schema: &Value, v: &Value, ptr: &str) -> Result<Vec<String>, Violation> {
    let Some(s) = schema.as_object() else {
        // `true` / `{}`-like schemas.
        if schema == &Value::Bool(false) {
            return fail(ptr, "no value is allowed here".into());
        }
        return Ok(Vec::new());
    };
    let mut evaluated: Vec<String> = Vec::new();

    if let Some(t) = s.get("type") {
        let ok = match t {
            Value::String(t) => type_ok(t, v),
            Value::Array(ts) => ts.iter().filter_map(Value::as_str).any(|t| type_ok(t, v)),
            _ => true,
        };
        if !ok {
            return fail(ptr, format!("expected type {t}, got {}", type_name(v)));
        }
    }
    if let Some(Value::Array(options)) = s.get("enum") {
        if !options.contains(v) {
            return fail(ptr, format!("value {v} is not one of {}", Value::Array(options.clone())));
        }
    }
    if let Some(c) = s.get("const") {
        if c != v {
            return fail(ptr, format!("value must be {c}"));
        }
    }
    if let Some(n) = v.as_f64() {
        if let Some(min) = s.get("minimum").and_then(Value::as_f64) {
            if n < min {
                return fail(ptr, format!("{n} is below the minimum {min}"));
            }
        }
        if let Some(max) = s.get("maximum").and_then(Value::as_f64) {
            if n > max {
                return fail(ptr, format!("{n} is above the maximum {max}"));
            }
        }
    }
    if let Some(text) = v.as_str() {
        if let Some(min) = s.get("minLength").and_then(Value::as_u64) {
            if (text.chars().count() as u64) < min {
                return fail(ptr, format!("string is shorter than {min}"));
            }
        }
        if let Some(Value::String(p)) = s.get("pattern") {
            if !pattern_ok(p, text) {
                return fail(ptr, format!("string does not match {p}"));
            }
        }
    }

    if let Some(obj) = v.as_object() {
        if let Some(Value::Array(req)) = s.get("required") {
            for r in req.iter().filter_map(Value::as_str) {
                if !obj.contains_key(r) {
                    return fail(ptr, format!("missing required property `{r}`"));
                }
            }
        }
        let props = s.get("properties").and_then(Value::as_object);
        if let Some(props) = props {
            for (k, sub) in props {
                if let Some(val) = obj.get(k) {
                    check(sub, val, &format!("{ptr}/{}", escape(k)))?;
                    evaluated.push(k.clone());
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
                    return fail(&child, format!("property `{k}` is not allowed"));
                }
                check(ap, val, &child)?;
                evaluated.push(k.clone());
            }
        }
    }

    if let Some(Value::Array(items)) = v.as_array().map(|a| Value::Array(a.clone())).as_ref() {
        if let Some(item_schema) = s.get("items") {
            for (i, item) in items.iter().enumerate() {
                check(item_schema, item, &format!("{ptr}/{i}"))?;
            }
        }
        if s.get("uniqueItems") == Some(&Value::Bool(true)) {
            for i in 0..items.len() {
                if items[..i].contains(&items[i]) {
                    return fail(&format!("{ptr}/{i}"), "duplicate array item".into());
                }
            }
        }
    }

    if let Some(Value::Array(branches)) = s.get("oneOf") {
        let mut passing = Vec::new();
        for b in branches {
            if let Ok(ev) = check(b, v, ptr) {
                passing.push(ev);
            }
        }
        if passing.len() != 1 {
            return fail(
                ptr,
                format!("value must match exactly one oneOf branch, matched {}", passing.len()),
            );
        }
        evaluated.extend(passing.pop().unwrap());
    }

    if let Some(cond) = s.get("if") {
        if check(cond, v, ptr).is_ok() {
            if let Some(then) = s.get("then") {
                check(then, v, ptr)?;
            }
        }
    }

    if s.get("unevaluatedProperties") == Some(&Value::Bool(false)) {
        if let Some(obj) = v.as_object() {
            for k in obj.keys() {
                if !evaluated.contains(k) {
                    return fail(&format!("{ptr}/{}", escape(k)), format!("property `{k}` is not allowed"));
                }
            }
        }
    }
    Ok(evaluated)
}

/// JSON pointer token escaping (RFC 6901).
pub fn escape(k: &str) -> String {
    k.replace('~', "~0").replace('/', "~1")
}

/// The two pattern shapes the contract uses. Any other pattern is accepted
/// (a deliberate limit of this validator; a test pins the set).
fn pattern_ok(p: &str, s: &str) -> bool {
    match p {
        "^[0-9a-f]{64}$" => s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "^[0-9]+\\.[0-9]+$" => match s.split_once('.') {
            Some((a, b)) => {
                !a.is_empty() && !b.is_empty() && a.bytes().all(|c| c.is_ascii_digit()) && b.bytes().all(|c| c.is_ascii_digit())
            }
            None => false,
        },
        _ => true,
    }
}

/// Every `pattern` value used in the schema files (tests pin this set to the
/// patterns [`pattern_ok`] implements).
pub fn patterns_in(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, val) in m {
                if k == "pattern" {
                    if let Value::String(p) = val {
                        out.push(p.clone());
                    }
                }
                patterns_in(val, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| patterns_in(x, out)),
        _ => {}
    }
}

/// The raw text of `common.schema.json` (tests compare the error-code set).
pub fn common_raw() -> &'static str {
    COMMON
}
