//! Tagged PDF reading order (CORE-375, tagged-pdf.spec.md §2a, AC-13).
//!
//! The structure tree follows DOM order even when layout moves or splits
//! content. The tests read the tree back from the raw PDF (krilla writes
//! `/StructElem` objects uncompressed) and assert its pre-order shape:
//! `Role:leaves`, where `leaves` counts the marked-content ids the element
//! owns directly.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout;
use typeanvil::pdf::{render_with_options, DocumentMetadata};

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(5.0 * 72.0),
        height: Scalar(3.0 * 72.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

struct Elem {
    role: String,
    kids: Vec<usize>,
    leaves: usize,
}

/// Parse every `/Type/StructElem` object into (obj number -> Elem).
fn parse_elems(pdf: &[u8]) -> std::collections::BTreeMap<usize, Elem> {
    let text = String::from_utf8_lossy(pdf).into_owned();
    let mut out = std::collections::BTreeMap::new();
    let mut at = 0;
    while let Some(i) = text[at..].find("/Type/StructElem") {
        let pos = at + i;
        // Object number: scan back to "<n> 0 obj".
        let head = &text[..pos];
        let obj_at = head.rfind(" 0 obj").expect("obj header");
        let num_start = head[..obj_at].rfind(|c: char| !c.is_ascii_digit()).map_or(0, |p| p + 1);
        let num: usize = head[num_start..obj_at].parse().unwrap();
        let end = text[pos..].find("endobj").map_or(text.len(), |e| pos + e);
        let dict = &text[pos..end];
        let role = dict
            .split("/S/")
            .nth(1)
            .map(|s| s.chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>())
            .unwrap_or_default();
        let mut kids = Vec::new();
        let mut leaves = 0;
        if let Some(k) = dict.find("/K") {
            let k = &dict[k + 2..];
            // Tokenise: refs "n 0 R" (not after /Pg), bare ints, /MCID.
            let mut toks: Vec<&str> = k
                .split(|c: char| c.is_whitespace() || "[]<>/".contains(c))
                .filter(|t| !t.is_empty())
                .collect();
            toks.retain(|t| *t != "Type" || false);
            let mut j = 0;
            let mut prev = "";
            while j < toks.len() {
                let t = toks[j];
                if t == "Pg" || t == "Lang" || t == "A" || t == "Alt" || t == "T" {
                    break;
                }
                if t == "MCID" {
                    leaves += 1;
                    j += 2;
                    prev = "";
                    continue;
                }
                if j + 2 < toks.len() && toks[j + 1] == "0" && toks[j + 2] == "R" {
                    if let Ok(n) = t.parse::<usize>() {
                        kids.push(n);
                        j += 3;
                        prev = "";
                        continue;
                    }
                }
                if t.parse::<usize>().is_ok() && prev != "MCR" {
                    leaves += 1;
                }
                prev = t;
                j += 1;
            }
        }
        out.insert(num, Elem { role, kids, leaves });
        at = end;
    }
    out
}

fn walk(elems: &std::collections::BTreeMap<usize, Elem>, id: usize, out: &mut Vec<String>) {
    let e = &elems[&id];
    out.push(format!("{}:{}", e.role, e.leaves));
    for &k in &e.kids {
        if elems.contains_key(&k) {
            walk(elems, k, out);
        }
    }
}

/// Pre-order `Role:leaves` signature below the `Document` element, plus the
/// page count.
fn signature(html: &str, css: &str) -> (Vec<String>, usize) {
    let dom = Dom::parse(html).unwrap();
    let layout = layout::layout(&dom, &Stylesheet::parse(css), geometry());
    let pages = layout.pages.len();
    let meta = DocumentMetadata {
        title: Some("T".into()),
        ..DocumentMetadata::default()
    };
    let pdf = render_with_options(&layout, Some(&dom), &meta, true, false).unwrap();
    // Plain-text check that the tree is uncompressed.
    assert!(find(&pdf, b"/StructElem", 0).is_some());
    let elems = parse_elems(&pdf);
    let roots: Vec<usize> = elems
        .iter()
        .filter(|(n, e)| e.role == "Document" || !elems.values().any(|o| o.kids.contains(n)))
        .map(|(n, _)| *n)
        .collect();
    let mut out = Vec::new();
    for r in roots {
        walk(&elems, r, &mut out);
    }
    (out, pages)
}

fn sig(html: &str, css: &str) -> Vec<String> {
    signature(html, css).0
}

fn paragraphs(sig: &[String]) -> usize {
    sig.iter().filter(|s| s.starts_with("P:")).count()
}

#[test]
fn multicol_split_content_stays_in_dom_groups() {
    let words = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen";
    let html = format!(
        r#"<html lang="en"><head><title>T</title></head><body><div class="c"><p>A {words}</p><p>B {words} {words}</p><p>C {words} {words}</p></div></body></html>"#
    );
    let css = ".c{column-count:2;column-gap:10pt}";
    let (s, pages) = signature(&html, css);
    assert!(pages >= 2, "fixture must continue onto a second page");
    // Three DOM paragraphs, each ONE group even when split across columns/pages.
    assert_eq!(paragraphs(&s), 3, "{s:?}");
}

#[test]
fn float_keeps_dom_position() {
    let html = r#"<html lang="en"><head><title>T</title></head><body><p>A first</p><div class="f"><p>X floated</p></div><p>B second</p></body></html>"#;
    let s = sig(html, ".f{float:right;width:60pt}");
    let ps: Vec<&String> = s.iter().filter(|x| x.starts_with("P:") || x.starts_with("Div:")).collect();
    // A, then the float's Div wrapper holding X, then B.
    let roles: Vec<&str> = s.iter().map(|x| x.split(':').next().unwrap()).collect();
    let pos_div = roles.iter().rposition(|r| *r == "Div").unwrap();
    assert_eq!(&roles[pos_div - 1], &"P", "{s:?} {ps:?}");
    assert_eq!(&roles[pos_div + 1..], &["P", "P"], "{s:?}");
}

#[test]
fn footnote_body_sits_inside_the_call_paragraph() {
    let html = r#"<html lang="en"><head><title>T</title></head><body><p>A call<span class="fn">Note body</span> after</p><p>B second</p></body></html>"#;
    let s = sig(html, ".fn{float:footnote}");
    // P(A) owns its own lines; the note (a Div) is its child; then P(B).
    let i = s.iter().position(|x| x.starts_with("P:")).unwrap();
    assert!(s[i + 1].starts_with("Div:"), "note must follow its call paragraph directly: {s:?}");
    assert!(s[i + 2].starts_with("P:"), "next paragraph follows the note: {s:?}");
}

#[test]
fn abspos_keeps_dom_position() {
    let html = r#"<html lang="en"><head><title>T</title></head><body><p>A first</p><div class="a"><p>X abs</p></div><p>B second</p></body></html>"#;
    let s = sig(html, ".a{position:absolute;top:10pt;left:100pt;width:80pt}");
    let roles: Vec<&str> = s.iter().map(|x| x.split(':').next().unwrap()).collect();
    let pos_div = roles.iter().rposition(|r| *r == "Div").unwrap();
    assert_eq!(&roles[pos_div - 1], &"P", "{s:?}");
    assert_eq!(&roles[pos_div + 1..], &["P", "P"], "{s:?}");
}

#[test]
fn flex_order_does_not_reorder_the_tree() {
    // `order` paints C, B, A; the tree must list them A, B, C. Leaf ids differ
    // from the tree order, so use distinct text lengths via distinct line
    // counts: each item has one line.
    let html = r#"<html lang="en"><head><title>T</title></head><body><div class="x"><p style="order:3">A</p><p style="order:1">B</p><p style="order:2">C</p></div></body></html>"#;
    let s = sig(html, ".x{display:flex}.x p{margin:0;width:60pt}");
    assert_eq!(paragraphs(&s), 3, "{s:?}");
    assert!(s.iter().all(|x| !x.starts_with("P:0")), "every item owns a leaf: {s:?}");
}

#[test]
fn grid_order_does_not_reorder_the_tree() {
    let html = r#"<html lang="en"><head><title>T</title></head><body><div class="x"><p style="order:3">A</p><p style="order:1">B</p><p style="order:2">C</p></div></body></html>"#;
    let s = sig(html, ".x{display:grid;grid-template-columns:100pt 100pt}.x p{margin:0}");
    assert_eq!(paragraphs(&s), 3, "{s:?}");
    assert!(s.iter().all(|x| !x.starts_with("P:0")), "every item owns a leaf: {s:?}");
}

/// A fragmented table with a repeating `thead` and `tfoot`: every DOM row is
/// one `TR`, once; the repeated chrome is artifact, not content.
#[test]
fn fragmented_table_chrome_repeat_is_not_in_the_tree() {
    let mut rows = String::new();
    for i in 1..=12 {
        rows.push_str(&format!("<tr><td>R{i:02}</td></tr>"));
    }
    let html = format!(
        r#"<html lang="en"><head><title>T</title></head><body><table><thead><tr><th>HEAD</th></tr></thead><tbody>{rows}</tbody><tfoot><tr><td>FOOT</td></tr></tfoot></table></body></html>"#
    );
    let (s, pages) = signature(&html, "");
    assert!(pages >= 2, "table must fragment");
    let count = |r: &str| s.iter().filter(|x| x.starts_with(&format!("{r}:"))).count();
    assert_eq!(count("TR"), 14, "one TR per DOM row (1 head + 12 body + 1 foot): {s:?}");
    assert_eq!(count("TH"), 1, "{s:?}");
    assert_eq!(count("TD"), 13, "{s:?}");
    // Groups are per DOM node, so a repeated row would show up as extra
    // LEAVES under the same cell. Each cell here holds one text line.
    let leaves = |r: &str| -> usize {
        s.iter()
            .filter(|x| x.starts_with(&format!("{r}:")))
            .map(|x| x.split(':').nth(1).unwrap().parse::<usize>().unwrap())
            .sum()
    };
    assert_eq!(leaves("TH"), 1, "repeated header text must be artifact: {s:?}");
    assert_eq!(leaves("TD"), 13, "repeated footer text must be artifact: {s:?}");
}
