//! CORE-395: a grid whose cell holds a multi-line inline-block, inside a
//! balanced multicol container, paginated without end (css-multicol
//! `balance-grid-container.html`). Own test binary so a regression that
//! hangs cannot mask other tests: layout runs on a worker thread and the
//! test fails on a timeout instead of waiting for the page cap.

use std::sync::mpsc;
use std::time::Duration;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

const HTML: &str = r#"<!DOCTYPE html><html><style>
:root { font: 16px/1.25 sans-serif; }
.c { column-count: 2; width: 550px; }
.g { display: grid; grid-template-columns: 1fr 1fr; grid-auto-rows: auto; }
.k { display: inline-block; }
.a { grid-column: 1; }
.b { grid-column: 2; }
</style>
<div class="c"><div class="g">
<div class="a">Address</div>
<div class="b"><div class="k">123 Fake Street<br>London<br>NW1 1AA<br>UK</div></div>
<div class="a">Relevant Countries</div>
<div class="b">Ireland<br>United Kingdom</div>
</div></div>
<div>Some content underneath the details.</div>
</html>"#;

fn pages_of(html: &'static str) -> usize {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let inches = |v: f64| Scalar(v * 72.0);
        let geometry = PageGeometry {
            width: inches(5.0),
            height: inches(3.0),
            margin_top: inches(0.5),
            margin_right: inches(0.5),
            margin_bottom: inches(0.5),
            margin_left: inches(0.5),
        };
        let s = html.find("<style>").unwrap() + "<style>".len();
        let e = html.find("</style>").unwrap();
        let dom = Dom::parse(html).expect("dom parses");
        let ss = Stylesheet::parse(&html[s..e]);
        let laid = layout(&dom, &ss, geometry);
        let _ = tx.send(laid.pages.len());
    });
    rx.recv_timeout(Duration::from_secs(30))
        .expect("layout did not finish in 30s: pagination does not terminate")
}

#[test]
fn grid_with_inline_block_cell_in_balanced_multicol_terminates() {
    let n = pages_of(HTML);
    assert!(n >= 1 && n <= 3, "unexpected page count {n}");
}
