//! Tagged PDF role fill (CORE-374, tagged-pdf.spec.md §2b, AC-14).
//!
//! The tree is read back from the raw PDF (krilla writes `/StructElem`
//! objects uncompressed) as a pre-order `Role:leaves` signature.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, Element};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout;
use typeanvil::pdf::{render_with_metadata, render_with_options, DocumentMetadata};
use typeanvil::tags::{role_for, TagRole};

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

struct Elem {
    role: String,
    kids: Vec<usize>,
    leaves: usize,
}

fn parse_elems(pdf: &[u8]) -> std::collections::BTreeMap<usize, Elem> {
    let text = String::from_utf8_lossy(pdf).into_owned();
    let mut out = std::collections::BTreeMap::new();
    let mut at = 0;
    while let Some(i) = text[at..].find("/Type/StructElem") {
        let pos = at + i;
        let head = &text[..pos];
        let obj_at = head.rfind(" 0 obj").expect("obj header");
        let num_start = head[..obj_at]
            .rfind(|c: char| !c.is_ascii_digit())
            .map_or(0, |p| p + 1);
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
            let toks: Vec<&str> = dict[k + 2..]
                .split(|c: char| c.is_whitespace() || "[]<>/".contains(c))
                .filter(|t| !t.is_empty())
                .collect();
            let mut j = 0;
            let mut prev = "";
            while j < toks.len() {
                let t = toks[j];
                if matches!(t, "Pg" | "Lang" | "A" | "Alt" | "T") {
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

fn sig(html: &str, css: &str) -> Vec<String> {
    let dom = Dom::parse(html).unwrap();
    let layout = layout::layout(&dom, &Stylesheet::parse(css), geometry());
    let meta = DocumentMetadata {
        title: Some("T".into()),
        ..DocumentMetadata::default()
    };
    let pdf = render_with_options(&layout, Some(&dom), &meta, true, false).unwrap();
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
    out
}

fn roles(s: &[String]) -> Vec<&str> {
    s.iter().map(|x| x.split(':').next().unwrap()).collect()
}

fn wrap(body: &str) -> String {
    format!(r#"<html lang="en"><head><title>T</title></head><body>{body}</body></html>"#)
}

fn el(tag: &str) -> Element {
    Element {
        tag: tag.to_string(),
        id: None,
        classes: Vec::new(),
        attrs: Default::default(),
    }
}

#[test]
fn role_for_maps_new_elements() {
    assert_eq!(role_for(&el("caption")), TagRole::Caption);
    assert_eq!(role_for(&el("section")), TagRole::Section);
    assert_eq!(role_for(&el("aside")), TagRole::Section);
    assert_eq!(role_for(&el("nav")), TagRole::Section);
    assert_eq!(role_for(&el("article")), TagRole::Article);
    // dl/dt/dd are unchanged (spec §2b).
    assert_eq!(role_for(&el("dl")), TagRole::List);
    assert_eq!(role_for(&el("dt")), TagRole::ListItem);
    assert_eq!(role_for(&el("dd")), TagRole::ListItem);
}

#[test]
fn caption_is_first_child_of_table() {
    // A bottom caption paints after the grid but is still the first child.
    for side in ["top", "bottom"] {
        let html = wrap("<table><caption>Cap</caption><tbody><tr><td>1</td></tr></tbody></table>");
        let s = sig(&html, &format!("caption{{caption-side:{side}}}"));
        let r = roles(&s);
        let t = r.iter().position(|x| *x == "Table").unwrap();
        assert_eq!(r[t + 1], "Caption", "{side}: {s:?}");
        assert!(s[t + 1].starts_with("Caption:"), "{s:?}");
    }
}

#[test]
fn sectioning_elements_get_sect_and_art() {
    let html = wrap(
        "<section><p>S</p></section><article><p>A</p></article>\
         <aside><p>X</p></aside><nav><p>N</p></nav>",
    );
    let s = sig(&html, "");
    let r = roles(&s);
    for want in ["Sect", "Art"] {
        assert!(r.contains(&want), "missing {want}: {s:?}");
    }
    assert_eq!(r.iter().filter(|x| **x == "Sect").count(), 3, "{s:?}");
    assert_eq!(r.iter().filter(|x| **x == "Art").count(), 1, "{s:?}");
}

#[test]
fn footnote_body_is_a_note() {
    let html = wrap(r#"<p>A call<span class="fn">Note body</span> after</p><p>B</p>"#);
    let s = sig(&html, ".fn{float:footnote}");
    let r = roles(&s);
    let p = r.iter().position(|x| *x == "P").unwrap();
    assert_eq!(r[p + 1], "Note", "note directly follows its call paragraph: {s:?}");
    assert!(!s[p + 1].ends_with(":0"), "note owns its lines: {s:?}");
}

#[test]
fn list_marker_rides_in_lbl() {
    let html = wrap("<ul><li>One</li><li>Two</li></ul><ol><li>Uno</li></ol>");
    let s = sig(&html, "");
    let r = roles(&s);
    assert_eq!(r.iter().filter(|x| **x == "LI").count(), 3, "{s:?}");
    assert_eq!(r.iter().filter(|x| **x == "Lbl").count(), 3, "{s:?}");
    for (i, x) in r.iter().enumerate() {
        if *x == "Lbl" {
            assert_eq!(s[i], "Lbl:1", "marker is the Lbl's one leaf: {s:?}");
            assert_eq!(r[i + 1], "LBody", "{s:?}");
            assert!(!s[i + 1].ends_with(":0"), "LBody owns the item text: {s:?}");
        }
    }
}

#[test]
fn unmarked_list_item_has_no_lbl() {
    let html = wrap("<ul><li>One</li></ul>");
    let s = sig(&html, "li{list-style:none}");
    assert!(!roles(&s).contains(&"Lbl"), "{s:?}");
}

#[test]
fn untagged_output_stays_byte_stable() {
    let html = wrap(
        r#"<table><caption>C</caption><tr><td>1</td></tr></table><ul><li>x</li></ul>
        <p>a<span class="fn">n</span></p>"#,
    );
    let dom = Dom::parse(&html).unwrap();
    let l = layout::layout(&dom, &Stylesheet::parse(".fn{float:footnote}"), geometry());
    let meta = DocumentMetadata::default();
    let old = render_with_metadata(&l, &meta).unwrap();
    let new = render_with_options(&l, None, &meta, false, false).unwrap();
    assert_eq!(old, new);
}
