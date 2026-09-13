//! Paged-media acceptance tests — one per acceptance criterion in
//! `docs/specifications/paged-media-css.spec.md`.
//!
//! These drive the library directly (`typeanvil::layout::layout`) and assert on
//! the fragment tree / outline model, plus determinism through the CLI. The
//! fragmentation tests (forced break, orphans/widows, avoid, monolithic,
//! 1,000-page linear) live in `fragmentation.rs` and run unchanged — this file
//! never touches them (regression guard, spec AC #13).

use std::path::Path;
use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, Fragmentainer};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

// --- helpers ---------------------------------------------------------------

/// Points from inches.
fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

/// A page geometry with uniform margins (inches).
fn geometry(w_in: f64, h_in: f64, margin_in: f64) -> PageGeometry {
    PageGeometry {
        width: inches(w_in),
        height: inches(h_in),
        margin_top: inches(margin_in),
        margin_right: inches(margin_in),
        margin_bottom: inches(margin_in),
        margin_left: inches(margin_in),
    }
}

/// Collect all `<style>` text into a stylesheet (mirrors the CLI).
fn stylesheet_of(dom: &Dom) -> Stylesheet {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    Stylesheet::parse(&css)
}

/// Lay out an HTML string with the given geometry.
fn lay(html: &str, geo: PageGeometry) -> Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    layout(&dom, &ss, geo)
}

/// All text-run strings on one fragmentainer, in pre-order (document order).
fn page_texts(page: &Fragmentainer) -> Vec<String> {
    let mut out = Vec::new();
    collect_text(&page.root, &mut out);
    out
}

fn collect_text(frag: &Fragment, out: &mut Vec<String>) {
    if let FragmentContent::Text(run) = &frag.content {
        out.push(run.text.clone());
    }
    for child in &frag.children {
        collect_text(child, out);
    }
}

fn any_text_contains(page: &Fragmentainer, needle: &str) -> bool {
    page_texts(page).iter().any(|t| t.contains(needle))
}

/// The absolute (x, y) of the first text run containing `needle` on a page.
fn text_pos(page: &Fragmentainer, needle: &str) -> Option<(f64, f64)> {
    fn walk(frag: &Fragment, px: f64, py: f64, needle: &str) -> Option<(f64, f64)> {
        let ax = px + frag.offset.x.get();
        let ay = py + frag.offset.y.get();
        if let FragmentContent::Text(run) = &frag.content {
            if run.text.contains(needle) {
                return Some((px + run.baseline.x.get(), py + run.baseline.y.get()));
            }
        }
        for child in &frag.children {
            if let Some(p) = walk(child, ax, ay, needle) {
                return Some(p);
            }
        }
        None
    }
    walk(&page.root, 0.0, 0.0, needle)
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_typeanvil")
}

fn render_cli(html: &Path, out: &Path, w: &str, h: &str) {
    let status = Command::new(bin())
        .args([
            "render",
            html.to_str().unwrap(),
            "--page-width",
            w,
            "--page-height",
            h,
            "--margin-top",
            "0.5in",
            "--margin-right",
            "0.5in",
            "--margin-bottom",
            "0.5in",
            "--margin-left",
            "0.5in",
            "-o",
            out.to_str().unwrap(),
        ])
        .status()
        .expect("failed to spawn typeanvil");
    assert!(status.success(), "engine exited non-zero: {status:?}");
}

// --- 1. Page size from @page -----------------------------------------------

#[test]
fn page_size_from_at_page() {
    let html = r#"<html><head><style>
        @page { size: 8.5in 11in; }
        p { font-size: 12px; }
    </style></head><body><p>Hello paged media.</p></body></html>"#;
    // CLI default 5in x 3in must be overridden by @page.
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    for page in &layout.pages {
        assert_eq!(page.root.size.0.get(), inches(8.5).get());
        assert_eq!(page.root.size.1.get(), inches(11.0).get());
    }
}

// --- 2. Page margins from @page --------------------------------------------

#[test]
fn page_margins_from_at_page() {
    let html = r#"<html><head><style>
        @page { size: 8.5in 11in; margin: 1in; }
        p { font-size: 12px; }
    </style></head><body><p>Body text with a one inch margin.</p></body></html>"#;
    let layout = lay(html, geometry(8.5, 11.0, 0.25));
    // The first content line's baseline must sit at least 1in from the top-left.
    let (x, y) = text_pos(&layout.pages[0], "Body").expect("content text missing");
    assert!(x >= inches(1.0).get() - 0.01, "content x {x} not inset 1in");
    assert!(y >= inches(1.0).get() - 0.01, "content y {y} not inset 1in");
}

// --- 3. Margin-box header --------------------------------------------------

#[test]
fn margin_box_header() {
    let html = r#"
    <html><head><style>
        @page { margin: 0.5in; @top-center { content: "Report"; } }
        p { font-size: 12px; }
        .pb { break-before: page; }
    </style></head><body>
        <p>First page paragraph one.</p>
        <p class="pb">Second page paragraph.</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 2, "expected multi-page");
    for page in &layout.pages {
        assert!(
            any_text_contains(page, "Report"),
            "page {} missing top-center header",
            page.index
        );
        // The header sits in the top margin (above the 0.5in content top).
        let (_, y) = text_pos(page, "Report").unwrap();
        assert!(y < inches(0.5).get(), "header y {y} not in top margin");
    }
}

// --- 3b. Margin-box centering (CORE-117) -------------------------------------

#[test]
fn margin_box_center_aligns_on_content_midline() {
    // CORE-117: a @top-center head wider than the middle-third slot must stay
    // centered on the CONTENT-box midline, spilling into adjacent slots
    // symmetrically — never clamp to start-align. The invariant: the run's
    // horizontal center equals (content_left + content_right) / 2 for every
    // page, at any head width.
    let html = r#"<html><head><style>
        @page { margin: 0.5in; @top-center { content: "A Very Long Running Head That Exceeds Any Third Slot Width"; } }
        p { font-size: 12px; }
        .pb { break-before: page; }
    </style></head><body>
        <p>First page paragraph.</p>
        <p class="pb">Second page paragraph.</p>
    </body></html>"#;
    let geo = geometry(5.0, 3.0, 0.5);
    let layout = lay(html, geo);
    assert!(layout.pages.len() >= 2, "expected multi-page");
    let content = geo.content_rect();
    let midline = (content.x.get() + (content.x.get() + content.width.get())) * 0.5;
    for page in &layout.pages {
        fn walk<'a>(frag: &'a Fragment, px: f64, py: f64) -> Option<(f64, f64, f64)> {
            if let FragmentContent::Text(run) = &frag.content {
                if run.text.contains("Running Head") {
                    let x = px + run.baseline.x.get();
                    // True shaped extent from per-glyph advances.
                    let w: f64 = run.glyphs.iter().map(|g| g.x_advance.get()).sum();
                    return Some((x, w, py + run.baseline.y.get()));
                }
            }
            let ax = px + frag.offset.x.get();
            let ay = py + frag.offset.y.get();
            for child in &frag.children {
                if let Some(hit) = walk(child, ax, ay) {
                    return Some(hit);
                }
            }
            None
        }
        let (x, w, y) = walk(&page.root, 0.0, 0.0)
            .unwrap_or_else(|| panic!("page {} missing top-center head", page.index));
        assert!(
            y < inches(0.5).get(),
            "head must sit in the top margin band"
        );
        let center = x + w * 0.5;
        assert!(
            (center - midline).abs() < 0.5,
            "page {}: head center {center} not on content midline {midline}",
            page.index
        );
    }
}

