//! CORE-571: render report — layout diagnostics mapped to source.
//!
//! Spec: `docs/specifications/render-report.spec.md`. Each event class has a
//! fixture here asserting its code, page, and HTML source position.

use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;
use typeanvil::render_report::{analyze_layout, LayoutEvent, RenderReport};

/// 5in x 3in, 0.5in margins: content area 288 x 144pt.
fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

fn report(css: &str, html: &str) -> RenderReport {
    let dom = Dom::parse(html).expect("parse");
    let sheet = Stylesheet::parse(css);
    let l = layout(&dom, &sheet, geo());
    analyze_layout(&dom, &sheet, &l)
}

fn find<'a>(r: &'a RenderReport, code: &str) -> Vec<&'a LayoutEvent> {
    r.events.iter().filter(|e| e.code == code).collect()
}

const BASE: &str = "html,body{margin:0} p{margin:0;font-size:12pt;line-height:14pt}";

#[test]
fn source_positions_recorded() {
    let dom = Dom::parse("<!DOCTYPE html>\n<body>\n  <p>a</p><p>b</p>\n</body>").unwrap();
    let ps: Vec<usize> = (0..dom.nodes.len())
        .filter(|&i| dom.nodes[i].kind.element().is_some_and(|e| e.tag == "p"))
        .collect();
    assert_eq!(dom.source_pos(ps[0]), (3, 3));
    assert_eq!(dom.source_pos(ps[1]), (3, 11));
    // Implied <head> is synthesized: unknown.
    let head = dom.find_tag("head").unwrap();
    assert_eq!(dom.source_pos(head), (0, 0));
}

#[test]
fn overflow_event() {
    let r = report(
        &format!("{BASE} .wide{{width:500pt;height:20pt;background:red}}"),
        "<!DOCTYPE html>\n<body>\n<p>intro</p>\n  <div id=\"w\" class=\"wide\">x</div>\n</body>",
    );
    let ev = find(&r, "layout-overflow");
    assert_eq!(ev.len(), 1, "{:?}", r.events);
    let e = ev[0];
    assert_eq!((e.page, e.line, e.column), (1, 4, 3));
    assert_eq!(e.element, "div#w.wide");
    let rect = e.rect.expect("rect");
    assert_eq!(rect[2], 500.0);
    assert_eq!(rect[0], 36.0);
}

#[test]
fn empty_page_event() {
    let r = report(
        &format!("{BASE} .b{{break-before:page}} .t{{height:144pt}}"),
        "<!DOCTYPE html>\n<body>\n<div class=\"t\"></div>\n<p class=\"b\">after</p>\n</body>",
    );
    let ev = find(&r, "empty-page");
    assert_eq!(ev.len(), 1, "{:?} pages={}", r.events, r.summary.pages);
    assert_eq!(ev[0].page, 1);
    assert_eq!(ev[0].rect, None);
}

#[test]
fn orphaned_heading_event() {
    // 9 lines of 14pt = 126pt; the 14pt heading ends at 140pt; the next
    // paragraph does not fit in the remaining 4pt. The UA sheet gives
    // headings `break-after: avoid`; the author overrides it here, which is
    // exactly the document the hint is for.
    let mut body = String::from("<!DOCTYPE html>\n<body>\n");
    for _ in 0..9 {
        body.push_str("<p>line</p>\n");
    }
    body.push_str("<h2 class=\"sec\">Heading</h2>\n<p>section text</p>\n</body>");
    let r = report(
        &format!("{BASE} h2{{margin:0;font-size:12pt;line-height:14pt;break-after:auto}}"),
        &body,
    );
    let ev = find(&r, "orphaned-heading");
    assert_eq!(ev.len(), 1, "{:?}", r.events);
    let e = ev[0];
    assert_eq!((e.page, e.line, e.column), (1, 12, 1));
    assert_eq!(e.element, "h2.sec");
    assert_eq!(e.hint.as_deref(), Some("add `break-after: avoid` to h2.sec"));
}

fn table_html(rows: usize) -> String {
    let mut s = String::from(
        "<!DOCTYPE html>\n<body>\n<table id=\"t\">\n<thead><tr><td>H</td></tr></thead>\n<tbody>\n",
    );
    for i in 0..rows {
        s.push_str(&format!("<tr><td>r{i}</td></tr>\n"));
    }
    s.push_str("</tbody></table>\n</body>");
    s
}

const TABLE_CSS: &str =
    "html,body{margin:0} table{border-spacing:0} td{padding:0;height:20pt;font-size:10pt;line-height:12pt}";

#[test]
fn table_single_row_event() {
    // 144pt holds the header + 6 rows; 7 body rows leaves one on page 2.
    let r = report(TABLE_CSS, &table_html(7));
    assert_eq!(r.summary.pages, 2, "{:?}", r.events);
    let ev = find(&r, "table-single-row-fragment");
    assert_eq!(ev.len(), 1, "{:?}", r.events);
    assert_eq!((ev[0].page, ev[0].line, ev[0].column), (2, 3, 1));
    assert_eq!(ev[0].element, "table#t");
    assert!(find(&r, "table-header-only").is_empty());
}

