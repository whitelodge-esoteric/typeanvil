//! CORE-552 (writing-modes-layout.spec.md Behaviors 16-19, 21-23): logical
//! sizes and edges land on the physical axes the mode in effect names.
//!
//! These are direct assertions on the cascaded physical `ComputedStyle`
//! fields and on laid-out fragment sizes. The WPT gate cannot see this class
//! of change because test and reference render through the same engine.

use typeanvil::css::{cascade, ComputedStyle, PageWritingMode, Stylesheet};
use typeanvil::dom::{Dom, NodeId};
use typeanvil::frag::Fragment;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    // 600pt x 400pt page, no margins: a round content box.
    PageGeometry {
        width: Scalar(600.0),
        height: Scalar(400.0),
        margin_top: Scalar(0.0),
        margin_right: Scalar(0.0),
        margin_bottom: Scalar(0.0),
        margin_left: Scalar(0.0),
    }
}

fn styles_of(html: &str, css: &str) -> (Dom, Vec<ComputedStyle>) {
    let dom = Dom::parse(html).expect("parse");
    let sheet = Stylesheet::parse(css);
    let styles = cascade(&dom, &sheet, &geo());
    (dom, styles)
}

fn by_id(dom: &Dom, id: &str) -> NodeId {
    dom.nodes
        .iter()
        .position(|n| n.kind.element().is_some_and(|e| e.id.as_deref() == Some(id)))
        .unwrap_or_else(|| panic!("no #{id}"))
}

fn close(a: Scalar, b: f64) -> bool {
    (a.get() - b).abs() < 0.01
}

fn doc(wm: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html><html style=\"writing-mode:{wm}\"><body style=\"margin:0\">{body}</body></html>"
    )
}

const LOGICAL: &str = "inline-size:100px;block-size:200px;\
    margin-inline-start:8px;margin-inline-end:16px;margin-block-start:24px;margin-block-end:32px;\
    padding-inline-start:4px;padding-inline-end:12px;padding-block-start:20px;padding-block-end:28px;";

/// Behavior 18: `inline-size`/`block-size` land on height/width in every
/// vertical mode and on width/height in horizontal-tb. 100px = 75pt,
/// 200px = 150pt.
#[test]
fn logical_sizes_land_on_named_axes() {
    for (wm, vertical) in [
        ("horizontal-tb", false),
        ("vertical-rl", true),
        ("vertical-lr", true),
        ("sideways-rl", true),
        ("sideways-lr", true),
    ] {
        let (dom, st) = styles_of(
            &doc(wm, &format!("<div id=d style=\"{LOGICAL}\"></div>")),
            "",
        );
        let s = &st[by_id(&dom, "d")];
        let (w, h) = if vertical { (150.0, 75.0) } else { (75.0, 150.0) };
        assert!(s.width.is_some_and(|v| close(v, w)), "{wm}: width {:?} want {w}", s.width);
        assert!(s.height.is_some_and(|v| close(v, h)), "{wm}: height {:?} want {h}", s.height);
    }
}

/// Behavior 16: logical margins map to the physical edges named in the
/// table, per mode and direction. horizontal-tb maps all four; the vertical
/// modes map the INLINE pair here (inline-start 6pt, inline-end 12pt). The
/// block pair in a vertical mode is covered by
/// `block_margins_in_vertical_modes_map_to_block_edges`.
#[test]
fn logical_edges_map_per_mode() {
    let (is, ie, bs, be) = (6.0, 12.0, 18.0, 24.0);
    let inline = "margin-inline-start:8px;margin-inline-end:16px;";
    let all = "margin-inline-start:8px;margin-inline-end:16px;margin-block-start:24px;margin-block-end:32px;";
    // (mode, rtl, margin css, [top, right, bottom, left])
    let cases: &[(&str, bool, &str, [f64; 4])] = &[
        ("horizontal-tb", false, all, [bs, ie, be, is]),
        ("horizontal-tb", true, all, [bs, is, be, ie]),
        ("vertical-rl", false, inline, [is, 0.0, ie, 0.0]),
        ("vertical-rl", true, inline, [ie, 0.0, is, 0.0]),
        ("vertical-lr", false, inline, [is, 0.0, ie, 0.0]),
        ("sideways-rl", false, inline, [is, 0.0, ie, 0.0]),
        ("sideways-lr", false, inline, [ie, 0.0, is, 0.0]),
    ];
    for (wm, rtl, css, want) in cases {
        let dir = if *rtl { "direction:rtl;" } else { "" };
        let (dom, st) = styles_of(
            &doc(wm, &format!("<div id=d style=\"{dir}{css}\"></div>")),
            "",
        );
        let s = &st[by_id(&dom, "d")];
        let got = [s.margin_top, s.margin_right, s.margin_bottom, s.margin_left];
        for (i, side) in ["top", "right", "bottom", "left"].iter().enumerate() {
            assert!(close(got[i], want[i]), "{wm} rtl={rtl}: margin-{side} {} want {}", got[i].get(), want[i]);
        }
    }
}