// --- 4. Named pages --------------------------------------------------------

#[test]
fn named_pages() {
    let html = r#"<html><head><style>
        @page { size: 5in 3in; }
        @page landscape { size: 8in 3in; }
        section { display: block; }
        section.note { page: landscape; break-before: page; }
        p { font-size: 12px; }
    </style></head><body>
        <section><p>Default page section.</p></section>
        <section class="note"><p>Landscape section.</p></section>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.25));
    assert!(layout.pages.len() >= 2, "expected two pages");
    assert_eq!(layout.pages[0].root.size.0.get(), inches(5.0).get());
    // The landscape section starts page 2 → wider page.
    assert_eq!(layout.pages[1].root.size.0.get(), inches(8.0).get());
}

#[test]
fn named_page_margin_box_suppression() {
    // CORE-82: `@page cover { @top-left { content: none } }` must suppress the
    // margin boxes on pages the cover element's boxes start, while pages after
    // a `page: auto` reset keep the default page's margin boxes. Previously
    // page 1 (a fresh token with no child tokens) never activated the named
    // page, and `page: auto` was indistinguishable from unset.
    let html = r#"<html><head><style>
        @page { margin: 0.4in;
            @top-left { content: "Northwind"; }
            @bottom-center { content: counter(page); } }
        @page cover { margin: 0.4in;
            @top-left { content: none; }
            @bottom-center { content: none; } }
        .cover { page: cover; break-after: page; }
        .body { page: auto; }
        p { font-size: 12px; }
    </style></head><body>
        <div class="cover"><p>Cover content.</p></div>
        <div class="body"><p>Letter body.</p></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    assert!(layout.pages.len() >= 2, "expected cover + letter pages");
    // Cover page: no default margin-box content.
    assert!(
        !any_text_contains(&layout.pages[0], "Northwind"),
        "cover page must suppress the top-left margin box: {:?}",
        page_texts(&layout.pages[0])
    );
    assert!(
        !any_text_contains(&layout.pages[0], "1"),
        "cover page must suppress the bottom-center counter: {:?}",
        page_texts(&layout.pages[0])
    );
    // Letter page: the default page's margin boxes return.
    assert!(
        any_text_contains(&layout.pages[1], "Northwind"),
        "letter page must keep the default top-left margin box: {:?}",
        page_texts(&layout.pages[1])
    );
}

#[test]
fn named_page_first_page_activates() {
    // CORE-82: the cover element is the FIRST box in the document — page 1's
    // incoming token is a bare break-before with no child tokens. The named
    // page must still activate (margin boxes suppressed), not fall through to
    // the default page. Regression for the page-1 walk.
    let html = r#"<html><head><style>
        @page { margin: 0.4in; @top-center { content: "HDR"; } }
        @page cover { margin: 0.4in; @top-center { content: none; } }
        .cover { page: cover; }
        p { font-size: 12px; }
    </style></head><body>
        <div class="cover"><p>Cover body text that fills the page.</p></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    assert!(!layout.pages.is_empty());
    for page in &layout.pages {
        assert!(
            !any_text_contains(page, "HDR"),
            "cover pages must not carry the default header: {:?}",
            page_texts(page)
        );
    }
}

// --- 5. First / left / right selectors -------------------------------------

#[test]
fn first_left_right() {
    let html = r#"
    <html><head><style>
        @page { margin: 0.25in; }
        @page :first { margin-top: 1.5in; }
        p { font-size: 12px; }
        .pb { break-before: page; }
    </style></head><body>
        <p>First page content one.</p>
        <p class="pb">Second page content.</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.25));
    assert!(layout.pages.len() >= 2);
    let (_, y1) = text_pos(&layout.pages[0], "First").unwrap();
    let (_, y2) = text_pos(&layout.pages[1], "Second").unwrap();
    // Page 1 has a 1.5in top margin; page 2 uses the default 0.25in.
    assert!(y1 > y2, "page 1 top ({y1}) should be below page 2 top ({y2})");
    assert!(y1 >= inches(1.5).get() - 0.01, "page 1 top margin not applied");
}

// --- 6. Running string -----------------------------------------------------

#[test]
fn running_header_string() {
    let html = r#"<html><head><style>
        @page { margin: 0.4in; @top-left { content: string(chapter); } }
        h1 { string-set: chapter content(); font-size: 14px; }
        .c { display: block; break-before: page; }
        p { font-size: 12px; }
    </style></head><body>
        <p>Front matter before any heading.</p>
        <div class="c"><h1>Alpha</h1><p>Alpha body.</p></div>
        <div class="c"><h1>Beta</h1><p>Beta body.</p></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    assert!(layout.pages.len() >= 3, "expected front + two chapters");
    // Page 1 (front matter) has no chapter assigned yet → empty header.
    assert!(
        !any_text_contains(&layout.pages[0], "Alpha")
            && !any_text_contains(&layout.pages[0], "Beta"),
        "page 1 header should be empty before first h1"
    );
    // Later pages carry the current chapter in the top-left.
    let has_alpha = layout.pages.iter().any(|p| any_text_contains(p, "Alpha"));
    let has_beta = layout.pages.iter().any(|p| any_text_contains(p, "Beta"));
    assert!(has_alpha && has_beta, "chapter headers missing");
}

// --- 6b. Running strings: css-gcpm-3 §7 keywords (CORE-108) ----------------

