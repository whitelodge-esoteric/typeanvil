// Debug probe: render a fixture and dump the layout tree.
use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::Fragment;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: cargo run --example probe_layout -- <fixture.html>");
    let html = std::fs::read_to_string(&path).unwrap();
    let dom = Dom::parse(&html).unwrap();
    let sheet = Stylesheet::parse(&html);
    let geo = PageGeometry {
        width: Scalar(5.0 * 72.0),
        height: Scalar(3.0 * 72.0),
        margin_top: Scalar(0.5 * 72.0),
        margin_right: Scalar(0.5 * 72.0),
        margin_bottom: Scalar(0.5 * 72.0),
        margin_left: Scalar(0.5 * 72.0),
    };
    let lay = layout::layout(&dom, &sheet, geo);
    println!("pages: {}", lay.pages.len());
    for (pi, page) in lay.pages.iter().enumerate() {
        println!("--- page {} ---", pi + 1);
        fn walk(f: &Fragment, depth: usize) {
            let ind = "  ".repeat(depth);
            println!(
                "{}{:?} off=({:.1},{:.1}) size=({:.1},{:.1}) src={:?}",
                ind,
                f.kind,
                f.offset.x.get(),
                f.offset.y.get(),
                f.size.0.get(),
                f.size.1.get(),
                f.source
            );
            for c in &f.children {
                walk(c, depth + 1);
            }
        }
        walk(&page.root, 0);
    }
}
