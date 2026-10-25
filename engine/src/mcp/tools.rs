// SPDX-License-Identifier: AGPL-3.0-only

//! The five contract tools. Each takes the tool's input object and returns
//! its output object or a [`ToolError`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use base64::Engine as _;
use serde_json::{json, Map, Value};

use super::input::{self, Access, Scratch, ToolError, ToolResult};
use super::raster;
use crate::css::ComputedStyle;
use crate::dom::{Dom, NodeId, NodeKind};
use crate::fonts::FaceId;
use crate::frag::{Fragment, FragmentContent, FragmentKind};
use crate::geom::{PageGeometry, Scalar};
use crate::pipeline::{self, RenderInput, Rendered};

/// Default page box when `options` gives none: US Letter, zero margins
/// (the CLI's margin defaults). mcp-server spec S6.
const DEFAULT_PAGE_WIDTH: &str = "8.5in";
const DEFAULT_PAGE_HEIGHT: &str = "11in";

/// The CSS support guide `capabilities.css.support_guide` links to.
const SUPPORT_GUIDE: &str = "https://typeanvil.dev/guides/css-support";

pub struct Ctx<'a> {
    pub roots: &'a [PathBuf],
}

pub fn dispatch(ctx: &Ctx, tool: &str, args: Value) -> ToolResult<Value> {
    // Contract order: the source rule first, then the schema.
    input::check_sources(&args)?;
    input::check_schema(tool, &args)?;
    match tool {
        "render" => render(ctx, &args),
        "validate" => validate(ctx, &args),
        "inspect_page" => inspect_page(ctx, &args),
        "capabilities" => Ok(capabilities(&args)),
        "explain" => explain(ctx, &args),
        other => Err(ToolError::new("INVALID_INPUT", &format!("unknown tool `{other}`"))),
    }
}

// --- shared render step -----------------------------------------------------

/// A finished render plus its diagnostics.
struct Prepared {
    done: Rendered,
    diagnostics: Vec<Value>,
    /// Layout-overflow events (for the `no_overflow` assertion): pages.
    overflow_pages: Vec<usize>,
    styles: Vec<ComputedStyle>,
}

fn opt_str<'a>(o: &'a Value, k: &str) -> Option<&'a str> {
    o.get(k).and_then(Value::as_str)
}

fn length(o: &Value, k: &str, default: &str) -> ToolResult<Scalar> {
    let raw = opt_str(o, k).unwrap_or(default);
    pipeline::parse_length(raw)
        .map_err(|e| ToolError::at("INVALID_INPUT", &format!("/options/{k}"), &format!("{e:#}")))
}