/// Two chapters, one per page (forced breaks), with all five keyword slots in
/// the margin boxes. Asserts the Prince-probed semantics table: first/last
/// take this page's assignments, start takes the value entering the page,
/// first-except is empty on pages WITH an assignment and carried elsewhere.
#[test]
fn string_keywords_first_last_start() {
    let html = r#"<html><head><style>
        @page { margin: 0.4in;
            @top-left { content: "F:" string(chapter, first); }
            @top-center { content: "S:" string(chapter, start); }
            @top-right { content: "L:" string(chapter, last); } }
        h1 { string-set: chapter content(); font-size: 14px; }
        .c { display: block; break-before: page; }
    </style></head><body>
        <div class="c"><h1>Alpha</h1><p>a</p></div>
        <div class="c"><h1>Beta</h1><p>b</p></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    assert!(layout.pages.len() >= 2);
    // Page 1 has an assignment (Alpha): F=Alpha, S empty (nothing entering),
    // L=Alpha.
    let p1 = page_texts(&layout.pages[0]).join("\n");
    let p2 = page_texts(&layout.pages[1]).join("\n");
    assert!(p1.contains("F:Alpha"), "page 1 first != Alpha: {p1:?}");
    assert!(p1.contains("L:Alpha"), "page 1 last != Alpha: {p1:?}");
    assert!(
        p1.contains("S:") && !p1.contains("S:Alpha") && !p1.contains("S:Beta"),
        "page 1 start must be EMPTY before any assignment: {p1:?}"
    );
    // Page 2 has an assignment (Beta): F=Beta but S=Alpha (the value ENTERING
    // the page — Prince ignores even a top-of-page assignment for `start`).
    assert!(p2.contains("F:Beta"), "page 2 first != Beta: {p2:?}");
    assert!(p2.contains("S:Alpha"), "page 2 start != Alpha carry: {p2:?}");
    assert!(p2.contains("L:Beta"), "page 2 last != Beta: {p2:?}");
}

/// A chapter assigned on page 1 only: pages without assignments show the
/// CARRIED value for every keyword except that `first-except` shows it too
/// (it is only empty on a page WITH an assignment).
#[test]
fn string_carry_over_and_first_except() {
    let html = r#"<html><head><style>
        @page { margin: 0.4in;
            @top-left { content: "F:" string(chapter); }
            @top-right { content: "X:" string(chapter, first-except); } }
        h1 { string-set: chapter content(); font-size: 14px; break-after: page; }
    </style></head><body>
        <h1>Solo</h1>
        <p>Filler.</p><p>Filler.</p><p>Filler.</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    assert!(layout.pages.len() >= 2, "need a continuation page");
    // Page 1 HAS the assignment → first-except empty; default(first) = Solo.
    let p1 = page_texts(&layout.pages[0]).join("\n");
    assert!(p1.contains("F:Solo"), "page 1 default != Solo: {p1:?}");
    assert!(
        !p1.contains("X:Solo"),
        "page 1 first-except must be empty: {p1:?}"
    );
    // Page 2 has NO assignment → both show the carried value.
    let p2 = page_texts(&layout.pages[1]).join("\n");
    assert!(p2.contains("F:Solo"), "page 2 carry-over missing: {p2:?}");
    assert!(p2.contains("X:Solo"), "page 2 first-except carry missing: {p2:?}");
}

/// Two assignments on ONE page (two h1s): `first` takes the earlier, `last`
/// the later — the distinguishing case for the page assignment log's ORDER.
#[test]
fn string_first_vs_last_with_two_assignments_on_one_page() {
    let html = r#"<html><head><style>
        @page { margin: 0.4in;
            @top-left { content: "F:" string(chapter, first); }
            @top-right { content: "L:" string(chapter, last); } }
        h1 { string-set: chapter content(); font-size: 14px; }
    </style></head><body>
        <h1>One</h1>
        <p>filler</p>
        <h1>Two</h1>
        <p>filler</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let p1 = page_texts(&layout.pages[0]).join("\n");
    assert!(p1.contains("F:One"), "first of two != One: {p1:?}");
    assert!(p1.contains("L:Two"), "last of two != Two: {p1:?}");
}

/// The parser splits `string(name, keyword)` correctly: the name must not
/// contain the argument list, and an unknown keyword falls back to `first`.
#[test]
fn string_reference_parses_name_and_keyword() {
    use typeanvil::paged::{parse_content, ContentPiece, StringKeyword};
    assert_eq!(
        parse_content("string(chapter)"),
        vec![ContentPiece::StringRef("chapter".into(), StringKeyword::First)]
    );
    assert_eq!(
        parse_content("string(chapter, last)"),
        vec![ContentPiece::StringRef("chapter".into(), StringKeyword::Last)]
    );
    assert_eq!(
        parse_content("string(chapter, FIRST-EXCEPT)"),
        vec![ContentPiece::StringRef(
            "chapter".into(),
            StringKeyword::FirstExcept
        )]
    );
    // Malformed keyword falls back to First, name stays clean.
    assert_eq!(
        parse_content("string(chapter, bogus)"),
        vec![ContentPiece::StringRef("chapter".into(), StringKeyword::First)]
    );
}

// --- 7. Page counter -------------------------------------------------------

#[test]
fn page_counter() {
    let html = r#"
    <html><head><style>
        @page { margin: 0.4in; @bottom-right { content: counter(page); } }
        p { font-size: 12px; }
        .pb { break-before: page; }
    </style></head><body>
        <p>Page one.</p>
        <p class="pb">Page two.</p>
        <p class="pb">Page three.</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    assert!(layout.pages.len() >= 3);
    assert!(any_text_contains(&layout.pages[0], "1"));
    assert!(any_text_contains(&layout.pages[1], "2"));
    assert!(any_text_contains(&layout.pages[2], "3"));

    // counter-reset: page 0 restarts the count at the section's page.
    let html2 = r#"<html><head><style>
        @page { margin: 0.4in; @bottom-right { content: counter(page); } }
        .reset { display: block; break-before: page; counter-reset: page 0; }
        p { font-size: 12px; }
    </style></head><body>
        <p>First.</p>
        <div class="reset"><p>Reset here.</p></div>
    </body></html>"#;
    let l2 = lay(html2, geometry(5.0, 3.0, 0.4));
    assert!(l2.pages.len() >= 2);
    // Page 2 is reset to counter 0 (bottom-right shows "0").
    let p2 = page_texts(&l2.pages[1]);
    assert!(p2.iter().any(|t| t == "0"), "reset page counter not 0: {p2:?}");
}

#[test]
fn total_page_counter() {
    // counter(pages) renders the document's total page count on every page
    // (CORE-84: footer previously showed "Page N of 0").
    let html = r#"
    <html><head><style>
        @page { margin: 0.4in; @bottom-center { content: "Page " counter(page) " of " counter(pages); } }
        p { font-size: 12px; }
        .pb { break-before: page; }
    </style></head><body>
        <p>Page one.</p>
        <p class="pb">Page two.</p>
        <p class="pb">Page three.</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let total = layout.pages.len();
    assert!(total >= 3, "expected a multi-page doc, got {total} page(s)");
    for (i, page) in layout.pages.iter().enumerate() {
        let want = format!("Page {} of {}", i + 1, total);
        assert!(
            any_text_contains(page, &want),
            "page {} footer wrong: {:?}",
            i + 1,
            page_texts(page)
        );
    }
}

// --- 8. TOC target-counter -------------------------------------------------

