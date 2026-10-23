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
/// block pair in a vertical mode is deferred, see
/// `block_margins_in_vertical_modes_are_deferred_to_core553`.
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

/// DEFERRED to CORE-553: `margin-block-start/end` in a vertical mode keep the
/// horizontal-tb slots (top/bottom). The block path still paginates along y,
/// so a physical left/right block margin turns the
/// page-name-orthogonal-writing-002 ref's `margin-block-end: 999in` into a
/// zero-width box that drops its text instead of breaking the page (canary).
/// CORE-553 flips this when it moves progression to x.
#[test]
fn block_margins_in_vertical_modes_are_deferred_to_core553() {
    let (dom, st) = styles_of(
        &doc(
            "vertical-rl",
            "<div id=d style=\"margin-block-start:24px;margin-block-end:32px\"></div>",
        ),
        "",
    );
    let s = &st[by_id(&dom, "d")];
    assert!(close(s.margin_top, 18.0) && close(s.margin_bottom, 24.0));
    assert!(close(s.margin_left, 0.0) && close(s.margin_right, 0.0));
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