/// Load inputs, materialize assets, run the shared pipeline.
fn prepare(ctx: &Ctx, args: &Value) -> ToolResult<Prepared> {
    let access = Access { roots: ctx.roots };
    let scratch = Scratch::new()?;
    // Drop anchors a previous call may have left behind (global collector).
    let _ = crate::diagnostics::take_broken_anchors();

    let doc_src = &args["document"];
    let doc = access.load(doc_src, "/document")?;
    let mut extra_css = Vec::new();
    if let Some(Value::Array(sheets)) = args.get("stylesheets") {
        for (i, s) in sheets.iter().enumerate() {
            extra_css.push(access.load(s, &format!("/stylesheets/{i}"))?.text);
        }
    }

    // The base directory for relative references.
    let base = match opt_str(doc_src, "base_url") {
        Some(b) => match input::base_dir(b) {
            Some(d) => {
                let d = std::fs::canonicalize(&d).unwrap_or(d);
                access.check_root(&d, "/document/base_url")?;
                Some(d)
            }
            None => None,
        },
        None => doc.file.as_ref().and_then(|f| f.parent().map(PathBuf::from)),
    };
    let base_dir = base.clone().unwrap_or_else(|| scratch.dir.clone());
    let nominal = match &doc.file {
        Some(f) if opt_str(doc_src, "base_url").is_none() => f.clone(),
        _ => base_dir.join(if doc.markdown { "document.md" } else { "document.html" }),
    };

    // Assets: write each one into the scratch dir; references that equal a
    // key point at the file (an asset wins over file or network lookup).
    let mut asset_map: Vec<(String, String)> = Vec::new();
    if let Some(Value::Object(assets)) = args.get("assets") {
        for (i, (key, src)) in assets.iter().enumerate() {
            let ptr = format!("/assets/{}", super::schema::escape(key));
            let bytes = access.asset_bytes(src, &ptr)?;
            let file = scratch.dir.join(format!("asset-{i}{}", asset_ext(key, src)));
            std::fs::write(&file, &bytes)
                .map_err(|e| ToolError::new("INTERNAL", &format!("cannot write asset: {e}")))?;
            asset_map.push((key.clone(), file.to_string_lossy().into_owned()));
        }
    }
    for css in extra_css.iter_mut() {
        *css = rewrite_css_urls(css, &asset_map);
    }

    let opts = args.get("options").cloned().unwrap_or_else(|| json!({}));
    let geometry = PageGeometry {
        width: length(&opts, "page_width", DEFAULT_PAGE_WIDTH)?,
        height: length(&opts, "page_height", DEFAULT_PAGE_HEIGHT)?,
        margin_top: length(&opts, "margin_top", "0")?,
        margin_right: length(&opts, "margin_right", "0")?,
        margin_bottom: length(&opts, "margin_bottom", "0")?,
        margin_left: length(&opts, "margin_left", "0")?,
    };
    if geometry.width.get() <= 0.0 || geometry.height.get() <= 0.0 {
        return Err(ToolError::at("INVALID_INPUT", "/options", "page_width and page_height must be positive"));
    }
    let tagged = opts.get("tagged").and_then(Value::as_bool).unwrap_or(false);

    let rewrite: Option<Box<dyn Fn(&mut Dom)>> = if asset_map.is_empty() {
        None
    } else {
        let map = asset_map.clone();
        Some(Box::new(move |dom: &mut Dom| rewrite_dom_refs(dom, &map)))
    };

    let done = pipeline::run(RenderInput {
        source: doc.text,
        input: nominal,
        markdown: doc.markdown,
        extra_css,
        default_css: true,
        geometry,
        base_url: Some(base_dir.clone()),
        title: opt_str(&opts, "title").map(str::to_string),
        author: opt_str(&opts, "author").map(str::to_string),
        tagged,
        ua: false,
        rewrite,
        // CORE-578: MCP input is agent-written, so the server runs the
        // strict local profile: document references read only under the
        // document's base directory, the call's scratch (materialized
        // assets) and the configured roots; remote references are blocked.
        policy: {
            let mut p = crate::safety::Policy {
                fetch: crate::safety::FetchPolicy::Local,
                ..crate::safety::Policy::unrestricted()
            }
            .with_root(&base_dir)
            .with_root(&scratch.dir);
            for r in ctx.roots {
                p = p.with_root(r);
            }
            p
        },
        max_input_bytes: None,
    })
    .map_err(|e| {
        if let Some(le) = e.downcast_ref::<crate::safety::LimitError>() {
            return ToolError::new(le.kind.code(), &le.message);
        }
        let text = format!("{e:#}");
        if text.starts_with("parsing HTML") || text.starts_with("converting Markdown") {
            ToolError::at("PARSE_FAILED", "/document", &text)
        } else {
            ToolError::new("RENDER_FAILED", &text)
        }
    })?;

    // Diagnostics: Markdown, stylesheet, bookmark anchors, then layout.
    let mut diagnostics = Vec::new();
    let mut push = |code: &str, severity: &str, message: &str, line: usize, column: usize| {
        let mut d = Map::new();
        d.insert("code".into(), json!(code));
        d.insert("severity".into(), json!(severity));
        d.insert("message".into(), json!(message));
        if line > 0 {
            d.insert("line".into(), json!(line));
        }
        if column > 0 {
            d.insert("column".into(), json!(column));
        }
        diagnostics.push(Value::Object(d));
    };
    for d in done
        .md_events
        .iter()
        .chain(crate::diagnostics::analyze(done.stylesheet.source()).iter())
    {
        push(&d.code, &d.severity, &d.message, d.line, d.column);
    }
    for msg in &done.blocked {
        push("fetch-blocked", "warning", msg, 0, 0);
    }
    for msg in crate::diagnostics::take_broken_anchors() {
        push("bookmark-anchor-unresolved", "warning", &msg, 0, 0);
    }
    let report = crate::render_report::analyze_layout(&done.dom, &done.stylesheet, &done.layout);
    let mut overflow_pages = Vec::new();
    for e in &report.events {
        if e.code == "layout-overflow" {
            overflow_pages.push(e.page);
        }
        let message = match (e.page, e.element.is_empty()) {
            (0, true) => e.message.clone(),
            (0, false) => format!("{} ({})", e.message, e.element),
            (p, true) => format!("{} (page {p})", e.message),
            (p, false) => format!("{} ({}, page {p})", e.message, e.element),
        };
        push(e.code, "warning", &message, e.line as usize, e.column as usize);
    }
    let styles = crate::css::cascade(&done.dom, &done.stylesheet, &done.layout.geometry);
    drop(scratch);
    Ok(Prepared { done, diagnostics, overflow_pages, styles })
}