#[test]
fn toc_target_counter() {
    let html = r##"<html><head><style>
        @page { margin: 0.4in; }
        body { margin: 0; }
        .toc a { display: block; }
        .e1 { content: "Chapter 1 " leader('.') target-counter(attr(href), page); }
        .e2 { content: "Chapter 2 " leader('.') target-counter(attr(href), page); }
        .ch { display: block; break-before: page; }
        h1 { font-size: 14px; }
        p { font-size: 12px; }
    </style></head><body>
        <div class="toc">
            <a class="e1" href="#ch1">Chapter 1</a>
            <a class="e2" href="#ch2">Chapter 2</a>
        </div>
        <div class="ch"><h1 id="ch1">One</h1><p>Body one.</p></div>
        <div class="ch"><h1 id="ch2">Two</h1><p>Body two.</p></div>
    </body></html>"##;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let toc = page_texts(&layout.pages[0]);
    // ch1 starts on page 2 (index 1) and ch2 on page 3 (index 2): 1-based 2, 3.
    let e1 = toc.iter().find(|t| t.contains("Chapter 1")).expect("entry 1");
    let e2 = toc.iter().find(|t| t.contains("Chapter 2")).expect("entry 2");
    assert!(e1.ends_with('2'), "entry 1 wrong page number: {e1:?}");
    assert!(e2.ends_with('3'), "entry 2 wrong page number: {e2:?}");
    // Dotted leader present.
    assert!(e1.contains(".."), "entry 1 missing leader dots: {e1:?}");
    // The leader reaches near the right content edge: line width ~= content
    // width. The content box is (5 - 0.8)in = 4.2in wide; the filled line
    // should be within one glyph advance of that width.
    let content_w = inches(5.0 - 0.8).get();
    let advance = 12.0 * 0.5;
    let line_w = e1.chars().count() as f64 * advance;
    assert!(
        line_w >= content_w - advance,
        "leader did not reach content edge: line {line_w} vs {content_w}"
    );
}

// --- 8b. Leader fill pitch uses the REAL '.' advance (CORE-99) --------------

#[test]
fn leader_fill_uses_real_dot_advance() {
    // CORE-99: `leader('.')` filled at the flat 0.5em AVG_ADVANCE_EM
    // heuristic, so dots were ~44% sparser than Prince's (report TOC: ~22
    // dots vs Prince's ~28 at demo geometry) and the run stopped short of
    // the right content edge. The fill pitch must be the shaped '.' advance;
    // only the RESERVATION for resolved pieces stays glyph-independent
    // (spec §9/§10 two-pass convergence).
    let html = r##"<html><head><style>
        @page { margin: 0.4in; }
        body { margin: 0; }
        .toc a { display: block; }
        .e1 { content: "Chapter 1 " leader('.') target-counter(attr(href), page); }
        .ch { display: block; break-before: page; }
        h1 { font-size: 14px; }
        p { font-size: 12px; }
    </style></head><body>
        <div class="toc">
            <a class="e1" href="#ch1">Chapter 1</a>
        </div>
        <div class="ch"><h1 id="ch1">One</h1><p>Body one.</p></div>
    </body></html>"##;
    let layout = lay(html, geometry(5.0, 0.5, 0.4));
    let toc = page_texts(&layout.pages[0]);
    let e1 = toc.iter().find(|t| t.contains("Chapter 1")).expect("entry 1");
    let dots = e1.chars().filter(|&c| c == '.').count() as f64;
    assert!(e1.ends_with('2'), "entry wrong page number: {e1:?}");

    // Independent expectation from the public shaping API. Body default is
    // 16px = 12pt, regular face. Reservation: the literal at its real shaped
    // width + the resolved page-number piece at the 0.5em heuristic.
    let fs = typeanvil::geom::Scalar(12.0);
    let face = typeanvil::fonts::FACE_REGULAR;
    let dot_w = typeanvil::typography::shape_word(".", fs, face).width.get();
    let lit_w = typeanvil::typography::shape_word("Chapter 1 ", fs, face).width.get();
    let content_w = inches(5.0 - 0.8).get();
    let expected = ((content_w - lit_w - 0.5 * 12.0) / dot_w).floor();
    assert!(
        dots >= expected && dots <= expected + 1.0,
        "leader dot count {dots} does not match real-advance fill {expected}: {e1:?}"
    );
    // The old 0.5em-pitch fill produced far fewer dots; pin the improvement.
    let heuristic_dots = ((content_w - lit_w - 0.5 * 12.0) / (0.5 * 12.0)).floor();
    assert!(
        dots > heuristic_dots,
        "fill regressed to the 0.5em heuristic: {dots} <= {heuristic_dots}"
    );
}

// --- 9. PDF bookmarks ------------------------------------------------------

#[test]
fn pdf_bookmarks() {
    let html = r#"<html><head><style>
        @page { margin: 0.4in; }
        .ch { display: block; break-before: page; }
        h1 { font-size: 16px; }
        h2 { font-size: 13px; }
        p { font-size: 12px; }
    </style></head><body>
        <div class="ch"><h1>Alpha</h1><h2>Alpha One</h2><p>Body.</p><h2>Alpha Two</h2><p>Body.</p></div>
        <div class="ch"><h1>Beta</h1><h2>Beta One</h2><p>Body.</p></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let hs = &layout.headings;
    // Five headings in DOM order: h1 Alpha, h2 Alpha One, h2 Alpha Two, h1 Beta,
    // h2 Beta One.
    assert_eq!(hs.len(), 5, "heading count: {hs:?}");
    assert_eq!(hs[0].level, 1);
    assert_eq!(hs[0].title, "Alpha");
    assert_eq!(hs[1].level, 2);
    assert_eq!(hs[1].title, "Alpha One");
    assert_eq!(hs[3].level, 1);
    assert_eq!(hs[3].title, "Beta");
    // Alpha is on page 0, Beta on a later page.
    assert_eq!(hs[0].page_index, 0);
    assert!(hs[3].page_index > hs[0].page_index, "Beta not on a later page");
}

// --- 10. Invoice demo ------------------------------------------------------

#[test]
fn invoice_demo() {
    let dir = tempfile::tempdir().unwrap();
    let html = Path::new("tests/fixtures/invoice.html");
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    render_cli(html, &a, "5in", "3in");
    render_cli(html, &b, "5in", "3in");

    let ba = std::fs::read(&a).unwrap();
    let bb = std::fs::read(&b).unwrap();
    assert!(ba.len() > 1024, "invoice PDF too small");
    assert!(ba.starts_with(b"%PDF"), "not a PDF");
    assert_eq!(ba, bb, "invoice PDF is not byte-deterministic");

    // Multi-page: assert via the fragment tree (marker counting is flaky).
    let src = std::fs::read_to_string(html).unwrap();
    let dom = Dom::parse(&src).unwrap();
    let ss = stylesheet_of(&dom);
    let layout = layout(&dom, &ss, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 2, "invoice should be multi-page");
    // Running header + footer page number appear.
    assert!(any_text_contains(&layout.pages[0], "ACME"));
    assert!(any_text_contains(&layout.pages[0], "1"));
}

// --- 11. Report demo -------------------------------------------------------

