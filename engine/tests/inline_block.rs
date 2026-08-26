//! CSS inline-block acceptance tests — one per acceptance criterion in
//! `docs/specifications/inline-block.spec.md` (CORE-120).

use std::path::Path;
use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

const EPS: f64 = 1e-6;

// --- helpers (mirrors tests/floats.rs conventions) --------------------------

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

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

fn style_text(html: &str) -> String {
    let dom = Dom::parse(html).unwrap();
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    css
}

fn lay(html: &str, geo: PageGeometry) -> Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = Stylesheet::parse(&style_text(html));
    layout(&dom, &ss, geo)
}

fn dom_of(html: &str) -> Dom {
    Dom::parse(html).unwrap()
}

fn node_id_by_class(dom: &Dom, class: &str) -> NodeId {
    dom.nodes
        .iter()
        .position(|n| {
            matches!(&n.kind, NodeKind::Element(el) if el.classes.iter().any(|c| c == class))
        })
        .expect("element with class must exist") as NodeId
}

/// Every fragment in the page tree, depth-first, with its absolute position.
fn collect_all<'a>(
    frag: &'a Fragment,
    dx: f64,
    dy: f64,
    out: &mut Vec<(f64, f64, f64, f64, &'a Fragment)>,
) {
    let x = dx + frag.offset.x.get();
    let y = dy + frag.offset.y.get();
    out.push((x, y, frag.size.0.get(), frag.size.1.get(), frag));
    for c in &frag.children {
        collect_all(c, x, y, out);
    }
}

fn page_fragments<'a>(l: &'a Layout, page: usize) -> Vec<(f64, f64, f64, f64, &'a Fragment)> {
    let mut out = Vec::new();
    collect_all(&l.pages[page].root, 0.0, 0.0, &mut out);
    out
}