/// File extension for a materialized asset: from the key, else from the
/// media type.
fn asset_ext(key: &str, src: &Value) -> String {
    let path = key.split(['?', '#']).next().unwrap_or(key);
    if let Some(ext) = std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
        if ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return format!(".{ext}");
        }
    }
    match opt_str(src, "media_type") {
        Some("text/css") => ".css".into(),
        Some("image/png") => ".png".into(),
        Some("image/jpeg") => ".jpg".into(),
        Some("image/gif") => ".gif".into(),
        Some("image/svg+xml") => ".svg".into(),
        Some("font/ttf") => ".ttf".into(),
        Some("font/otf") => ".otf".into(),
        Some("font/woff2") => ".woff2".into(),
        _ => String::new(),
    }
}

/// Point `src`/`href` attributes and `<style>` `url()` references that equal
/// an asset key at the asset's file.
fn rewrite_dom_refs(dom: &mut Dom, map: &[(String, String)]) {
    let mut style_texts: Vec<NodeId> = Vec::new();
    for id in 0..dom.nodes.len() {
        if let NodeKind::Element(el) = &dom.nodes[id].kind {
            if el.tag == "style" {
                style_texts.extend(dom.nodes[id].children.iter().copied());
            }
        }
    }
    for node in dom.nodes.iter_mut() {
        if let NodeKind::Element(el) = &mut node.kind {
            for (name, value) in el.attrs.iter_mut() {
                if name == "src" || name == "href" {
                    if let Some((_, file)) = map.iter().find(|(k, _)| k == value.trim()) {
                        *value = file.clone();
                    }
                }
            }
        }
    }
    for id in style_texts {
        if let NodeKind::Text(t) = &mut dom.nodes[id].kind {
            *t = rewrite_css_urls(t, map);
        }
    }
}