#[test]
fn report_demo() {
    let dir = tempfile::tempdir().unwrap();
    let html = Path::new("tests/fixtures/report.html");
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    render_cli(html, &a, "5in", "3in");
    render_cli(html, &b, "5in", "3in");
    let ba = std::fs::read(&a).unwrap();
    let bb = std::fs::read(&b).unwrap();
    assert!(ba.starts_with(b"%PDF"));
    assert_eq!(ba, bb, "report PDF is not byte-deterministic");

    let src = std::fs::read_to_string(html).unwrap();
    let dom = Dom::parse(&src).unwrap();
    let ss = stylesheet_of(&dom);
    let layout = layout(&dom, &ss, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 4, "report should be multi-page (TOC + 3 chapters)");

    // The TOC page numbers are present and correct: each entry ends in a page
    // number that matches the chapter's landing page.
    let toc = page_texts(&layout.pages[0]);
    let e1 = toc.iter().find(|t| t.contains("Chapter 1")).expect("toc e1");
    assert!(e1.contains(".."), "TOC leader dots missing");
    // Chapter 1 lands on page index 1 (2nd page) → number "2".
    assert!(e1.trim_end().ends_with('2'), "TOC entry 1 page number: {e1:?}");
    // Headings drive the outline.
    assert!(layout.headings.iter().any(|h| h.title == "Foundations"));
}

// --- 12. Determinism -------------------------------------------------------

#[test]
fn determinism_multi() {
    let html = r#"<html><head><style>
        @page { margin: 0.4in;
            @top-center { content: "Deterministic"; }
            @bottom-right { content: counter(page); } }
        p { font-size: 12px; }
    </style></head><body>
        <p>Page one text.</p>
        <p style="break-before: page">Page two text.</p>
        <p style="break-before: page">Page three text.</p>
    </body></html>"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("in.html");
    std::fs::write(&path, html).unwrap();
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    render_cli(&path, &a, "5in", "3in");
    render_cli(&path, &b, "5in", "3in");
    let ba = std::fs::read(&a).unwrap();
    let bb = std::fs::read(&b).unwrap();
    assert_eq!(ba, bb, "margin-box PDF not byte-identical across runs");
}

// --- 13. UA body margin is 0 (Prince parity, CORE-92) ----------------------
//
// The engine's UA stylesheet changed body margin from 8px (6pt) to 0 to match
// Prince's print default. Before this fix, a bare `<body>` doc had its first
// baseline pushed 6pt down from the content box top, which changed page-break
// decisions and flipped the prose page-count comparison at line-height 1.2.

#[test]
fn ua_body_margin_is_zero() {
    // No body rule; pin p's margin to isolate the body UA default.
    let html = "<html><head><style>p { margin: 0; }</style></head><body><p>Hi.</p></body></html>";
    // 0.5in margin → content top = 36pt. 16px default font → 12pt.
    let geo = geometry(5.0, 3.0, 0.5);
    let layout = lay(html, geo);
    let (_, y) = text_pos(&layout.pages[0], "Hi").expect("text must be present");
    // Expected: 36 (content top) + 12*0.9053 + (12*1.2 − 12*0.9053 − 12*0.2119)/2
    // = 36 + 10.864 + (14.4 − 10.864 − 2.543)/2 = 47.36 ± 0.1.
    // The old 8px body margin would have pushed this to ~53.4.
    let expected = 36.0 + 12.0 * 1854.0 / 2048.0
        + (14.4 - 12.0 * 1854.0 / 2048.0 - 12.0 * 434.0 / 2048.0) * 0.5;
    assert!(
        (y - expected).abs() < 0.1,
        "UA body margin: baseline y={y:.3} want {expected:.3} (old push was +6pt → ~{:.3})",
        expected + 6.0
    );
}

// --- 14. UA heading/paragraph defaults are Prince print defaults (CORE-95) --
//
// Prince's UA sheet (lib/prince/style/html.css) uses FIXED point heading
// sizes and margins (h1: 24pt/16pt … h6: 8pt/21pt) and 1.12em paragraph
// margins. The engine mirrors the fixed sizes/margins. It does NOT mirror
// Prince's first-in-flow top-margin truncation: css-break-3 §3.1 preserves a
// fresh box's top margin at the document start and after a forced break, and
// Chromium (the WPT oracle) does the same (page-left-right-001/002,
// page-box-006). The tests below pin the fixed-pt sizes independent of body
// font-size, the preserved first-box margin, the 1.12em paragraph margin,
// and the h6 21pt margin mid-page.
#[test]
fn ua_heading_margin_preserved_at_document_start() {
    // Unstyled h1 at the top of page 1: the 16pt UA margin is PRESERVED — the
    // document start is not a fragmentation break, so css-break-3 does not
    // truncate it (Chromium/WPT; page-left-right-001). Baseline = content top
    // (36) + 16pt margin + ascent (24×1854/2048) + half-leading of the 24pt
    // line at lh 1.2 → 74.72. The old Prince-matching behavior truncated the
    // margin to flush the heading at 58.72.
    let html = "<html><head></head><body><h1>Heading</h1></body></html>";
    let geo = geometry(5.0, 3.0, 0.5);
    let layout = lay(html, geo);
    let (_, y) = text_pos(&layout.pages[0], "Heading").expect("text must be present");
    let expected = 36.0 + 16.0 + 24.0 * 1854.0 / 2048.0
        + (24.0 * 1.2 - 24.0 * 1854.0 / 2048.0 - 24.0 * 434.0 / 2048.0) * 0.5;
    assert!(
        (y - expected).abs() < 0.1,
        "UA h1 top margin must be preserved at document start: baseline y={y:.3} want {expected:.3} (truncated was ~{:.3})",
        expected - 16.0
    );
}

#[test]
fn ua_heading_font_sizes_are_fixed_pt() {
    // Prince's h1 is 24pt REGARDLESS of body font-size; the HTML4 2em value
    // would scale to 16pt at body 8pt. Baseline = content top (36) + 16pt
    // margin (preserved at document start) + 24pt line box → 74.72.
    let html = "<html><head><style>body { font-size: 8pt; }</style></head><body><h1>Heading</h1></body></html>";
    let geo = geometry(5.0, 3.0, 0.5);
    let layout = lay(html, geo);
    let (_, y) = text_pos(&layout.pages[0], "Heading").expect("text must be present");
    let expected = 36.0 + 16.0 + 24.0 * 1854.0 / 2048.0
        + (24.0 * 1.2 - 24.0 * 1854.0 / 2048.0 - 24.0 * 434.0 / 2048.0) * 0.5;
    assert!(
        (y - expected).abs() < 0.1,
        "UA h1 font-size must be fixed 24pt, not 2em of body: baseline y={y:.3} want {expected:.3}"
    );
}