/// CORE-553: `margin-block-start/end` map to the physical BLOCK edges:
/// right/left for vertical-rl and sideways-rl, left/right for vertical-lr and
/// sideways-lr (block-start 18pt, block-end 24pt).
#[test]
fn block_margins_in_vertical_modes_map_to_block_edges() {
    // (mode, [top, right, bottom, left])
    let cases: &[(&str, [f64; 4])] = &[
        ("vertical-rl", [0.0, 18.0, 0.0, 24.0]),
        ("sideways-rl", [0.0, 18.0, 0.0, 24.0]),
        ("vertical-lr", [0.0, 24.0, 0.0, 18.0]),
        ("sideways-lr", [0.0, 24.0, 0.0, 18.0]),
    ];
    for (wm, want) in cases {
        let (dom, st) = styles_of(
            &doc(
                wm,
                "<div id=d style=\"margin-block-start:24px;margin-block-end:32px\"></div>",
            ),
            "",
        );
        let s = &st[by_id(&dom, "d")];
        let got = [s.margin_top, s.margin_right, s.margin_bottom, s.margin_left];
        for (i, side) in ["top", "right", "bottom", "left"].iter().enumerate() {
            assert!(close(got[i], want[i]), "{wm}: margin-{side} {} want {}", got[i].get(), want[i]);
        }
    }
}

/// Collect `(x, y, w, h)` page-absolute rects of every fragment sourced from
/// `id` on one page.
fn rects_of(f: &Fragment, id: NodeId, px: f64, py: f64, out: &mut Vec<(f64, f64, f64, f64)>) {
    let (x, y) = (px + f.offset.x.get(), py + f.offset.y.get());
    if f.source == Some(id) {
        out.push((x, y, f.size.0.get(), f.size.1.get()));
    }
    for c in &f.children {
        rects_of(c, id, x, y, out);
    }
}

/// CORE-553 (Behaviors 11-12, 15): a vertical root paginates along the
/// physical x axis. A box with a block extent of 2.5 page widths fragments
/// into 3 pages; each fragment spans the full page height (its inline axis)
/// and the pages progress from the block-start edge (right for -rl, left for
/// -lr). The loop terminates.
#[test]
fn root_vertical_progression_fragments_along_x() {
    // Page 600x400pt, no margins. Block extent 2.5 * 600 = 1500pt.
    for (wm, rl) in [("vertical-rl", true), ("vertical-lr", false)] {
        let html = doc(wm, "<div id=b style=\"block-size:1500pt;background:red\"></div>");
        let dom = Dom::parse(&html).unwrap();
        let l = layout(&dom, &Stylesheet::parse(""), geo());
        assert_eq!(l.pages.len(), 3, "{wm}: page count");
        let b = by_id(&dom, "b");
        let widths = [600.0, 600.0, 300.0];
        for (pi, page) in l.pages.iter().enumerate() {
            let mut r = Vec::new();
            rects_of(&page.root, b, 0.0, 0.0, &mut r);
            let (x, y, w, h) = *r
                .iter()
                .max_by(|a, b| (a.2 * a.3).partial_cmp(&(b.2 * b.3)).unwrap())
                .unwrap_or_else(|| panic!("{wm}: no fragment of #b on page {pi}"));
            assert!((w - widths[pi]).abs() < 0.5, "{wm} p{pi}: width {w}");
            assert!((h - 400.0).abs() < 0.5, "{wm} p{pi}: height {h}");
            assert!(y.abs() < 0.5, "{wm} p{pi}: y {y}");
            // Block-start edge: right for -rl, left for -lr.
            let want_x = if rl { 600.0 - widths[pi] } else { 0.0 };
            assert!((x - want_x).abs() < 0.5, "{wm} p{pi}: x {x} want {want_x}");
        }
    }
}

