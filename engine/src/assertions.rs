// SPDX-License-Identifier: AGPL-3.0-only

//! Intent assertions (CORE-573): layout goals with a pass/fail verdict.
//!
//! Spec: `docs/specifications/intent-assertions.spec.md`. Assertions are a
//! pure read of the final `Layout`, the DOM and the render report. They never
//! change the layout or the PDF. Every walk is in input, page, or DOM order,
//! so the output is deterministic.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::css::{BookmarkLevel, Stylesheet};
use crate::diagnostics::json_string;
use crate::dom::{Dom, NodeId};
use crate::frag::Fragment;
use crate::layout::Layout;
use crate::render_report::{selector_of, RenderReport};

/// One assertion kind with its parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    MaxPages(usize),
    ExactPages(usize),
    NoOverflow,
    NoEmptyPages,
    KeepTogether(String),
    OnPage(String, usize),
    OutlineCovers(String),
    FontsResolved,
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::MaxPages(_) => "max-pages",
            Kind::ExactPages(_) => "exact-pages",
            Kind::NoOverflow => "no-overflow",
            Kind::NoEmptyPages => "no-empty-pages",
            Kind::KeepTogether(_) => "keep-together",
            Kind::OnPage(..) => "on-page",
            Kind::OutlineCovers(_) => "outline-covers",
            Kind::FontsResolved => "fonts-resolved",
        }
    }
}

/// One assertion from the input file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assertion {
    pub id: Option<String>,
    pub kind: Kind,
}

/// One piece of evidence (field meanings as in the render report).
#[derive(Clone, Debug, PartialEq)]
pub struct Evidence {
    pub page: usize,
    pub rect: Option<[f64; 4]>,
    pub line: u32,
    pub column: u32,
    pub element: String,
}

/// The verdict for one assertion.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub index: usize,
    pub id: Option<String>,
    pub kind: &'static str,
    pub passed: bool,
    pub detail: String,
    pub evidence: Vec<Evidence>,
}

// ---------------------------------------------------------------------------
// Minimal JSON reader (the engine has no serde). Objects keep key order.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("invalid JSON at byte {}: {what}", self.i))
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn lit(&mut self, word: &str, v: Json) -> Result<Json, String> {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            self.err("unexpected token")
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > 64 {
            return self.err("nesting too deep");
        }
        self.ws();
        match self.s.get(self.i) {
            None => self.err("unexpected end of input"),
            Some(b'n') => self.lit("null", Json::Null),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::Str),
            Some(b'[') => {
                self.i += 1;
                let mut v = Vec::new();
                if self.eat(b']') {
                    return Ok(Json::Arr(v));
                }
                loop {
                    v.push(self.value(depth + 1)?);
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b']') {
                        return Ok(Json::Arr(v));
                    }
                    return self.err("expected `,` or `]`");
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut v: Vec<(String, Json)> = Vec::new();
                if self.eat(b'}') {
                    return Ok(Json::Obj(v));
                }
                loop {
                    self.ws();
                    if self.s.get(self.i) != Some(&b'"') {
                        return self.err("expected a string key");
                    }
                    let k = self.string()?;
                    if !self.eat(b':') {
                        return self.err("expected `:`");
                    }
                    let val = self.value(depth + 1)?;
                    if v.iter().any(|(e, _)| *e == k) {
                        return Err(format!("duplicate key `{k}`"));
                    }
                    v.push((k, val));
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b'}') {
                        return Ok(Json::Obj(v));
                    }
                    return self.err("expected `,` or `}`");
                }
            }
            Some(c) if *c == b'-' || c.is_ascii_digit() => {
                let start = self.i;
                while self.i < self.s.len()
                    && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
                match t.parse::<f64>() {
                    Ok(n) if n.is_finite() => Ok(Json::Num(n)),
                    _ => self.err("bad number"),
                }
            }
            Some(_) => self.err("unexpected character"),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1; // opening quote
        let mut out = String::new();
        loop {
            let Some(&c) = self.s.get(self.i) else {
                return self.err("unterminated string");
            };
            self.i += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(&e) = self.s.get(self.i) else {
                        return self.err("unterminated escape");
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self
                                .s
                                .get(self.i..self.i + 4)
                                .and_then(|h| std::str::from_utf8(h).ok())
                                .and_then(|h| u32::from_str_radix(h, 16).ok());
                            let Some(cp) = hex else {
                                return self.err("bad \\u escape");
                            };
                            self.i += 4;
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        }
                        _ => return self.err("bad escape"),
                    }
                }
                c if c < 0x20 => return self.err("control character in string"),
                _ => {
                    // Copy one UTF-8 code point (input is a &str, so valid).
                    let start = self.i - 1;
                    let mut end = self.i;
                    while end < self.s.len() && (self.s[end] & 0xC0) == 0x80 {
                        end += 1;
                    }
                    out.push_str(std::str::from_utf8(&self.s[start..end]).unwrap_or("\u{fffd}"));
                    self.i = end;
                }
            }
        }
    }
}