#[test]
fn ua_paragraph_margin_is_1_12em_mid_page() {
    // A second, mid-page paragraph keeps the UA 1.12em top margin (12pt font
    // → 13.44pt): baseline = first line box bottom (50.4) + 13.44 + ascent +
    // half-leading → 75.70. The old 1em default gave 74.26; Prince uses
    // 1.12em.
    let html = r#"<html><head><style>.z { margin: 0; }</style></head>
        <body><p class="z">First.</p><p>Second.</p></body></html>"#;
    let geo = geometry(5.0, 3.0, 0.5);
    let layout = lay(html, geo);
    let (_, y) = text_pos(&layout.pages[0], "Second").expect("text must be present");
    let expected = 36.0 + 14.4 + 12.0 * 1.12 + 12.0 * 1854.0 / 2048.0
        + (14.4 - 12.0 * 1854.0 / 2048.0 - 12.0 * 434.0 / 2048.0) * 0.5;
    assert!(
        (y - expected).abs() < 0.1,
        "UA p margin must be 1.12em mid-page: baseline y={y:.3} want {expected:.3}"
    );
}

#[test]
fn ua_h6_margin_is_21pt_mid_page() {
    // Prince's h6 margin is a FIXED 21pt (the HTML4 2.33em at 8pt font is
    // 18.64pt). Mid-page it must apply in full: baseline = first line box
    // bottom (50.4) + 21 + 8pt ascent + half-leading → 78.97.
    let html = r#"<html><head><style>.z { margin: 0; }</style></head>
        <body><p class="z">First.</p><h6>Six.</h6></body></html>"#;
    let geo = geometry(5.0, 3.0, 0.5);
    let layout = lay(html, geo);
    let (_, y) = text_pos(&layout.pages[0], "Six").expect("text must be present");
    let expected = 36.0 + 14.4 + 21.0 + 8.0 * 1854.0 / 2048.0
        + (8.0 * 1.2 - 8.0 * 1854.0 / 2048.0 - 8.0 * 434.0 / 2048.0) * 0.5;
    assert!(
        (y - expected).abs() < 0.1,
        "UA h6 margin must be 21pt mid-page: baseline y={y:.3} want {expected:.3}"
    );
}

// --- 15. Cross-references: target-counter / target-text (CORE-129) ----------

#[test]
fn target_counter_pages_resolves_target_page_number() {
    // The classic TOC: entries reference chapters via href anchors, and each
    // entry's `content` ends with target-counter(attr(href), page). The
    // number must be the 1-based page the TARGET lands on (chapter 1 →
    // page 2, chapter 2 → page 3 at this geometry).
    let html = r##"<html><head><style>
        @page { margin: 0.4in; }
        body { margin: 0; }
        .toc a { display: block; }
        .e1 { content: "Chapter 1 " target-counter(attr(href), page); }
        .e2 { content: "Chapter 2 " target-counter(attr(href), page); }
        .ch { display: block; break-before: page; }
        h1 { font-size: 14px; }
        p { font-size: 12px; }
    </style></head><body>
        <div class="toc">
            <a class="e1" href="#ch1">Chapter 1</a>
            <a class="e2" href="#ch2">Chapter 2</a>
        </div>
        <div class="ch"><h1 id="ch1">One</h1><p>Body one.</p></div>
        <div class="ch"><h1 id="ch2">Two</h1><p>Body two.</p></div>
    </body></html>"##;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let toc = page_texts(&layout.pages[0]);
    let e1 = toc.iter().find(|t| t.contains("Chapter 1")).expect("entry 1");
    let e2 = toc.iter().find(|t| t.contains("Chapter 2")).expect("entry 2");
    assert!(e1.ends_with('2'), "entry 1 page number: {e1:?}");
    assert!(e2.ends_with('3'), "entry 2 page number: {e2:?}");
}

#[test]
fn target_counter_missing_target_renders_question_mark() {
    // A href with no matching id must degrade to `?` (spec edge case), never
    // panic or render a bogus number.
    let html = r##"<html><head><style>
        @page { margin: 0.4in; }
        body { margin: 0; }
        .e1 { display: block; content: "Ghost " target-counter(attr(href), page); }
    </style></head><body>
        <a class="e1" href="#nope">Ghost</a>
    </body></html>"##;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let texts = page_texts(&layout.pages[0]);
    let e1 = texts.iter().find(|t| t.contains("Ghost")).expect("entry");
    assert!(
        e1.trim_end().ends_with('?'),
        "missing target must resolve to '?': {e1:?}"
    );
}

#[test]
fn target_counter_named_counter_reads_document_counter_state() {
    // `target-counter(attr(href), section)` with `counter-reset: section N` /
    // `counter-increment: section` in the document: the TOC entry for
    // chapter 2 must read the chapter-2 target's counter value (2), not the
    // entry element's own value (0).
    let html = r##"<html><head><style>
        @page { margin: 0.4in; }
        body { margin: 0; }
        .toc a { display: block; }
        .e1 { content: "Chapter 1 " target-counter(attr(href), section); }
        .e2 { content: "Chapter 2 " target-counter(attr(href), section); }
        .ch { display: block; break-before: page; counter-increment: section; }
        .ch1 { counter-reset: section 0; }
        h1 { font-size: 14px; }
        p { font-size: 12px; }
    </style></head><body>
        <div class="toc">
            <a class="e1" href="#ch1">Chapter 1</a>
            <a class="e2" href="#ch2">Chapter 2</a>
        </div>
        <div class="ch ch1"><h1 id="ch1">One</h1><p>Body one.</p></div>
        <div class="ch"><h1 id="ch2">Two</h1><p>Body two.</p></div>
    </body></html>"##;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let toc = page_texts(&layout.pages[0]);
    let e1 = toc.iter().find(|t| t.contains("Chapter 1")).expect("entry 1");
    let e2 = toc.iter().find(|t| t.contains("Chapter 2")).expect("entry 2");
    assert!(
        e1.trim_end().ends_with('1'),
        "chapter 1 section counter: {e1:?}"
    );
    assert!(
        e2.trim_end().ends_with('2'),
        "chapter 2 section counter must read the TARGET's snapshot: {e2:?}"
    );
}

#[test]
fn target_counter_pages_includes_total_pages_counter() {
    // `target-counter(attr(href), pages)` resolves to the DOCUMENT's total
    // page count (the same value `counter(pages)` shows on any page).
    let html = r##"<html><head><style>
        @page { margin: 0.4in; }
        body { margin: 0; }
        .toc a { display: block; }
        .e1 { content: "Chapter 1 " target-counter(attr(href), pages); }
        .ch { display: block; break-before: page; }
        p { font-size: 12px; }
    </style></head><body>
        <div class="toc"><a class="e1" href="#ch1">Chapter 1</a></div>
        <div class="ch" id="ch1"><p>Body one.</p></div>
        <div class="ch" id="ch2"><p>Body two.</p></div>
    </body></html>"##;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let total = layout.pages.len();
    assert!(total >= 2, "fixture must be multi-page");
    let toc = page_texts(&layout.pages[0]);
    let e1 = toc.iter().find(|t| t.contains("Chapter 1")).expect("entry");
    assert!(
        e1.trim_end().ends_with(total.to_string().as_str()),
        "pages counter must equal total {total}: {e1:?}"
    );
}