/// CORE-553: block-axis margins push content along x in a vertical root,
/// and a huge block-end margin ends the page rather than dropping text
/// (page-name-orthogonal-writing-002 ref shape: 2 pages).
#[test]
fn root_vertical_block_end_margin_breaks_the_page() {
    let html = doc(
        "vertical-rl",
        "<div id=a style=\"margin-block-end:999in\">a</div><div id=b>b</div>",
    );
    let dom = Dom::parse(&html).unwrap();
    let l = layout(&dom, &Stylesheet::parse(""), geo());
    assert_eq!(l.pages.len(), 2, "a then b on the next page");
    let a = by_id(&dom, "a");
    let mut r = Vec::new();
    rects_of(&l.pages[0].root, a, 0.0, 0.0, &mut r);
    let (x, _, w, h) = r[0];
    assert!((x + w - 600.0).abs() < 0.5, "vertical-rl: first box at the right edge, x={x} w={w}");
    assert!((h - 400.0).abs() < 0.5, "auto inline size fills the page height, h={h}");
}

/// Every fragment of `id` on one page, with its page-absolute origin and
/// whether any text run in its subtree carries a rotation.
fn subtree_of(f: &Fragment, id: NodeId, px: f64, py: f64, out: &mut Vec<(f64, f64, f64, f64, bool)>) {
    fn rotated(f: &Fragment) -> bool {
        f.transform.is_some() || f.children.iter().any(rotated)
    }
    let (x, y) = (px + f.offset.x.get(), py + f.offset.y.get());
    if f.source == Some(id) {
        out.push((x, y, f.size.0.get(), f.size.1.get(), rotated(f)));
    }
    for c in &f.children {
        subtree_of(c, id, x, y, out);
    }
}

/// CORE-553 Behavior 28 (AC 8): an `absolute` box whose mode is
/// `horizontal-tb` inside a vertical root lays out in the PHYSICAL frame. Its
/// text is not rotated, its box is wider than tall, and its insets resolve
/// against the physical page content box (`left:20px; top:20px` = 15pt).
#[test]
fn horizontal_abspos_in_vertical_root_is_not_transposed() {
    for wm in ["vertical-rl", "vertical-lr", "sideways-lr"] {
        let html = doc(
            wm,
            "<div style=\"block-size:200px;background:yellow\">vertical body text</div>\
             <div id=h style=\"position:absolute;writing-mode:horizontal-tb;left:20px;top:20px;width:200px;background:lime\">Horizontal abspos text</div>",
        );
        let dom = Dom::parse(&html).unwrap();
        let l = layout(&dom, &Stylesheet::parse(""), geo());
        assert_eq!(l.pages.len(), 1, "{wm}: one page");
        let h = by_id(&dom, "h");
        let mut r = Vec::new();
        subtree_of(&l.pages[0].root, h, 0.0, 0.0, &mut r);
        let (x, y, w, hh, rotated) = *r
            .iter()
            .max_by(|a, b| (a.2 * a.3).partial_cmp(&(b.2 * b.3)).unwrap())
            .unwrap_or_else(|| panic!("{wm}: no fragment of #h"));
        assert!(!rotated, "{wm}: horizontal abspos text must not carry a rotation");
        assert!(w > hh, "{wm}: box wider than tall, w={w} h={hh}");
        assert!((w - 150.0).abs() < 0.5, "{wm}: width 200px = 150pt, got {w}");
        assert!((x - 15.0).abs() < 0.5, "{wm}: left:20px = 15pt, got x={x}");
        assert!((y - 15.0).abs() < 0.5, "{wm}: top:20px = 15pt, got y={y}");
    }
}

