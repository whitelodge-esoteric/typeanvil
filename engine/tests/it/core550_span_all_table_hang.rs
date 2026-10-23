//! CORE-550: a `column-span: all` table taller than one page, inside a
//! multicol container, paginated without end. A spanner at the top of a
//! fragmentainer was deferred to the next page again and again. Layout runs
//! on a worker thread so a regression fails on a timeout and a page-count
//! bound instead of waiting for the page cap.

use std::sync::mpsc;
use std::time::Duration;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn html() -> String {
    let mut rows = String::new();
    for i in 1..=24 {
        rows.push_str(&format!("<tr><td>{i}</td><td>a</td></tr>"));
    }
    format!(
        r#"<!DOCTYPE html><html><style>
:root {{ font: 10pt sans-serif; }}
.cols {{ columns: 2; column-gap: 10pt; }}
table {{ column-span: all; border-collapse: collapse; width: 100%; }}
td {{ padding: 2pt; border: 1px solid #000; }}
</style>
<div class="cols"><p>Intro text before the table.</p>
<table><tbody>{rows}</tbody></table>
<p>Text after the table.</p></div>
</html>"#
    )
}

fn pages_of(html: String) -> usize {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let inches = |v: f64| Scalar(v * 72.0);
        let geometry = PageGeometry {
            width: inches(5.0),
            height: inches(3.0),
            margin_top: inches(0.25),
            margin_right: inches(0.25),
            margin_bottom: inches(0.25),
            margin_left: inches(0.25),
        };
        let s = html.find("<style>").unwrap() + "<style>".len();
        let e = html.find("</style>").unwrap();
        let dom = Dom::parse(&html).expect("dom parses");
        let ss = Stylesheet::parse(&html[s..e]);
        let laid = layout(&dom, &ss, geometry);
        let _ = tx.send(laid.pages.len());
    });
    rx.recv_timeout(Duration::from_secs(30))
        .expect("layout did not finish in 30s: pagination does not terminate")
}

#[test]
fn tall_span_all_table_in_multicol_terminates() {
    let n = pages_of(html());
    assert!((2..=5).contains(&n), "unexpected page count {n}");
}