#[test]
fn target_text_resolves_target_element_text() {
    // `target-text(attr(href))` resolves to the target element's text
    // content — the classic "see §X: <title>" cross-reference form.
    let html = r##"<html><head><style>
        @page { margin: 0.4in; }
        body { margin: 0; }
        .e1 { content: "Ref: " target-text(attr(href)); }
        .ch { display: block; break-before: page; }
        h1 { font-size: 14px; }
        p { font-size: 12px; }
    </style></head><body>
        <div class="ch"><p class="e1" href="#sec1">Ref:</p></div>
        <div class="ch"><h1 id="sec1">Foundations of Typesetting</h1></div>
    </body></html>"##;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let texts = page_texts(&layout.pages[0]);
    let e1 = texts.iter().find(|t| t.contains("Ref:")).expect("entry");
    assert!(
        e1.contains("Foundations of Typesetting"),
        "target-text must carry the target's text: {e1:?}"
    );
}

#[test]
fn target_text_missing_target_renders_question_mark() {
    let html = r##"<html><head><style>
        @page { margin: 0.4in; }
        body { margin: 0; }
        .e1 { content: "Ref: " target-text(attr(href)); }
    </style></head><body>
        <p class="e1" href="#ghost">Ref:</p>
    </body></html>"##;
    let layout = lay(html, geometry(5.0, 3.0, 0.4));
    let texts = page_texts(&layout.pages[0]);
    let e1 = texts.iter().find(|t| t.contains("Ref:")).expect("entry");
    assert!(
        e1.trim_end().ends_with('?'),
        "missing target-text must resolve to '?': {e1:?}"
    );
}

// --- CORE-143 slice (d) regressions -----------------------------------------

/// Empty sibling divs that declare `page:` (no text content) still force the
/// page-change boundary (pseudo-first-margin-001..004). The target-side scan
/// used to skip a leaf-less block as a "contentless wrapper", so the pair
/// `page:a` div + unnamed div stayed on ONE page.
#[test]
fn page_change_break_between_empty_page_declaring_divs() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        @page :first { margin: 1in; }
        div { width: 1in; height: 1in; border: 2px solid red; box-sizing: border-box; }
    </style></head><body>
        <div style="page: a; border-color: lightblue"></div>
        <div style="page: b; border-color: pink"></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        2,
        "an empty div with page:a followed by one with page:b must break"
    );
}

/// A sibling with NO page declaration still demands the boundary when its
/// effective page differs from the preceding declared context
/// (pseudo-first-margin-003: `page:a` div then a plain div = default page).
#[test]
fn page_change_break_to_undeclared_sibling_page() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        @page a { margin: 1in; }
        div { width: 1in; height: 1in; border: 2px solid red; box-sizing: border-box; }
    </style></head><body>
        <div style="page: a; border-color: lightblue"></div>
        <div style="border-color: pink"></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        2,
        "page:a div followed by an undeclared div (default page) must break"
    );
}

/// CORE-152 addendum: with default `box-sizing: content-box`, a 1in div + 2px
/// borders inside an exactly 1in page content box (3in page − 1in top + 1in
/// bottom margins) overflows by the borders, so the declared extent
/// continues onto a second page BEFORE the page:a→b boundary — three pages
/// total. Independently measured with playwright Chromium (page.pdf 5x3in,
/// 0.5in margins, prefer_css_page_size): 3 pages. The old engine suppressed
/// the continuation (2 pages, background overflow past the fragmentainer);
/// the corrected height-continuation model (css-break-3 box splitting)
/// yields the extra fragment. The two tests above use border-box precisely
/// to keep their page:a→b invariant free of this overflow.
#[test]
fn content_box_border_overflow_continues_extent() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        @page :first { margin: 1in; }
        div { width: 1in; height: 1in; border: 2px solid red; }
    </style></head><body>
        <div style="page: a; border-color: lightblue"></div>
        <div style="page: b; border-color: pink"></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        3,
        "content-box 1in+4px border box in a 1in content height continues before the page:a→b boundary"
    );
}

/// `@page a:first` (name and pseudo joined without whitespace) parses as the
/// named page `a` with the :first pseudo (pseudo-first-margin-002). The old
/// whitespace tokenizer turned `a:first` into a page NAME.
#[test]
fn named_page_pseudo_without_whitespace_parses() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        @page a:first { margin: 2in; }
        div { width: 1in; height: 1in; border: 2px solid red; }
    </style></head><body>
        <div style="page: a; border-color: lightblue"></div>
        <div style="page: b; border-color: pink"></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(layout.pages.len(), 2, "a:first then b must break");
    // Page 1 carries the 2in (:first on page a) margin: the 1in box's border
    // starts at 2in + 0 = 144pt from the page top edge... assert via the box
    // top: first page's first child y == margin 2in = 144pt.
    let first = &layout.pages[0].root.children[0];
    assert!(
        (first.offset.y.get() - 144.0).abs() < 1.0,
        "page 1 uses @page a:first margin (2in=144pt), got {}",
        first.offset.y.get()
    );
}

/// Cascade layers order @page rules: later-declared layer beats an earlier
/// one regardless of source position; unlayered beats all (layers-001..004).
#[test]
fn cascade_layers_order_page_margins() {
    // layer2 declared AFTER layer1 → layer2 wins, despite source position.
    let html = r#"<html><head><style>
        @layer layer1, layer2;
        @layer layer1 { @page { margin: 1in; } }
        @layer layer2 { @page { margin: 0; } }
        div { width: 1in; height: 1in; border: 2px solid red; }
    </style></head><body>
        <div></div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let first = &layout.pages[0].root.children[0];
    assert!(
        first.offset.y.get() < 2.0,
        "layer2 (margin 0) must beat layer1 (margin 1in) even though layer1's \
         rule appears later in source; got top {}",
        first.offset.y.get()
    );

    // Swapped statement order: layer1 wins now.
    let html2 = r#"<html><head><style>
        @layer layer2, layer1;
        @layer layer1 { @page { margin: 1in; } }
        @layer layer2 { @page { margin: 0; } }
        div { width: 1in; height: 1in; border: 2px solid red; }
    </style></head><body>
        <div></div>
    </body></html>"#;
    let layout2 = lay(html2, geometry(5.0, 3.0, 0.5));
    let first2 = &layout2.pages[0].root.children[0];
    assert!(
        (first2.offset.y.get() - 72.0).abs() < 2.0,
        "layer1 (margin 1in) must beat layer2 (margin 0) after the order \
         swap; got top {}",
        first2.offset.y.get()
    );

    // Unlayered beats every layer.
    let html3 = r#"<html><head><style>
        @layer l1 { @page { margin: 0; } }
        @page { margin: 2in; }
        div { width: 1in; height: 1in; border: 2px solid red; }
    </style></head><body>
        <div></div>
    </body></html>"#;
    let layout3 = lay(html3, geometry(5.0, 3.0, 0.5));
    let first3 = &layout3.pages[0].root.children[0];
    assert!(
        (first3.offset.y.get() - 144.0).abs() < 2.0,
        "unlayered @page (margin 2in) must beat the layered one; got top {}",
        first3.offset.y.get()
    );
}