/// CORE-553 Behavior 27 (AC 7, footnotes): in a vertical root the footnote
/// band is a PHYSICAL bottom band. Every note line lies inside the page
/// content rect's bottom band, unrotated, and the body does not paint into
/// the band.
#[test]
fn footnotes_in_vertical_root_stay_in_a_physical_bottom_band() {
    use typeanvil::frag::FragPart;
    fn walk(f: &Fragment, px: f64, py: f64, notes: &mut Vec<(f64, f64, f64, f64, bool)>, body_max_y: &mut f64) {
        let (x, y) = (px + f.offset.x.get(), py + f.offset.y.get());
        let (w, h) = (f.size.0.get(), f.size.1.get());
        if f.part == FragPart::FootnoteBody {
            notes.push((x, y, w, h, f.transform.is_some()));
        } else if f.source.is_some() && w > 0.0 && h > 0.0 {
            *body_max_y = body_max_y.max(y + h);
        }
        for c in &f.children {
            walk(c, x, y, notes, body_max_y);
        }
    }
    let html = "<!DOCTYPE html><html style=\"writing-mode:vertical-rl\"><head><style>\
        @page { margin: 30pt; } body { margin: 0 } .fn { float: footnote; }\
        </style></head><body><div style=\"background:yellow\">Body line one<span class=\"fn\">FOOTNOTE text</span></div></body></html>";
    let dom = Dom::parse(html).unwrap();
    let g = PageGeometry {
        width: Scalar(600.0),
        height: Scalar(400.0),
        margin_top: Scalar(30.0),
        margin_right: Scalar(30.0),
        margin_bottom: Scalar(30.0),
        margin_left: Scalar(30.0),
    };
    let l = layout(&dom, &Stylesheet::parse(".fn { float: footnote; }"), g);
    assert_eq!(l.pages.len(), 1);
    let mut notes = Vec::new();
    let mut body_max_y = 0.0;
    walk(&l.pages[0].root, 0.0, 0.0, &mut notes, &mut body_max_y);
    assert!(!notes.is_empty(), "the footnote body must be laid out");
    let content_bottom = 400.0 - 30.0;
    let band_top = notes.iter().map(|n| n.1).fold(f64::MAX, f64::min);
    for (x, y, w, h, rotated) in &notes {
        assert!(!rotated, "note line must not be rotated");
        assert!(*x >= 0.0 && x + w <= 600.0, "note line inside the page width: x={x} w={w}");
        assert!(*y >= 30.0 && y + h <= content_bottom + 0.01, "note line inside the content rect: y={y} h={h}");
        assert!(w > h, "a note line runs horizontally: w={w} h={h}");
    }
    assert!(
        body_max_y <= band_top + 0.01,
        "body paints above the footnote band: body bottom {body_max_y} > band top {band_top}"
    );
}

/// CORE-553 (page-size-012 ref shape): an interior `vertical-rl` block in a
/// horizontal root lays out in its own frame. Its physical width is its
/// content's block extent (not the page width), and `margin-left: auto` with
/// `margin-right: 50pt` puts it 50pt from the right edge.
#[test]
fn interior_vertical_block_sizes_to_content_and_honours_auto_margin() {
    let html = doc(
        "horizontal-tb",
        "<div id=v style=\"writing-mode:vertical-rl;margin-left:auto;margin-right:50pt;height:400pt;background:yellow\">a<br>b</div>",
    );
    let dom = Dom::parse(&html).unwrap();
    let l = layout(&dom, &Stylesheet::parse(""), geo());
    assert_eq!(l.pages.len(), 1);
    let v = by_id(&dom, "v");
    let mut r = Vec::new();
    rects_of(&l.pages[0].root, v, 0.0, 0.0, &mut r);
    let (x, y, w, h) = r[0];
    assert!(w > 0.0 && w < 100.0, "width is two line columns, w={w}");
    assert!((h - 400.0).abs() < 0.5, "declared height is the inline size, h={h}");
    assert!((x + w - 550.0).abs() < 0.5, "right edge 50pt in, x={x} w={w}");
    assert!(y.abs() < 0.5, "y={y}");
}

/// Behavior 16: padding follows the same map as margin (inline-start 3pt,
/// inline-end 9pt, block-start 15pt, block-end 21pt).
#[test]
fn logical_padding_maps_per_mode() {
    let (is, ie, bs, be) = (3.0, 9.0, 15.0, 21.0);
    let cases: &[(&str, [f64; 4])] = &[
        ("horizontal-tb", [bs, ie, be, is]),
        ("vertical-rl", [is, bs, ie, be]),
        ("vertical-lr", [is, be, ie, bs]),
        ("sideways-lr", [ie, be, is, bs]),
    ];
    for (wm, want) in cases {
        let (dom, st) = styles_of(
            &doc(wm, &format!("<div id=d style=\"{LOGICAL}\"></div>")),
            "",
        );
        let s = &st[by_id(&dom, "d")];
        let got = [s.padding_top, s.padding_right, s.padding_bottom, s.padding_left];
        for (i, side) in ["top", "right", "bottom", "left"].iter().enumerate() {
            assert!(close(got[i], want[i]), "{wm}: padding-{side} {} want {}", got[i].get(), want[i]);
        }
    }
}