#[test]
fn table_header_only_event() {
    // Row "a" fits on page 1; the 200pt row moves on, and page 2 carries the
    // repeated header with no body row placed beside it.
    let html = "<!DOCTYPE html>\n<body>\n<table id=\"t\">\n<thead><tr><td>H</td></tr></thead>\n<tbody>\n<tr><td>a</td></tr>\n<tr class=\"big\"><td>b</td></tr>\n</tbody></table>\n</body>";
    let r = report(
        &format!("{TABLE_CSS} tr.big td{{height:200pt}}"),
        html,
    );
    let ev = find(&r, "table-header-only");
    assert_eq!(r.summary.pages, 2, "{:?}", r.events);
    assert_eq!(ev.len(), 1, "{:?}", r.events);
    assert_eq!((ev[0].page, ev[0].line, ev[0].column), (2, 3, 1));
    assert_eq!(ev[0].element, "table#t");
}

#[test]
fn image_font_target_link_events() {
    let html = "<!DOCTYPE html>\n<body>\n<img src=\"/nonexistent/missing.png\" width=\"20\" height=\"20\">\n<p style=\"font-family: NoSuchFamilyXyz\">fallback</p>\n<a class=\"toc\" href=\"#nope\">ref</a>\n</body>";
    let r = report(
        &format!("{BASE} a.toc::after{{content: target-counter(attr(href), page)}} a.toc{{content: target-counter(attr(href), page)}}"),
        html,
    );
    let img = find(&r, "image-missing");
    assert_eq!(img.len(), 1, "{:?}", r.events);
    assert_eq!((img[0].page, img[0].line, img[0].column), (1, 3, 1));
    let font = find(&r, "font-fallback");
    assert_eq!(font.len(), 1, "{:?}", r.events);
    assert_eq!((font[0].page, font[0].line), (1, 4));
    assert!(font[0].message.contains("NoSuchFamilyXyz"), "{}", font[0].message);
    let tgt = find(&r, "target-unresolved");
    assert_eq!(tgt.len(), 1, "{:?}", r.events);
    // The inline <a> has no box of its own: the page comes from its ancestor.
    assert_eq!((tgt[0].page, tgt[0].line, tgt[0].column), (1, 5, 1));
    let link = find(&r, "link-target-missing");
    assert_eq!(link.len(), 1, "{:?}", r.events);
    assert_eq!((link[0].page, link[0].line, link[0].column), (1, 5, 1));
    assert_eq!(link[0].element, "a.toc");
}

#[test]
fn summary_counts_passes() {
    let one = report(BASE, "<!DOCTYPE html><body><p>x</p></body>");
    assert_eq!((one.summary.pages, one.summary.passes), (1, 1));
    assert!(one.summary.font_bundle.contains('@'));
    let two = report(
        &format!("{BASE} p::after{{content: counter(pages)}} p.x{{content: counter(pages)}}"),
        "<!DOCTYPE html><body><p class=\"x\">x</p></body>",
    );
    assert_eq!(two.summary.pages, 1);
    assert_eq!(two.summary.passes, 2, "counter(pages) runs a confirming pass");
}

#[test]
fn clean_document_has_no_layout_events() {
    let r = report(BASE, "<!DOCTYPE html>\n<body>\n<h1>T</h1>\n<p>body</p>\n</body>");
    assert!(r.events.is_empty(), "{:?}", r.events);
}

fn cli(html: &str, name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("ta-core571-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.html");
    std::fs::write(&input, html).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_typeanvil"))
        .arg("render")
        .arg(&input)
        .args([
            "--page-width", "5in", "--page-height", "3in", "--margin-top", "0.5in",
            "--margin-right", "0.5in", "--margin-bottom", "0.5in", "--margin-left", "0.5in",
            "--diagnostics", "json", "-o",
        ])
        .arg(dir.join("out.pdf"))
        .output()
        .expect("run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

const CLI_FIXTURE: &str = "<!DOCTYPE html>\n<html><head><style>\nhtml,body{margin:0}\n.wide{width:500pt;height:20pt;background-color:red;text-indent:1pt}\n</style></head>\n<body>\n<div class=\"wide\">x</div>\n<img src=\"missing.png\">\n</body></html>";

#[test]
fn cli_json_is_deterministic() {
    let a = cli(CLI_FIXTURE, "a");
    let b = cli(CLI_FIXTURE, "b");
    assert_eq!(a, b);
    assert!(a.starts_with("{\n  \"schema\": 2,\n"), "{a}");
    // CSS event first, unchanged shape; then layout events; then summary.
    let css = a.find("\"unsupported-property\"").expect("css event");
    let lay = a.find("\"layout-overflow\"").expect("overflow event");
    assert!(css < lay, "{a}");
    assert!(a.contains("\"line\": 7, \"column\": 1, \"source\": \"layout\", \"page\": 1, \"rect\": [36, 36, 500, 20], \"element\": \"div.wide\"}"), "{a}");
    assert!(a.contains("\"image-missing\""), "{a}");
    assert!(a.contains("\"summary\": {\"pages\": 1, \"passes\": 1, \"font_bundle\": "), "{a}");
}