/// `html { display: none }` suppresses the whole document: exactly one blank
/// page with no page-box chrome (root-element-display-none, css-page-3 —
/// the root element generates no boxes, so the page is empty; the blank ref
/// compares equal). The suppression check must read the HTML ELEMENT's
/// computed display, not the synthetic document node's (which is never
/// element-styled).
#[test]
fn root_display_none_produces_one_blank_page_without_chrome() {
    let html = r#"<html><head><style>
        @page { margin: 0.5in; border: solid red; background: hotpink; }
        html { display: none; }
    </style></head><body>FAIL, this page should be blank.</body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        1,
        "display:none on the html element must yield exactly one page"
    );
    let page = &layout.pages[0];
    assert!(
        page.root.children.is_empty(),
        "a display:none root must place no content boxes"
    );
    assert!(
        page.background.is_none(),
        "the suppressed-document page must carry no @page chrome background"
    );
    assert!(
        page.canvas_background.is_none(),
        "the suppressed-document page must carry no canvas background"
    );
    assert!(
        !page_texts(page).iter().any(|t| t.contains("FAIL")),
        "root content must not render when html is display:none"
    );
}

/// A `display: none` child must never fold its text into the parent's flow:
/// the parent then carries no content (page-name-display-none-child — the
/// page:c div holding only a display:none child stays empty, and the sibling
/// page-change break still fires around it). Without the guard the child's
/// text leaks onto the empty page.
#[test]
fn display_none_child_does_not_leak_text_into_parent_flow() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        @page a { margin: 1in; }
        @page c { margin: 1in; }
        div { width: 1in; height: 1in; }
    </style></head><body>
        <div style="page: a">a</div>
        <div style="page: c">
            <div style="display: none">c</div>
        </div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    // The display:none child is not content: the page:c wrapper is empty and
    // must not render its hidden child's text anywhere.
    for (i, page) in layout.pages.iter().enumerate() {
        assert!(
            !page_texts(page).iter().any(|t| t.contains('c')),
            "page {} must not contain the display:none child's text",
            i + 1
        );
    }
}

/// An inline replaced element (`<img>` without `display: block`) is not a
/// class-A box: its own `page` declaration does not create a page-change
/// boundary (css-page-3 §8.1 "Applies to: boxes that create class A break
/// points"). The break fires only where a real block sibling's context
/// differs (page-name-img-001: inline img `page:b` under body `page:a` stays
/// on page a; the following `page:b` div starts page b).
#[test]
fn inline_img_page_declaration_is_inert_for_boundaries() {
    // A tiny real PNG (1x1) so the image store has something to intern.
    let img = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let html = format!(
        r#"<html><head><style>
        @page {{ margin: 0; }}
        @page a {{ margin: 1in; }}
    </style></head><body style="page: a">
        <img style="page: b" src="{img}">
        <div style="page: b">b</div>
    </body></html>"#
    );
    let layout = lay(&html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        2,
        "inline img page:b under body page:a, then a page:b div: the div's \
         page differs from the img's inherited context (a), so a break must \
         fire between them (img-001)"
    );
}

/// Mirror of the above with the inline img AFTER a page-declaring block
/// (page-name-img-002: `page:b` div then inline img `page:b` — the div ends
/// page b, the img's context is the ancestor's (a), so a break fires between
/// them and the img lands on a new page).
#[test]
fn inline_img_page_declaration_is_inert_after_declaring_block() {
    let img = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let html = format!(
        r#"<html><head><style>
        @page {{ margin: 0; }}
        @page a {{ margin: 1in; }}
    </style></head><body style="page: a">
        <div style="page: b">a</div>
        <img style="page: b" src="{img}">
    </body></html>"#
    );
    let layout = lay(&html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        2,
        "page:b div then inline img page:b under body page:a: the div's end \
         context (b) differs from the img's inherited context (a), so a break \
         fires before the img (img-002)"
    );
}

// --- CORE-157-b residuals --------------------------------------------------

/// A page-declaring wrapper whose ONLY child is `display: none` is still a
/// page-context box: the sibling page-change break fires around it, so the
/// following page-declaring sibling starts a fresh page (page-name-display-
/// none-child — test renders 3 pages like its ref and Chromium). The
/// `!res.empty` guard must not skip the comparison for a genuinely contentless
/// box; only a resume-empty wrapper (whose subtree still holds content,
/// and-break-003) may skip.
#[test]
fn display_none_child_wrapper_keeps_page_change_break() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        @page a { margin: 1in; }
        @page b { margin: 1in; }
        @page c { margin: 1in; }
    </style></head><body>
        <div style="page: a">a</div>
        <div style="page: c">
            <div style="display: none">c</div>
        </div>
        <div style="page: b">b</div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        3,
        "contentless page:c wrapper must still host a page-change boundary: \
         a|c|b on three pages (display-none-child)"
    );
    // The hidden child's text must never leak onto any page.
    for (i, page) in layout.pages.iter().enumerate() {
        assert!(
            !page_texts(page).iter().any(|t| t.contains('c')),
            "page {} must not contain the display:none child's text",
            i + 1
        );
    }
}

/// An inline-block's OWN `page` declaration is inert (css-page-3 §8.1 class-A
/// applicability), and its interior page-changes do not propagate (atomic
/// interior — spec Non-Goal 3). A following in-flow `page:c` BLOCK sibling is
/// class A and its context change from the inline-block's inherited context
/// forces the break (page-name-inline-block-002: inline-block `page:c` with
/// interior a/b + following `page:c` div = 2 pages; the div's `page:c` is
/// inert against the default page, so the break is demanded by the div).
#[test]
fn inline_block_page_declaration_is_inert_following_block_breaks() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        @page a { margin: 1in; }
        @page b { margin: 1in; }
        @page c { margin: 1in; }
        div { width: 1in; height: 1in; }
    </style></head><body>
        <div style="page: c; display: inline-block">
            <div style="page: a">a</div>
            <div style="page: b">b</div>
        </div>
        <div style="page: c">c</div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        2,
        "following page:c div after an inert page:c inline-block must break \
         (inline-block-002): 2 pages"
    );
    let p1 = page_texts(&layout.pages[0]).concat();
    assert!(p1.contains('a') && p1.contains('b'), "page 1 holds ab");
    assert!(
        page_texts(&layout.pages[1]).concat().contains('c'),
        "page 2 holds the following div's c"
    );
}
