//! CORE-241: a block with a declared `height: 0` takes no flow space, even
//! when block children overflow it. The overflow must not continue the box
//! across fragmentainers (monolithic-overflow-031 ref: height:0 wrappers over
//! a 675px and a 50px child, then a 1450px box = 5 pages, same as a plain
//! 1450px box).

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn pages(html: &str) -> usize {
    let geo = PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    };
    let dom = Dom::parse(html).expect("parse html");
    let sheet = Stylesheet::parse("");
    layout(&dom, &sheet, geo).pages.len()
}

const BASE: &str = "<!DOCTYPE html><body style=\"margin:0\">";

#[test]
fn zero_height_wrapper_with_tall_children_adds_no_pages() {
    let plain = format!("{BASE}<div style=\"width:50%;height:1450px\"></div>");
    let wrapped = format!(
        "{BASE}<div style=\"height:0\"><div style=\"height:675px\"></div>\
         <div style=\"height:50px\"></div></div>\
         <div style=\"width:50%;height:1450px\"></div>"
    );
    assert_eq!(pages(&wrapped), pages(&plain));
}

#[test]
fn zero_height_wrapper_alone_stays_one_page() {
    let wrapped = format!(
        "{BASE}<div style=\"height:0\"><div style=\"height:675px\"></div></div>"
    );
    assert_eq!(pages(&wrapped), 1);
}