/// Behavior 16/21: `min-/max-inline-size` and `-block-size` clamp the named
/// physical axis.
#[test]
fn logical_min_max_map_to_physical_axes() {
    let (dom, st) = styles_of(
        &doc(
            "vertical-rl",
            "<div id=d style=\"min-inline-size:40px;max-inline-size:80px;min-block-size:10px;max-block-size:20px\"></div>",
        ),
        "",
    );
    let s = &st[by_id(&dom, "d")];
    let lim = |l: Option<typeanvil::css::SizeLimit>| l.and_then(|l| l.length).unwrap();
    assert!((lim(s.min_height) - 30.0).abs() < 0.01, "min-height {:?}", s.min_height);
    assert!((lim(s.max_height) - 60.0).abs() < 0.01, "max-height {:?}", s.max_height);
    assert!((lim(s.min_width) - 7.5).abs() < 0.01, "min-width {:?}", s.min_width);
    assert!((lim(s.max_width) - 15.0).abs() < 0.01, "max-width {:?}", s.max_width);
}

/// Behavior 17: a PHYSICAL property keeps its physical meaning in any mode.
#[test]
fn physical_properties_stay_physical() {
    let (dom, st) = styles_of(
        &doc(
            "vertical-rl",
            "<div id=d style=\"width:100px;height:200px;margin-left:8px;padding-top:4px\"></div>",
        ),
        "",
    );
    let s = &st[by_id(&dom, "d")];
    assert!(s.width.is_some_and(|v| close(v, 75.0)));
    assert!(s.height.is_some_and(|v| close(v, 150.0)));
    assert!(close(s.margin_left, 6.0));
    assert!(close(s.padding_top, 3.0));
}

