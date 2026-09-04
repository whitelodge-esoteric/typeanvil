// e2e probe: fragment tree walk (gridcols verified separately = correct).
use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(300.0),
        height: Scalar(225.0),
        margin_top: Scalar::ZERO,
        margin_right: Scalar::ZERO,
        margin_bottom: Scalar::ZERO,
        margin_left: Scalar::ZERO,
    }
}

fn tag_of(dom: &Dom, id: Option<typeanvil::dom::NodeId>) -> String {
    match id {
        Some(i) => match &dom.nodes[i].kind {
            NodeKind::Element(el) => format!("<{} {:?}>", el.tag, el.classes),
            _ => "?".to_string(),
        },
        None => "-".to_string(),
    }
}

fn walk(dom: &Dom, frag: &typeanvil::frag::Fragment, depth: usize, ox: f64, oy: f64) {
    let x = ox + frag.offset.x.get();
    let y = oy + frag.offset.y.get();
    println!(
        "{}{} src={} x={:.1} y={:.1} w={:.1} h={:.1}",
        "  ".repeat(depth),
        tag_of(dom, frag.source),
        frag.source.map(|i| i).unwrap_or(9999),
        x,
        y,
        frag.size.0.get(),
        frag.size.1.get()
    );
    for c in &frag.children {
        walk(dom, c, depth + 1, x, y);
    }
}

#[test]
fn grid_probe_layout() {
    let html = r#"<html><head><style>
        body { margin: 0; }
        .g { display: grid; grid-template-columns: 100px auto 100px; width: 300px; }
        .a { height: 30px; background: red; }
        .b { height: 30px; background: lime; }
        .c { height: 30px; background: blue; }
    </style></head>
    <body><div class="g"><div class="a">A</div><div class="b">B</div><div class="c">C</div></div></body></html>"#;
    let dom = Dom::parse(html).unwrap();
    let stylesheet = Stylesheet::parse(
        "body { margin: 0; } .g { display: grid; grid-template-columns: 100px auto 100px; width: 300px; } .a { height: 30px; background: red; } .b { height: 30px; background: lime; } .c { height: 30px; background: blue; }",
    );
    let layout = layout(&dom, &stylesheet, geo());
    walk(&dom, &layout.pages[0].root, 0, 0.0, 0.0);
}
