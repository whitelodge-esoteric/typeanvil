//! CORE-319: element-level `background-image` with position / size / repeat.
//!
//! The painted result is a `FragmentContent::BackgroundImage` child of the
//! element's box fragment. Tests assert the run's tile size, first-tile
//! offset and repeat flags, plus the tile origins the painter will use.

use tempfile::TempDir;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{BackgroundImageRun, Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::images::ImageStore;
use typeanvil::layout::{layout_with_images_and_store, Layout};

use super::images::{make_png, write_fixture};

fn geometry() -> PageGeometry {
    let m = Scalar(36.0);
    PageGeometry {
        width: Scalar(400.0),
        height: Scalar(400.0),
        margin_top: m,
        margin_right: m,
        margin_bottom: m,
        margin_left: m,
    }
}

fn lay(css: &str, body: &str, dir: &TempDir) -> Layout {
    let html = format!("<html><head><style>{css}</style></head><body>{body}</body></html>");
    let dom = Dom::parse(&html).unwrap();
    let ss = Stylesheet::parse(css);
    let mut store = ImageStore::new();
    layout_with_images_and_store(&dom, &ss, geometry(), Some(dir.path()), &mut store)
}

fn runs(layout: &Layout) -> Vec<(BackgroundImageRun, f64, f64)> {
    fn walk(f: &Fragment, out: &mut Vec<(BackgroundImageRun, f64, f64)>) {
        if let FragmentContent::BackgroundImage(r) = &f.content {
            out.push((r.clone(), f.size.0.get(), f.size.1.get()));
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    for page in &layout.pages {
        for root in page.paint_roots() {
            walk(root, &mut out);
        }
    }
    out
}

fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    // 40x20 px = 30x15 pt.
    write_fixture(&dir, "t.png", &make_png(40, 20, [200, 30, 30]));
    dir
}

const BOX: &str = "#b { width: 200px; height: 100px; }"; // 150 x 75 pt

#[test]
fn natural_size_tiles_from_top_left() {
    let dir = fixture();
    let l = lay(
        &format!("{BOX} #b {{ background-image: url(t.png); }}"),
        "<div id=b></div>",
        &dir,
    );
    let r = runs(&l);
    assert_eq!(r.len(), 1, "element background-image must produce a run");
    let (run, w, h) = &r[0];
    assert!((run.tile_w.get() - 30.0).abs() < 0.05, "{}", run.tile_w.get());
    assert!((run.tile_h.get() - 15.0).abs() < 0.05);
    assert!(run.repeat_x && run.repeat_y);
    // 150 x 75 box tiled with 30 x 15 tiles = 5 x 5.
    assert_eq!(run.tile_origins(*w, *h).len(), 25);
}

#[test]
fn no_repeat_and_position_percent() {
    let dir = fixture();
    let l = lay(
        &format!("{BOX} #b {{ background: url(t.png) no-repeat 100% 100%; }}"),
        "<div id=b></div>",
        &dir,
    );
    let (run, w, h) = &runs(&l)[0];
    assert!(!run.repeat_x && !run.repeat_y);
    let t = run.tile_origins(*w, *h);
    assert_eq!(t.len(), 1);
    // bottom-right aligned: 150-30, 75-15
    assert!((t[0].0 - 120.0).abs() < 0.1 && (t[0].1 - 60.0).abs() < 0.1, "{t:?}");
}

#[test]
fn repeat_x_only() {
    let dir = fixture();
    let l = lay(
        &format!("{BOX} #b {{ background-image: url(t.png); background-repeat: repeat-x; }}"),
        "<div id=b></div>",
        &dir,
    );
    let (run, w, h) = &runs(&l)[0];
    assert!(run.repeat_x && !run.repeat_y);
    assert_eq!(run.tile_origins(*w, *h).len(), 5);
}

#[test]
fn size_percent_keeps_ratio_with_auto() {
    let dir = fixture();
    // The letter-skyline shape: `no-repeat top left / 50% url(...)`.
    let l = lay(
        &format!("{BOX} #b {{ background: no-repeat top left / 50% url(t.png); }}"),
        "<div id=b></div>",
        &dir,
    );
    let (run, _, _) = &runs(&l)[0];
    assert!((run.tile_w.get() - 75.0).abs() < 0.1, "{}", run.tile_w.get());
    assert!((run.tile_h.get() - 37.5).abs() < 0.1, "{}", run.tile_h.get());
}

#[test]
fn cover_and_contain() {
    let dir = fixture();
    // Area 150 x 75 (ratio 2:1) matches the image ratio; use a 100px-tall
    // 100px-wide box (75 x 75 pt) so cover and contain differ.
    let sq = "#b { width: 100px; height: 100px; }";
    let cov = lay(
        &format!("{sq} #b {{ background: url(t.png) center / cover no-repeat; }}"),
        "<div id=b></div>",
        &dir,
    );
    let (run, _, _) = &runs(&cov)[0];
    assert!((run.tile_h.get() - 75.0).abs() < 0.1 && (run.tile_w.get() - 150.0).abs() < 0.1);
    // centered: x offset = (75-150)/2
    assert!((run.offset_x.get() + 37.5).abs() < 0.1, "{}", run.offset_x.get());
    let con = lay(
        &format!("{sq} #b {{ background: url(t.png) center / contain no-repeat; }}"),
        "<div id=b></div>",
        &dir,
    );
    let (run, _, _) = &runs(&con)[0];
    assert!((run.tile_w.get() - 75.0).abs() < 0.1 && (run.tile_h.get() - 37.5).abs() < 0.1);
}

#[test]
fn shorthand_keeps_color_and_image() {
    let dir = fixture();
    let l = lay(
        &format!("{BOX} #b {{ background: #00ff00 url(t.png) no-repeat; }}"),
        "<div id=b></div>",
        &dir,
    );
    assert_eq!(runs(&l).len(), 1);
    fn has_bg(f: &Fragment) -> bool {
        matches!(f.content, FragmentContent::Background(_)) || f.children.iter().any(has_bg)
    }
    assert!(l.pages.iter().any(|p| p.paint_roots().iter().any(|r| has_bg(r))));
}

#[test]
fn missing_image_paints_nothing() {
    let dir = fixture();
    let l = lay(
        &format!("{BOX} #b {{ background-image: url(nope.png); }}"),
        "<div id=b></div>",
        &dir,
    );
    assert!(runs(&l).is_empty());
}