/// Behavior 22: viewport units are physical. `block-size: 50vw` in
/// vertical-rl is a WIDTH of half the page width; `inline-size: 50vh` is a
/// HEIGHT of half the page height. Both resolve at the layout seam.
#[test]
fn viewport_units_stay_physical() {
    let html = doc(
        "vertical-rl",
        "<div id=d style=\"inline-size:50vh;block-size:50vw;background:red\"></div>",
    );
    let dom = Dom::parse(&html).unwrap();
    let l = layout(&dom, &Stylesheet::parse(""), geo());
    let mut found = None;
    fn walk(f: &Fragment, out: &mut Option<(f64, f64)>) {
        let (w, h) = (f.size.0.get(), f.size.1.get());
        if (w - 300.0).abs() < 0.5 && (h - 200.0).abs() < 0.5 {
            *out = Some((w, h));
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    walk(&l.pages[0].root, &mut found);
    assert!(found.is_some(), "expected a 300x200pt box (block 50vw=300, inline 50vh=200)");
}

/// Behavior 9: percentage margins/padding resolve against the containing
/// block INLINE size — physical HEIGHT in a vertical mode — and a
/// percentage `inline-size` against the same extent.
#[test]
fn percent_inline_size_uses_containing_block_inline_extent() {
    let html = doc(
        "vertical-rl",
        "<div id=o style=\"inline-size:200px;block-size:100px\"><div id=i style=\"inline-size:50%\"></div></div>",
    );
    let (dom, st) = styles_of(&html, "");
    let i = &st[by_id(&dom, "i")];
    assert_eq!(i.height_percent, Some(0.5), "inline-size:50% is a HEIGHT fraction in vertical-rl");
    assert_eq!(i.width_percent, None);
}

/// Nested both ways: a horizontal subtree inside a vertical one keeps ITS
/// OWN physical sizes (no blind transpose), and a vertical subtree inside a
/// horizontal one maps its own logical sizes.
#[test]
fn nested_modes_use_the_mode_in_effect_at_each_box() {
    let html = "<!DOCTYPE html><html style=\"writing-mode:vertical-rl\"><body style=\"margin:0\">\
        <div id=h style=\"writing-mode:horizontal-tb;inline-size:100px;block-size:200px\">\
          <div id=v style=\"writing-mode:vertical-lr;inline-size:40px;block-size:80px\"></div>\
        </div></body></html>";
    let (dom, st) = styles_of(html, "");
    let h = &st[by_id(&dom, "h")];
    let v = &st[by_id(&dom, "v")];
    // horizontal box inside vertical root: inline = width.
    assert!(h.width.is_some_and(|x| close(x, 75.0)), "h.width {:?}", h.width);
    assert!(h.height.is_some_and(|x| close(x, 150.0)), "h.height {:?}", h.height);
    // vertical-lr box inside horizontal box: inline = height.
    assert!(v.height.is_some_and(|x| close(x, 30.0)), "v.height {:?}", v.height);
    assert!(v.width.is_some_and(|x| close(x, 60.0)), "v.width {:?}", v.width);
    assert_eq!(v.writing_mode, PageWritingMode::VerticalLr);
    assert_eq!(h.writing_mode, PageWritingMode::HorizontalTb);
}

/// Definite vs indefinite containing blocks (Behavior 8/9): a percentage
/// block-size under an indefinite containing block stays a fraction for the
/// layout seam to treat as `auto`; under a definite one the declared extent
/// is kept on the physical axis.
#[test]
fn definite_and_indefinite_extents() {
    let html = doc(
        "vertical-lr",
        "<div id=def style=\"inline-size:200px;block-size:100px\"><div id=c style=\"block-size:50%\"></div></div>\
         <div id=ind><div id=c2 style=\"block-size:50%\"></div></div>",
    );
    let (dom, st) = styles_of(&html, "");
    let c = &st[by_id(&dom, "c")];
    // block-size:50% is a WIDTH fraction in vertical-lr.
    assert_eq!(c.width_percent, Some(0.5));
    assert_eq!(c.height_percent, None);
    let def = &st[by_id(&dom, "def")];
    assert!(def.width.is_some_and(|x| close(x, 75.0)));
    assert!(def.height.is_some_and(|x| close(x, 150.0)));
    let ind = &st[by_id(&dom, "ind")];
    assert!(ind.width.is_none() && ind.height.is_none());
}

/// A horizontal-tb subtree inside a sideways-lr box keeps `ltr` and its own
/// physical inline edges: the walk-time `direction: rtl` stand-in must not
/// leak into it. An explicit `rtl` descendant stays `rtl`.
#[test]
fn horizontal_subtree_in_sideways_lr_keeps_its_direction() {
    let (dom, st) = styles_of(
        &doc(
            "sideways-lr",
            "<div id=o><div id=h style=\"writing-mode:horizontal-tb\">\
             <p id=p style=\"margin-inline-start:8px;margin-inline-end:16px\"></p>\
             <p id=r style=\"direction:rtl;margin-inline-start:8px\"></p></div></div>",
        ),
        "",
    );
    assert!(!st[by_id(&dom, "o")].rtl, "sideways-lr box itself is ltr");
    assert!(!st[by_id(&dom, "h")].rtl, "horizontal subtree is ltr");
    let p = &st[by_id(&dom, "p")];
    assert!(!p.rtl);
    assert!(close(p.margin_left, 6.0) && close(p.margin_right, 12.0), "{:?} {:?}", p.margin_left, p.margin_right);
    let r = &st[by_id(&dom, "r")];
    assert!(r.rtl);
    assert!(close(r.margin_right, 6.0) && close(r.margin_left, 0.0));
}

/// CORE-562 (css-logical-1 §4.3): `border-{block,inline}-*-style: none`
/// removes the physical side the mode maps it to, after a `border: 5px
/// solid` shorthand. Before the fix, the declared-width pass re-applied the
/// shorthand's 5px (3.75pt) to the side stylo had already zeroed. Both the
/// stylesheet and inline paths are checked.
#[test]
fn logical_border_style_none_removes_mapped_side() {
    let w = 3.75;
    // (mode, logical style property, [top, right, bottom, left])
    let cases: &[(&str, &str, [f64; 4])] = &[
        ("horizontal-tb", "border-block-end-style", [w, w, 0.0, w]),
        ("horizontal-tb", "border-inline-start-style", [w, w, w, 0.0]),
        ("vertical-lr", "border-block-start-style", [w, w, w, 0.0]),
        ("vertical-lr", "border-block-end-style", [w, 0.0, w, w]),
        ("vertical-rl", "border-block-start-style", [w, 0.0, w, w]),
        ("vertical-rl", "border-inline-end-style", [w, w, 0.0, w]),
    ];
    for (wm, prop, want) in cases {
        for inline in [false, true] {
            let (html, css) = if inline {
                (doc(wm, &format!("<div id=d style=\"border:5px solid black;{prop}:none\"></div>")), String::new())
            } else {
                (doc(wm, "<div id=d class=b></div>"), format!(".b {{ border: 5px solid black; {prop}: none; }}"))
            };
            let (dom, st) = styles_of(&html, &css);
            let s = &st[by_id(&dom, "d")];
            let got = [s.border_top, s.border_right, s.border_bottom, s.border_left];
            for (i, side) in ["top", "right", "bottom", "left"].iter().enumerate() {
                assert!(
                    close(got[i], want[i]),
                    "{wm} {prop} inline={inline}: border-{side} {} want {}",
                    got[i].get(),
                    want[i]
                );
            }
        }
    }
}