/// Replace `url(key)`, `url('key')` and `url("key")` with the asset file.
fn rewrite_css_urls(css: &str, map: &[(String, String)]) -> String {
    let mut out = css.to_string();
    for (key, file) in map {
        for (open, close) in [("url(", ")"), ("url('", "')"), ("url(\"", "\")")] {
            out = out.replace(&format!("{open}{key}{close}"), &format!("url('{file}')"));
        }
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    crate::images::sha256(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

// --- render ------------------------------------------------------------------

fn render(ctx: &Ctx, args: &Value) -> ToolResult<Value> {
    let output = args.get("output").cloned().unwrap_or_else(|| json!({}));
    let mode = opt_str(&output, "mode").unwrap_or("auto");
    let access = Access { roots: ctx.roots };
    // Check the output form before the (possibly long) render.
    let out_path = match mode {
        "path" => Some(access.output_path(opt_str(&output, "path").unwrap_or_default())?),
        "bytes" | "auto" => None,
        _ => {
            return Err(ToolError::at(
                "OUTPUT_FORM_UNSUPPORTED",
                "/output/mode",
                "the local server returns `bytes` or writes a `path`; `link` is a cloud form",
            ))
        }
    };
    let p = prepare(ctx, args)?;
    let pdf = &p.done.pdf;
    let artifact = match out_path {
        Some(path) => {
            std::fs::write(&path, pdf).map_err(|e| {
                ToolError::at("PATH_NOT_FOUND", "/output/path", &format!("cannot write {}: {e}", path.display()))
            })?;
            json!({ "kind": "path", "path": path.to_string_lossy(), "media_type": "application/pdf" })
        }
        None => json!({
            "kind": "bytes",
            "data_base64": base64::engine::general_purpose::STANDARD.encode(pdf),
            "media_type": "application/pdf",
        }),
    };
    Ok(json!({
        "environment": super::environment(),
        "page_count": p.done.layout.pages.len(),
        "byte_size": pdf.len(),
        "sha256": sha256_hex(pdf),
        "artifact": artifact,
        "diagnostics": p.diagnostics,
    }))
}

// --- validate ----------------------------------------------------------------

fn validate(ctx: &Ctx, args: &Value) -> ToolResult<Value> {
    let p = prepare(ctx, args)?;
    let pages = p.done.layout.pages.len();
    let fonts = fonts_used(&p.done);
    let page_texts: Vec<String> = (0..pages).map(|i| page_text(&p.done, i)).collect();

    let mut results = Vec::new();
    let mut all_passed = true;
    if let Some(Value::Array(list)) = args.get("assertions") {
        for (i, a) in list.iter().enumerate() {
            let kind = opt_str(a, "kind").unwrap_or_default();
            let page = a.get("page").and_then(Value::as_u64).map(|n| n as usize);
            let ptr = |f: &str| format!("/assertions/{i}/{f}");
            let need_text = || {
                opt_str(a, "text").ok_or_else(|| {
                    ToolError::at("INVALID_INPUT", &ptr("text"), &format!("`{kind}` needs `text`"))
                })
            };
            let (passed, detail) = match kind {
                "page_count" => {
                    let eq = a.get("equals").and_then(Value::as_i64);
                    let min = a.get("min").and_then(Value::as_i64);
                    let max = a.get("max").and_then(Value::as_i64);
                    if eq.is_none() && min.is_none() && max.is_none() {
                        return Err(ToolError::at(
                            "INVALID_INPUT",
                            &format!("/assertions/{i}"),
                            "`page_count` needs `equals`, `min` or `max`",
                        ));
                    }
                    let n = pages as i64;
                    let ok = eq.is_none_or(|e| n == e) && min.is_none_or(|m| n >= m) && max.is_none_or(|m| n <= m);
                    (ok, format!("the document has {n} page(s)"))
                }
                "no_overflow" => {
                    let hits: Vec<usize> = p
                        .overflow_pages
                        .iter()
                        .copied()
                        .filter(|pg| page.is_none_or(|want| *pg == want))
                        .collect();
                    if hits.is_empty() {
                        (true, "no box overflows its page".to_string())
                    } else {
                        (false, format!("{} box(es) overflow, on page(s) {}", hits.len(), join_pages(&hits)))
                    }
                }
                "no_diagnostics" => {
                    let n = p.diagnostics.len();
                    (n == 0, format!("{n} diagnostic(s)"))
                }
                "text_on_page" => {
                    let want = normalize_ws(need_text()?);
                    let found: Vec<usize> = page_texts
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| t.contains(&want))
                        .map(|(i, _)| i + 1)
                        .collect();
                    let ok = match page {
                        Some(pg) => found.contains(&pg),
                        None => !found.is_empty(),
                    };
                    let detail = if found.is_empty() {
                        "the text is on no page".to_string()
                    } else {
                        format!("the text is on page(s) {}", join_pages(&found))
                    };
                    (ok, detail)
                }
                "element_on_page" => {
                    let sel = opt_str(a, "selector").ok_or_else(|| {
                        ToolError::at("INVALID_INPUT", &ptr("selector"), "`element_on_page` needs `selector`")
                    })?;
                    let ids = crate::css::select_elements(&p.done.dom, sel).ok_or_else(|| {
                        ToolError::at("SELECTOR_INVALID", &ptr("selector"), &format!("`{sel}` is not a valid selector"))
                    })?;
                    let mut on: BTreeSet<usize> = BTreeSet::new();
                    for id in &ids {
                        for (pg, _) in placements(&p.done, *id) {
                            on.insert(pg);
                        }
                    }
                    let on: Vec<usize> = on.into_iter().collect();
                    let ok = match page {
                        Some(pg) => on.contains(&pg),
                        None => !on.is_empty(),
                    };
                    let detail = if ids.is_empty() {
                        "no element matches the selector".to_string()
                    } else if on.is_empty() {
                        format!("{} element(s) match but generate no box", ids.len())
                    } else {
                        format!("matching elements are on page(s) {}", join_pages(&on))
                    };
                    (ok, detail)
                }
                _ /* fonts_embedded */ => {
                    let missing: Vec<&str> = fonts
                        .iter()
                        .filter(|f| !f["embedded"].as_bool().unwrap_or(false))
                        .filter_map(|f| f["face"].as_str())
                        .collect();
                    if missing.is_empty() {
                        (true, format!("all {} font face(s) are embedded", fonts.len()))
                    } else {
                        (false, format!("not embedded: {}", missing.join(", ")))
                    }
                }
            };
            all_passed &= passed;
            let mut r = Map::new();
            r.insert("index".into(), json!(i));
            if let Some(id) = opt_str(a, "id") {
                r.insert("id".into(), json!(id));
            }
            r.insert("kind".into(), json!(kind));
            r.insert("passed".into(), json!(passed));
            r.insert("detail".into(), json!(detail));
            results.push(Value::Object(r));
        }
    }
    let has_error = p.diagnostics.iter().any(|d| d["severity"] == "error");
    Ok(json!({
        "environment": super::environment(),
        "ok": all_passed && !has_error,
        "page_count": pages,
        "diagnostics": p.diagnostics,
        "assertions": results,
        "fonts": fonts,
    }))
}

fn join_pages(pages: &[usize]) -> String {
    let mut v: Vec<usize> = pages.to_vec();
    v.dedup();
    v.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// All text painted on page `index` (body and margin boxes), whitespace
/// normalized, lines joined by one space.
fn page_text(done: &Rendered, index: usize) -> String {
    fn walk(f: &Fragment, out: &mut Vec<String>) {
        if let FragmentContent::Text(run) = &f.content {
            out.push(run.text.clone());
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    let page = &done.layout.pages[index];
    let mut parts = Vec::new();
    walk(&page.root, &mut parts);
    for mb in &page.margin_boxes {
        walk(&mb.fragment, &mut parts);
    }
    normalize_ws(&parts.join(" "))
}

/// Every face that painted text, in face-id order.
fn fonts_used(done: &Rendered) -> Vec<Value> {
    fn walk(f: &Fragment, out: &mut BTreeSet<u32>) {
        if let FragmentContent::Text(run) = &f.content {
            if !run.glyphs.is_empty() {
                out.insert(run.font_face.0);
            }
            for g in &run.glyphs {
                if let Some(face) = g.face {
                    out.insert(face.0);
                }
            }
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    let mut ids = BTreeSet::new();
    for page in &done.layout.pages {
        walk(&page.root, &mut ids);
        for mb in &page.margin_boxes {
            walk(&mb.fragment, &mut ids);
        }
    }
    ids.into_iter()
        .map(|id| {
            let face = FaceId(id);
            let source = if id < 4 {
                "bundle"
            } else if crate::fonts::is_font_face_bytes(face) {
                "document"
            } else {
                "system"
            };
            json!({
                "family": family_name(face),
                "face": crate::fonts::face_postscript_name(face),
                // krilla embeds (subsets) every face the PDF draws with.
                "embedded": true,
                "source": source,
            })
        })
        .collect()
}

/// A face's family name (name table id 16, else id 1), else its PostScript
/// name.
fn family_name(face: FaceId) -> String {
    use read_fonts::TableProvider as _;
    let bytes = crate::fonts::face_bytes(face);
    let index = crate::fonts::face_index(face);
    let from_table = (|| {
        let font = read_fonts::FontRef::from_index(bytes, index).ok()?;
        let name = font.name().ok()?;
        let data = name.string_data();
        let mut best: Option<(u8, String)> = None;
        for record in name.name_record().iter() {
            let rank = match record.name_id() {
                read_fonts::tables::name::NameId::TYPOGRAPHIC_FAMILY_NAME => 0,
                read_fonts::tables::name::NameId::FAMILY_NAME => 1,
                _ => continue,
            };
            if let Ok(s) = record.string(data) {
                let s = s.to_string();
                if !s.is_empty() && best.as_ref().is_none_or(|(r, _)| rank < *r) {
                    best = Some((rank, s));
                }
            }
        }
        best.map(|(_, s)| s)
    })();
    from_table.unwrap_or_else(|| crate::fonts::face_postscript_name(face))
}

// --- inspect_page ------------------------------------------------------------

fn inspect_page(ctx: &Ctx, args: &Value) -> ToolResult<Value> {
    let p = prepare(ctx, args)?;
    let count = p.done.layout.pages.len();
    let page_no = args["page"].as_u64().unwrap_or(1) as usize;
    if page_no == 0 || page_no > count {
        return Err(ToolError::at(
            "PAGE_OUT_OF_RANGE",
            "/page",
            &format!("page {page_no} is out of range; the document has {count} page(s)"),
        ));
    }
    let max_boxes = args.get("max_boxes").and_then(Value::as_u64).unwrap_or(500) as usize;
    let index = page_no - 1;
    let page = &p.done.layout.pages[index];
    let dom = &p.done.dom;

    let prev = page_sources(&p.done, index.checked_sub(1));
    let next = page_sources(&p.done, Some(index + 1));

    let mut flat: Vec<(Option<&'static str>, &Fragment, f64, f64)> = Vec::new();
    flatten(&page.root, 0.0, 0.0, &mut flat, true);
    for mb in page.margin_boxes_in_paint_order() {
        let start = flat.len();
        flatten(&mb.fragment, 0.0, 0.0, &mut flat, false);
        if let Some(first) = flat.get_mut(start) {
            first.0 = Some("margin-box");
        }
    }

    let truncated = flat.len() > max_boxes;
    let mut boxes = Vec::new();
    for (n, (forced, f, x, y)) in flat.iter().take(max_boxes).enumerate() {
        let mut b = Map::new();
        b.insert("id".into(), json!(format!("p{page_no}-b{n}")));
        b.insert("kind".into(), json!(forced.unwrap_or_else(|| box_kind(f, &p.styles))));
        if let Some(src) = f.source.filter(|s| *s < dom.nodes.len()) {
            if dom.nodes[src].kind.element().is_some() {
                b.insert("element".into(), json!(crate::render_report::selector_of(dom, src)));
                b.insert("dom_path".into(), json!(dom_path(dom, src)));
                b.insert("continued_from_previous_page".into(), json!(prev.contains(&src)));
                b.insert("continues_on_next_page".into(), json!(next.contains(&src)));
            }
        }
        b.insert("rect".into(), rect(*x, *y, f.size.0.get(), f.size.1.get()));
        if let FragmentContent::Text(run) = &f.content {
            b.insert("text".into(), json!(run.text));
        }
        boxes.push(Value::Object(b));
    }

    let mut out = Map::new();
    out.insert("environment".into(), super::environment());
    out.insert("page".into(), json!(page_no));
    out.insert("page_count".into(), json!(count));
    out.insert(
        "page_size".into(),
        json!({ "width": round2(page.root.size.0.get()), "height": round2(page.root.size.1.get()) }),
    );
    out.insert("boxes".into(), Value::Array(boxes));
    out.insert("truncated".into(), json!(truncated));
    let image = args.get("image").cloned().unwrap_or_else(|| json!({}));
    if image.get("include").and_then(Value::as_bool).unwrap_or(true) {
        let dpi = image.get("dpi").and_then(Value::as_u64).unwrap_or(96) as u32;
        let (png, w, h) = raster::page_png(&p.done.pdf, index, dpi)
            .map_err(|e| ToolError::new("RENDER_FAILED", &format!("rasterizing page {page_no}: {e}")))?;
        out.insert(
            "image".into(),
            json!({
                "media_type": "image/png",
                "data_base64": base64::engine::general_purpose::STANDARD.encode(png),
                "width_px": w,
                "height_px": h,
                "dpi": dpi,
            }),
        );
    }
    Ok(Value::Object(out))
}

/// Pre-order flatten with page-absolute top-left coordinates (the
/// render-report convention). The page root fragment itself is skipped.
fn flatten<'a>(
    f: &'a Fragment,
    px: f64,
    py: f64,
    out: &mut Vec<(Option<&'static str>, &'a Fragment, f64, f64)>,
    skip_self: bool,
) {
    let x = px + f.offset.x.get();
    let y = py + f.offset.y.get();
    if !skip_self {
        out.push((None, f, x, y));
    }
    for c in &f.children {
        flatten(c, x, y, out, false);
    }
}

fn box_kind(f: &Fragment, styles: &[ComputedStyle]) -> &'static str {
    match &f.content {
        FragmentContent::Text(_) => return "text",
        FragmentContent::Image(_) => return "image",
        FragmentContent::Background(_) | FragmentContent::BackgroundImage(_) => return "background",
        FragmentContent::Border(_) => return "border",
        FragmentContent::None => {}
    }
    match f.kind {
        FragmentKind::Line => "line",
        FragmentKind::Fragmentainer => "other",
        FragmentKind::Block => match f.source.and_then(|s| styles.get(s)).map(|s| s.display) {
            Some(crate::css::Display::Table) => "table",
            Some(crate::css::Display::TableCell) => "table-cell",
            _ => "block",
        },
    }
}

fn round2(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 { 0.0 } else { r }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Value {
    json!({ "x": round2(x), "y": round2(y), "width": round2(w), "height": round2(h) })
}

/// Source nodes with a box on page `index` (empty when out of range).
fn page_sources(done: &Rendered, index: Option<usize>) -> BTreeSet<NodeId> {
    fn walk(f: &Fragment, out: &mut BTreeSet<NodeId>) {
        if let Some(s) = f.source {
            out.insert(s);
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    let mut out = BTreeSet::new();
    if let Some(page) = index.and_then(|i| done.layout.pages.get(i)) {
        walk(&page.root, &mut out);
    }
    out
}

/// `html > body:nth-child(2) > p:nth-child(3)` — element steps from the root.
fn dom_path(dom: &Dom, id: NodeId) -> String {
    let mut steps = Vec::new();
    let mut cur = Some(id);
    while let Some(n) = cur {
        let Some(el) = dom.nodes[n].kind.element() else { break };
        let parent = dom.nodes[n].parent;
        let step = match parent {
            Some(p) if dom.nodes[p].kind.element().is_some() => {
                let pos = dom.nodes[p]
                    .children
                    .iter()
                    .filter(|c| dom.nodes[**c].kind.element().is_some())
                    .position(|c| *c == n)
                    .map(|i| i + 1)
                    .unwrap_or(1);
                format!("{}:nth-child({pos})", el.tag)
            }
            _ => el.tag.clone(),
        };
        steps.push(step);
        cur = parent;
    }
    steps.reverse();
    steps.join(" > ")
}

// --- capabilities ------------------------------------------------------------

fn capabilities(args: &Value) -> Value {
    let section = opt_str(args, "section").unwrap_or("all");
    let mut out = Map::new();
    out.insert("environment".into(), super::environment());
    out.insert("tools".into(), json!(super::TOOL_NAMES));
    out.insert("input_forms".into(), json!({ "path": true, "content": true, "url": true }));
    out.insert("output_modes".into(), json!(["path", "bytes"]));
    out.insert(
        "limits".into(),
        json!({
            "timeout_seconds": null,
            "max_input_bytes": null,
            "max_output_bytes": null,
            "max_pages": null,
            "max_assets": null,
            "quota": null,
        }),
    );
    if matches!(section, "all" | "inputs") {
        out.insert("input_languages".into(), json!(["html", "markdown"]));
    }
    if matches!(section, "all" | "fonts") {
        let mut bundle: Vec<String> = Vec::new();
        for id in 0..4 {
            let f = family_name(FaceId(id));
            if !bundle.contains(&f) {
                bundle.push(f);
            }
        }
        out.insert("fonts".into(), json!({ "bundle": bundle, "system_fonts": true }));
    }
    if matches!(section, "all" | "css") {
        out.insert("css".into(), json!({ "support_guide": SUPPORT_GUIDE }));
    }
    Value::Object(out)
}

// --- explain -----------------------------------------------------------------

fn explain(ctx: &Ctx, args: &Value) -> ToolResult<Value> {
    let selector = opt_str(args, "selector").unwrap_or_default().to_string();
    // A selector that cannot parse fails before the render.
    if crate::css::select_elements(&Dom::parse("").unwrap_or_default(), &selector).is_none() {
        return Err(ToolError::at("SELECTOR_INVALID", "/selector", &format!("`{selector}` is not a valid selector")));
    }
    let p = prepare(ctx, args)?;
    let ids = crate::css::select_elements(&p.done.dom, &selector).unwrap_or_default();
    let max = args.get("max_matches").and_then(Value::as_u64).unwrap_or(10) as usize;
    let wanted: Option<Vec<String>> = args.get("properties").and_then(Value::as_array).map(|a| {
        a.iter().filter_map(Value::as_str).map(|s| s.trim().to_ascii_lowercase()).collect()
    });
    let initial = style_table(&p.styles[p.done.dom.root]);
    let pages = p.done.layout.pages.len();

    let mut matches = Vec::new();
    for &id in ids.iter().take(max) {
        let table = style_table(&p.styles[id]);
        let mut computed = Map::new();
        for (name, value) in &table {
            let keep = match &wanted {
                Some(w) => w.iter().any(|x| x == name),
                None => initial.get(name) != Some(value),
            };
            if keep {
                computed.insert(name.clone(), json!(value));
            }
        }
        let places = placements(&p.done, id);
        let on: BTreeSet<usize> = places.iter().map(|(pg, _)| *pg).collect();
        let placements: Vec<Value> = places
            .iter()
            .map(|(pg, r)| {
                json!({
                    "page": pg,
                    "rect": rect(r[0], r[1], r[2], r[3]),
                    "continued_from_previous_page": *pg > 1 && on.contains(&(pg - 1)),
                    "continues_on_next_page": *pg < pages && on.contains(&(pg + 1)),
                })
            })
            .collect();
        matches.push(json!({
            "element": crate::render_report::selector_of(&p.done.dom, id),
            "dom_path": dom_path(&p.done.dom, id),
            "computed_style": computed,
            "placements": placements,
        }));
    }
    Ok(json!({
        "environment": super::environment(),
        "selector": selector,
        "match_count": ids.len(),
        "matches": matches,
    }))
}

/// One `(page, [x, y, w, h])` per page the element has a box on: the union
/// of the element's own fragments on that page (page-absolute points).
fn placements(done: &Rendered, id: NodeId) -> Vec<(usize, [f64; 4])> {
    fn walk(f: &Fragment, px: f64, py: f64, id: NodeId, acc: &mut Option<[f64; 4]>) {
        let x = px + f.offset.x.get();
        let y = py + f.offset.y.get();
        if f.source == Some(id) {
            let (x1, y1) = (x + f.size.0.get(), y + f.size.1.get());
            *acc = Some(match *acc {
                None => [x, y, x1, y1],
                Some(a) => [a[0].min(x), a[1].min(y), a[2].max(x1), a[3].max(y1)],
            });
        }
        for c in &f.children {
            walk(c, x, y, id, acc);
        }
    }
    let mut out = Vec::new();
    for (i, page) in done.layout.pages.iter().enumerate() {
        let mut acc = None;
        walk(&page.root, 0.0, 0.0, id, &mut acc);
        if let Some(a) = acc {
            out.push((i + 1, [a[0], a[1], a[2] - a[0], a[3] - a[1]]));
        }
    }
    out
}

/// The computed values `explain` reports, as CSS-like strings.
fn style_table(s: &ComputedStyle) -> BTreeMap<String, String> {
    let mut t = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        t.insert(k.to_string(), v);
    };
    let px = |v: Scalar| format!("{}pt", round2(v.get()));
    let opt_len = |v: Option<Scalar>| v.map(px).unwrap_or_else(|| "auto".to_string());
    put("display", kebab(&format!("{:?}", s.display)));
    put("position", kebab(&format!("{:?}", s.position)));
    put("float", kebab(&format!("{:?}", s.float)));
    put("color", color(s.color));
    put(
        "background-color",
        s.background_color.map(color).unwrap_or_else(|| "transparent".to_string()),
    );
    put("font-family", s.font_family.clone());
    put("font-size", px(s.font_size));
    put("font-weight", format!("{}", s.font_weight));
    put("font-style", kebab(&format!("{:?}", s.font_style)));
    put("line-height", px(s.line_height));
    put(
        "width",
        match (s.width, s.width_percent) {
            (Some(w), _) => px(w),
            (None, Some(p)) => format!("{}%", round2(p * 100.0)),
            _ => "auto".to_string(),
        },
    );
    put(
        "height",
        match (s.height, s.height_percent) {
            (Some(h), _) => px(h),
            (None, Some(p)) => format!("{}%", round2(p * 100.0)),
            _ => "auto".to_string(),
        },
    );
    put("margin-top", if s.margin_top_auto { "auto".into() } else { px(s.margin_top) });
    put("margin-right", if s.margin_right_auto { "auto".into() } else { px(s.margin_right) });
    put("margin-bottom", if s.margin_bottom_auto { "auto".into() } else { px(s.margin_bottom) });
    put("margin-left", if s.margin_left_auto { "auto".into() } else { px(s.margin_left) });
    put("padding-top", px(s.padding_top));
    put("padding-right", px(s.padding_right));
    put("padding-bottom", px(s.padding_bottom));
    put("padding-left", px(s.padding_left));
    put("border-top-width", px(s.border_top));
    put("border-right-width", px(s.border_right));
    put("border-bottom-width", px(s.border_bottom));
    put("border-left-width", px(s.border_left));
    put("break-before", kebab(&format!("{:?}", s.break_before)));
    put("break-after", kebab(&format!("{:?}", s.break_after)));
    put("break-inside", kebab(&format!("{:?}", s.break_inside)));
    put("orphans", s.orphans.to_string());
    put("widows", s.widows.to_string());
    put("page", s.page.clone().unwrap_or_else(|| "auto".to_string()));
    put("z-index", s.z_index.map(|z| z.to_string()).unwrap_or_else(|| "auto".to_string()));
    put("opacity", format!("{}", s.opacity));
    put("inset-top", opt_len(s.inset_top));
    put("inset-right", opt_len(s.inset_right));
    put("inset-bottom", opt_len(s.inset_bottom));
    put("inset-left", opt_len(s.inset_left));
    t
}

fn color(c: crate::css::Color) -> String {
    if c.a == 255 {
        format!("rgb({}, {}, {})", c.r, c.g, c.b)
    } else {
        format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, round2(c.a as f64 / 255.0))
    }
}

/// `TableCell` → `table-cell`; tuple payloads keep their Debug text.
fn kebab(debug: &str) -> String {
    let mut out = String::new();
    for (i, ch) in debug.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