fn parse_json(text: &str) -> Result<Json, String> {
    let mut r = Reader { s: text.as_bytes(), i: 0 };
    let v = r.value(0)?;
    r.ws();
    if r.i != r.s.len() {
        return r.err("trailing characters");
    }
    Ok(v)
}

/// Parse an assertions file (spec Behavior 1–3). Errors are usage errors.
pub fn parse(text: &str) -> Result<Vec<Assertion>, String> {
    let Json::Arr(items) = parse_json(text)? else {
        return Err("assertions file must be a JSON array".to_string());
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let Json::Obj(fields) = item else {
            return Err(format!("assertion {i}: must be an object"));
        };
        let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        let kind = match get("kind") {
            Some(Json::Str(s)) => s.as_str(),
            Some(_) => return Err(format!("assertion {i}: `kind` must be a string")),
            None => return Err(format!("assertion {i}: missing `kind`")),
        };
        let id = match get("id") {
            None => None,
            Some(Json::Str(s)) => Some(s.clone()),
            Some(_) => return Err(format!("assertion {i}: `id` must be a string")),
        };
        let int = |k: &str, min: usize| -> Result<usize, String> {
            match get(k) {
                Some(Json::Num(n)) if n.fract() == 0.0 && *n >= min as f64 && *n <= 1e9 => Ok(*n as usize),
                Some(_) => Err(format!("assertion {i} ({kind}): `{k}` must be an integer >= {min}")),
                None => Err(format!("assertion {i} ({kind}): missing `{k}`")),
            }
        };
        let text_field = |k: &str| -> Result<String, String> {
            match get(k) {
                Some(Json::Str(s)) => Ok(s.clone()),
                Some(_) => Err(format!("assertion {i} ({kind}): `{k}` must be a string")),
                None => Err(format!("assertion {i} ({kind}): missing `{k}`")),
            }
        };
        let (k, allowed): (Kind, &[&str]) = match kind {
            "max-pages" => (Kind::MaxPages(int("value", 0)?), &["value"]),
            "exact-pages" => (Kind::ExactPages(int("value", 0)?), &["value"]),
            "no-overflow" => (Kind::NoOverflow, &[]),
            "no-empty-pages" => (Kind::NoEmptyPages, &[]),
            "keep-together" => (Kind::KeepTogether(text_field("selector")?), &["selector"]),
            "on-page" => (
                Kind::OnPage(text_field("selector")?, int("page", 1)?),
                &["selector", "page"],
            ),
            "outline-covers" => (Kind::OutlineCovers(text_field("selector")?), &["selector"]),
            "fonts-resolved" => (Kind::FontsResolved, &[]),
            other => {
                return Err(format!(
                    "assertion {i}: unknown kind `{other}` (expected max-pages, exact-pages, \
                     no-overflow, no-empty-pages, keep-together, on-page, outline-covers, fonts-resolved)"
                ))
            }
        };
        for (name, _) in fields {
            if name != "kind" && name != "id" && !allowed.contains(&name.as_str()) {
                return Err(format!("assertion {i} ({kind}): unknown field `{name}`"));
            }
        }
        out.push(Assertion { id, kind: k });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Every (page, rect) at which each sourced element has a box. Per element
/// the list holds the first box on each page, in page order.
fn boxes(layout: &Layout) -> BTreeMap<NodeId, Vec<(usize, [f64; 4])>> {
    fn walk(
        f: &Fragment,
        px: f64,
        py: f64,
        page: usize,
        map: &mut BTreeMap<NodeId, Vec<(usize, [f64; 4])>>,
    ) {
        let x = px + f.offset.x.get();
        let y = py + f.offset.y.get();
        if let Some(src) = f.source {
            let v = map.entry(src).or_default();
            if v.last().map(|b| b.0) != Some(page) {
                v.push((
                    page,
                    [round2(x), round2(y), round2(f.size.0.get()), round2(f.size.1.get())],
                ));
            }
        }
        for c in &f.children {
            walk(c, x, y, page, map);
        }
    }
    let mut map = BTreeMap::new();
    for (pi, page) in layout.pages.iter().enumerate() {
        walk(&page.root, 0.0, 0.0, pi + 1, &mut map);
    }
    map
}

fn element_evidence(dom: &Dom, id: NodeId, page: usize, rect: Option<[f64; 4]>) -> Evidence {
    let (line, column) = dom.source_pos(id);
    Evidence { page, rect, line, column, element: selector_of(dom, id) }
}

/// Evaluate every assertion, in input order (spec Behavior 5–9).
pub fn evaluate(
    dom: &Dom,
    stylesheet: &Stylesheet,
    layout: &Layout,
    report: &RenderReport,
    list: &[Assertion],
) -> Vec<Outcome> {
    let pages = layout.pages.len();
    let boxes = boxes(layout);
    let needs_styles = list.iter().any(|a| matches!(a.kind, Kind::OutlineCovers(_)));
    let styles = if needs_styles {
        crate::css::cascade(dom, stylesheet, &layout.geometry)
    } else {
        Vec::new()
    };
    let events = |code: &str| -> Vec<Evidence> {
        report
            .events
            .iter()
            .filter(|e| e.code == code)
            .map(|e| Evidence {
                page: e.page,
                rect: e.rect,
                line: e.line,
                column: e.column,
                element: e.element.clone(),
            })
            .collect()
    };

    list.iter()
        .enumerate()
        .map(|(index, a)| {
            let (passed, detail, evidence) = match &a.kind {
                Kind::MaxPages(n) => (pages <= *n, format!("{pages} pages (max {n})"), Vec::new()),
                Kind::ExactPages(n) => {
                    (pages == *n, format!("{pages} pages (expected {n})"), Vec::new())
                }
                Kind::NoOverflow => event_verdict(events("layout-overflow"), "overflowing box"),
                Kind::NoEmptyPages => event_verdict(events("empty-page"), "empty page"),
                Kind::FontsResolved => event_verdict(events("font-fallback"), "font fallback"),
                Kind::KeepTogether(sel) => match matched(dom, sel) {
                    Err(d) => (false, d, Vec::new()),
                    Ok(ids) => {
                        let mut ev = Vec::new();
                        let mut split = 0;
                        for &id in &ids {
                            let b = boxes.get(&id).map(Vec::as_slice).unwrap_or(&[]);
                            if b.len() > 1 {
                                split += 1;
                                for (p, r) in b {
                                    ev.push(element_evidence(dom, id, *p, Some(*r)));
                                }
                            }
                        }
                        let n = ids.len();
                        if split == 0 {
                            (true, format!("{n} matched elements each on one page"), Vec::new())
                        } else {
                            (false, format!("{split} of {n} matched elements split across pages"), ev)
                        }
                    }
                },
                Kind::OnPage(sel, want) => match matched(dom, sel) {
                    Err(d) => (false, d, Vec::new()),
                    Ok(ids) => {
                        let mut ev = Vec::new();
                        let mut nobox = 0;
                        for &id in &ids {
                            match boxes.get(&id).and_then(|b| b.first()) {
                                Some((p, _)) if p == want => {}
                                Some((p, r)) => ev.push(element_evidence(dom, id, *p, Some(*r))),
                                None => {
                                    nobox += 1;
                                    ev.push(element_evidence(dom, id, 0, None));
                                }
                            }
                        }
                        let n = ids.len();
                        if ev.is_empty() {
                            (true, format!("{n} matched elements start on page {want}"), Vec::new())
                        } else {
                            let mut d = format!(
                                "{} of {n} matched elements do not start on page {want}",
                                ev.len()
                            );
                            if nobox > 0 {
                                let _ = write!(d, " ({nobox} with no box)");
                            }
                            (false, d, ev)
                        }
                    }
                },
                Kind::OutlineCovers(sel) => match matched(dom, sel) {
                    Err(d) => (false, d, Vec::new()),
                    Ok(ids) => {
                        let mut ev = Vec::new();
                        for &id in &ids {
                            let first = boxes.get(&id).and_then(|b| b.first()).copied();
                            let level = styles.get(id).map(|s| s.bookmark_level);
                            let covered =
                                first.is_some() && !matches!(level, Some(BookmarkLevel::None) | None);
                            if !covered {
                                ev.push(element_evidence(
                                    dom,
                                    id,
                                    first.map(|b| b.0).unwrap_or(0),
                                    first.map(|b| b.1),
                                ));
                            }
                        }
                        let n = ids.len();
                        if ev.is_empty() {
                            (true, format!("{n} matched elements in the outline"), Vec::new())
                        } else {
                            (false, format!("{} of {n} matched elements missing from the outline", ev.len()), ev)
                        }
                    }
                },
            };
            Outcome { index, id: a.id.clone(), kind: a.kind.name(), passed, detail, evidence }
        })
        .collect()
}

fn event_verdict(ev: Vec<Evidence>, what: &str) -> (bool, String, Vec<Evidence>) {
    if ev.is_empty() {
        (true, format!("no {what}"), ev)
    } else {
        let n = ev.len();
        let s = if n == 1 { "" } else { "s" };
        (false, format!("{n} {what}{s}"), ev)
    }
}

fn matched(dom: &Dom, sel: &str) -> Result<Vec<NodeId>, String> {
    match crate::css::select(dom, sel) {
        None => Err(format!("invalid selector `{sel}`")),
        Some(ids) if ids.is_empty() => Err(format!("selector `{sel}` matches no element")),
        Some(ids) => Ok(ids),
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

fn fmt_num(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

fn evidence_json(e: &Evidence) -> String {
    let rect = match e.rect {
        Some(r) => format!("[{}, {}, {}, {}]", fmt_num(r[0]), fmt_num(r[1]), fmt_num(r[2]), fmt_num(r[3])),
        None => "null".to_string(),
    };
    format!(
        "{{\"page\": {}, \"rect\": {rect}, \"line\": {}, \"column\": {}, \"element\": {}}}",
        e.page,
        e.line,
        e.column,
        json_string(&e.element)
    )
}

/// The `"assertions": {...}` member (spec Behavior 10), indented for the
/// schema-2 document, WITHOUT a trailing comma or newline.
pub fn to_json_block(outcomes: &[Outcome]) -> String {
    let passed = outcomes.iter().filter(|o| o.passed).count();
    let mut s = format!(
        "  \"assertions\": {{\"passed\": {passed}, \"failed\": {}, \"results\": [",
        outcomes.len() - passed
    );
    if outcomes.is_empty() {
        s.push_str("]}");
        return s;
    }
    s.push('\n');
    for (i, o) in outcomes.iter().enumerate() {
        let _ = write!(s, "    {{\"index\": {}", o.index);
        if let Some(id) = &o.id {
            let _ = write!(s, ", \"id\": {}", json_string(id));
        }
        let _ = write!(
            s,
            ", \"kind\": {}, \"passed\": {}, \"detail\": {}, \"evidence\": [",
            json_string(o.kind),
            o.passed,
            json_string(&o.detail)
        );
        let ev: Vec<String> = o.evidence.iter().map(evidence_json).collect();
        s.push_str(&ev.join(", "));
        s.push_str("]}");
        if i + 1 < outcomes.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("  ]}");
    s
}

/// Text-mode line (spec Behavior 12).
pub fn to_text(o: &Outcome) -> String {
    format!(
        "assert[{}] {} {}: {}",
        o.index,
        if o.passed { "PASS" } else { "FAIL" },
        o.kind,
        o.detail
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_kind() {
        let a = parse(
            r#"[{"kind":"max-pages","value":2,"id":"fit"},{"kind":"exact-pages","value":1},
               {"kind":"no-overflow"},{"kind":"no-empty-pages"},{"kind":"keep-together","selector":"table"},
               {"kind":"on-page","selector":"h1","page":1},{"kind":"outline-covers","selector":"h1, h2"},
               {"kind":"fonts-resolved"}]"#,
        )
        .unwrap();
        assert_eq!(a.len(), 8);
        assert_eq!(a[0].id.as_deref(), Some("fit"));
        assert_eq!(a[5].kind, Kind::OnPage("h1".into(), 1));
    }

    #[test]
    fn rejects_bad_input() {
        for bad in [
            "{}",
            "[1]",
            "[{\"kind\":\"nope\"}]",
            "[{\"kind\":\"max-pages\"}]",
            "[{\"kind\":\"max-pages\",\"value\":1.5}]",
            "[{\"kind\":\"max-pages\",\"value\":-1}]",
            "[{\"kind\":\"on-page\",\"selector\":\"p\",\"page\":0}]",
            "[{\"kind\":\"no-overflow\",\"extra\":1}]",
            "[{\"kind\":\"no-overflow\"}",
            "[{\"kind\":\"no-overflow\"}] x",
            "[{\"kind\":\"no-overflow\",\"kind\":\"no-overflow\"}]",
        ] {
            assert!(parse(bad).is_err(), "accepted {bad}");
        }
        assert_eq!(parse(" [ ] ").unwrap(), Vec::new());
    }

    #[test]
    fn string_escapes() {
        let a = parse(r#"[{"kind":"keep-together","selector":"p\u002ex \"q\" é"}]"#).unwrap();
        assert_eq!(a[0].kind, Kind::KeepTogether("p.x \"q\" é".into()));
    }
}