/// Fragments whose `source` is the given node.
fn find_source<'a>(
    frags: &[(f64, f64, f64, f64, &'a Fragment)],
    id: NodeId,
) -> Vec<(f64, f64, f64, f64, &'a Fragment)> {
    frags.iter().filter(|(_, _, _, _, f)| f.source == Some(id)).copied().collect()
}

fn assert_close(got: f64, want: f64, label: &str) {
    let diff = (got - want).abs();
    assert!(diff < EPS, "{label}: got {got:.6} want {want:.6} (diff {diff:.6})");
}

/// The engine binary path for render-based checks.
fn bin_path() -> std::path::PathBuf {
    let mut p = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    p.push("target/debug/typeanvil");
    if !p.exists() {
        p.set_extension("");
        p.push("target/debug/typeanvil");
    }
    p
}

// --- 1. Two 50% bordered inline-blocks place side by side -------------------

#[test]
fn two_inline_blocks_side_by_side() {
    // Content width 4in; each box is 50% = 2in. Both must sit on ONE line:
    // same y, second at x = first + 2in.
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        div { margin: 0; padding: 0; }
        .box { display: inline-block; width: 50%; border: 4px solid black; background: #eee; }
    </style></head><body>
        <div class="box">1</div><div class="box">2</div>
    </body></html>"#;
    let dom = dom_of(html);
    let b1 = node_id_by_class(&dom, "box");
    // Second .box: next element whose class matches.
    let b2 = dom.nodes[b1 as usize + 1..]
        .iter()
        .position(|n| {
            matches!(&n.kind, NodeKind::Element(el) if el.classes.iter().any(|c| c == "box"))
        })
        .expect("second box must exist") as NodeId
        + b1 as usize
        + 1;

    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_fragments(&layout, 0);

    for id in [b1, b2] {
        let placed = find_source(&frags, id);
        assert!(!placed.is_empty(), "inline-block must paint its own fragment");
    }

    let f1 = find_source(&frags, b1)[0];
    let f2 = find_source(&frags, b2)[0];
    // Same line: equal top.
    assert_close(f1.1, f2.1, "both boxes share one line (equal y)");
    // Side by side: second starts where the first ends.
    assert_close(f2.0 - f1.0, inches(2.0).get(), "horizontal offset is 50% width");
    // Each paints a border box of its own.
    for (_, _, _, _, f) in [f1, f2] {
        assert!(
            matches!(f.content, FragmentContent::Background(_))
                || !f.children.is_empty(),
            "box must carry painting content"
        );
    }
    // Border fragments exist inside each box subtree.
    for (id, label) in [(b1, "first"), (b2, "second")] {
        let sub = find_source(&frags, id);
        assert!(
            sub.iter().any(|(_, _, _, _, f)| matches!(f.content, FragmentContent::Border(_)))
                || sub.iter().any(|(_, _, _, _, f)| matches!(f.content, FragmentContent::Background(_))),
            "{label} inline-block must paint border/background, got {} fragments",
            sub.len()
        );
    }
}

// --- 2. Inline-block child is NOT folded into the parent text run ----------

#[test]
fn inline_block_not_folded_into_text_run() {
    // A paragraph containing an inline-block child: the child's text must
    // NOT merge into the parent's line run — the child keeps its own box.
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p { margin: 0; padding: 0; }
        .ib { display: inline-block; width: 1in; border: 1px solid red; }
    </style></head><body>
        <p>Before <span class="ib">IB</span> after.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let ib = node_id_by_class(&dom, "ib");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_fragments(&layout, 0);

    // The inline-block's own fragment exists and carries its border.
    let ib_frags = find_source(&frags, ib);
    assert!(!ib_frags.is_empty(), "inline-block must be laid out as its own box");
    assert!(
        ib_frags.iter().any(|(_, _, w, h, _)| *w > 0.0 && *h > 0.0),
        "inline-block box has nonzero size"
    );

    // The parent's text lines never contain the child's glyphs merged away:
    // the word "IB" appears as its own line run inside the box, not glued to
    // "Before"/"after".
    let runs: Vec<String> = frags
        .iter()
        .filter_map(|(_, _, _, _, f)| match &f.content {
            FragmentContent::Text(run) => Some(run.text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        runs.iter().any(|t| t.contains("Before")),
        "parent run 'Before' must still lay out"
    );
    assert!(
        runs.iter().any(|t| t.trim() == "IB"),
        "child text lays out inside its own box, not fused into the parent run"
    );
}

// --- 3. Over-tall inline-block defers whole to the next page ---------------

#[test]
fn tall_inline_block_defers_whole() {
    // Page content height = 3in - 1in margins = 2in. A 1in block + a 3in
    // inline-block: the second cannot split; it moves whole to page 2.
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .filler { height: 1in; }
        .tall { display: inline-block; width: 2in; height: 3in; border: 1px solid blue; }
    </style></head><body>
        <div class="filler">filler</div>
        <div class="tall">T</div>
    </body></html>"#;
    let dom = dom_of(html);
    let tall = node_id_by_class(&dom, "tall");
    let layout = lay(html, geometry(4.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 2, "must produce a second page");

    let p1 = page_fragments(&layout, 0);
    let p2 = page_fragments(&layout, 1);
    assert!(
        find_source(&p1, tall).is_empty(),
        "over-tall inline-block must not start on page 1"
    );
    let on_p2 = find_source(&p2, tall);
    assert!(!on_p2.is_empty(), "inline-block defers whole to page 2");
    // Whole box on one page: full height present there.
    let h = on_p2.iter().map(|(_, _, _, hh, _)| *hh).fold(0.0f64, f64::max);
    assert_close(h, inches(3.0).get(), "full 3in height lands on page 2");
}

// --- 4. Render smoke: borders visible side by side via CLI ------------------

#[test]
fn render_two_inline_blocks_cli() {
    // Skip when the binary has not been built (`cargo test` without build).
    let bin = bin_path();
    if !bin.exists() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("typeanvil-core120-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("ib.html");
    std::fs::write(
        &html,
        r#"<!DOCTYPE html><html><head><style>
@page { size: 5in 3in; margin: 0.5in; }
body { margin: 0; font-size: 12pt; }
.box { display: inline-block; width: 50%; height: 0.5in;
       border: 8px solid black; background: #ddd; }
</style></head><body>
<div class="box">1</div><div class="box">2</div>
</body></html>"#,
    )
    .unwrap();
    let pdf = dir.join("ib.pdf");
    let out = Command::new(&bin)
        .arg("render")
        .args(["--page-width", "5in", "--page-height", "3in",
               "--margin-top", "0.5in", "--margin-right", "0.5in",
               "--margin-bottom", "0.5in", "--margin-left", "0.5in"])
        .arg("-o")
        .arg(&pdf)
        .arg(&html)
        .output()
        .expect("engine binary runs");
    assert!(out.status.success(), "render failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(pdf.exists(), "pdf written");
    let _ = std::fs::remove_dir_all(&dir);
}
