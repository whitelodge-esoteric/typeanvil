// SPDX-License-Identifier: AGPL-3.0-only

//! MathML Core layout (CORE-363, spec `math-rendering.spec.md`).
//!
//! `layout_math` walks the `<math>` subtree and returns a [`MathBox`]: a list
//! of positioned shaped text runs and filled rules, measured from the math
//! baseline. `layout.rs` turns the box into ordinary fragments, so the PDF
//! text layer, ToUnicode and tagged paths need no math-specific emitter code.
//!
//! Coordinates: `x` grows right from the box origin; `y` grows DOWN from the
//! math baseline (negative = above). `Text.y` is a baseline, `Rule.y` the top
//! edge of the rule.

use crate::dom::{Dom, NodeId, NodeKind};
use crate::fonts::{self, FaceId};
use crate::geom::Scalar;
use crate::typography::{shape_word, ShapedGlyph};

/// One positioned item of a laid-out equation.
#[derive(Clone, Debug)]
pub enum MathOp {
    Text {
        x: f64,
        y: f64,
        size: f64,
        face: FaceId,
        text: String,
        glyphs: Vec<ShapedGlyph>,
    },
    Rule {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },
}

/// A laid-out equation. Sizes in points.
#[derive(Clone, Debug, Default)]
pub struct MathBox {
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
    pub ops: Vec<MathOp>,
}

/// True when `id` is a `<math>` element.
pub fn is_math(dom: &Dom, id: NodeId) -> bool {
    dom.nodes[id]
        .kind
        .element()
        .is_some_and(|el| el.tag == "math")
}

/// True for `<math display="block">`.
pub fn is_block_math(dom: &Dom, id: NodeId) -> bool {
    dom.nodes[id]
        .kind
        .element()
        .is_some_and(|el| el.tag == "math" && el.attr("display") == Some("block"))
}

/// The face used for math: the installed STIX Two Math, else `fallback`.
pub fn math_face(fallback: FaceId) -> FaceId {
    let faces = fonts::system_family_faces("STIX Two Math");
    match faces.first() {
        Some(info) => fonts::register_system_face_pub(info),
        None => fallback,
    }
}

/// Lay out the `<math>` element `id` at `size` points.
pub fn layout_math(dom: &Dom, id: NodeId, size: f64, fallback_face: FaceId) -> MathBox {
    let face = math_face(fallback_face);
    let cx = Cx { dom, face, root: size };
    let st = St {
        size,
        display: is_block_math(dom, id),
    };
    let kids = cx.elems(id);
    let b = cx.row(&kids, st);
    MathBox {
        width: b.w,
        ascent: b.asc,
        descent: b.desc,
        ops: b.ops,
    }
}

// ---------------------------------------------------------------------------

const AXIS: f64 = 0.25;
const RULE: f64 = 0.06;
const SUP_UP: f64 = 0.363;
const SUB_DOWN: f64 = 0.247;
const NUM_UP_D: f64 = 0.677;
const NUM_UP_T: f64 = 0.394;
const DEN_DN_D: f64 = 0.686;
const DEN_DN_T: f64 = 0.345;
const LIMIT_GAP: f64 = 0.11;
const SCRIPT_SPACE: f64 = 0.056;
const SCRIPT_SCALE: f64 = 0.71;

#[derive(Clone, Copy)]
struct St {
    size: f64,
    display: bool,
}

#[derive(Clone, Default)]
struct B {
    w: f64,
    asc: f64,
    desc: f64,
    ops: Vec<MathOp>,
}

impl B {
    fn put(&mut self, other: B, dx: f64, dy: f64) {
        for mut op in other.ops {
            match &mut op {
                MathOp::Text { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
                MathOp::Rule { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
            }
            self.ops.push(op);
        }
    }
    fn rule(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.ops.push(MathOp::Rule { x, y, w, h });
    }
}

struct Cx<'a> {
    dom: &'a Dom,
    face: FaceId,
    root: f64,
}

/// Glyph box in points at `size`: (x_min, y_min, x_max, y_max), y up.
fn glyph_box(face: FaceId, gid: u32, size: f64) -> Option<(f64, f64, f64, f64)> {
    use skrifa::instance::{LocationRef, Size};
    use skrifa::MetadataProvider;
    let bytes = fonts::face_bytes(face);
    let font = skrifa::FontRef::from_index(bytes, fonts::face_index(face)).ok()?;
    let upem = font
        .metrics(Size::unscaled(), LocationRef::default())
        .units_per_em as f64;
    if upem <= 0.0 {
        return None;
    }
    let gm = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let bb = gm.bounds(skrifa::GlyphId::new(gid))?;
    let k = size / upem;
    Some((
        bb.x_min as f64 * k,
        bb.y_min as f64 * k,
        bb.x_max as f64 * k,
        bb.y_max as f64 * k,
    ))
}

fn is_relation(c: char) -> bool {
    "=<>≤≥≠≈≡≅∼≃∈∉⊂⊃⊆⊇→←↔⇒⇐⇔↦∝∥⊥≪≫∣".contains(c)
}
fn is_binary(c: char) -> bool {
    "+\u{2212}±∓×÷·∗∘∧∨∩∪⊕⊗∖⋅".contains(c)
}
fn is_large_op(c: char) -> bool {
    "∑∏∐⋃⋂⋁⋀⨁⨂⨀".contains(c)
}
fn is_integral(c: char) -> bool {
    "∫∬∭∮".contains(c)
}
fn is_fence(c: char) -> bool {
    "()[]{}|‖⟨⟩⌈⌉⌊⌋".contains(c)
}

/// Mathematical italic mapping for a single-letter identifier.
fn math_italic(c: char) -> char {
    match c {
        'h' => '\u{210E}',
        'a'..='z' => char::from_u32(0x1D44E + (c as u32 - 'a' as u32)).unwrap_or(c),
        'A'..='Z' => char::from_u32(0x1D434 + (c as u32 - 'A' as u32)).unwrap_or(c),
        'α'..='ω' if c != 'ς' => {
            char::from_u32(0x1D6FC + (c as u32 - 'α' as u32)).unwrap_or(c)
        }
        _ => c,
    }
}

fn parse_len(v: &str, em: f64) -> Option<f64> {
    let v = v.trim();
    if let Some(n) = v.strip_suffix("em") {
        return n.trim().parse::<f64>().ok().map(|x| x * em);
    }
    if let Some(n) = v.strip_suffix("pt") {
        return n.trim().parse::<f64>().ok();
    }
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse::<f64>().ok().map(|x| x * 0.75);
    }
    v.parse::<f64>().ok().map(|x| x * em)
}

impl<'a> Cx<'a> {
    fn tag(&self, id: NodeId) -> &str {
        self.dom.nodes[id]
            .kind
            .element()
            .map(|e| e.tag.as_str())
            .unwrap_or("")
    }

    fn attr(&self, id: NodeId, name: &str) -> Option<&str> {
        self.dom.nodes[id].kind.element().and_then(|e| e.attr(name))
    }

    fn elems(&self, id: NodeId) -> Vec<NodeId> {
        self.dom.nodes[id]
            .children
            .iter()
            .copied()
            .filter(|&c| {
                matches!(self.dom.nodes[c].kind, NodeKind::Element(_))
                    && !matches!(self.tag(c), "annotation" | "annotation-xml")
            })
            .collect()
    }

    fn script(&self, st: St) -> St {
        St {
            size: (st.size * SCRIPT_SCALE).max(self.root * 0.5),
            display: false,
        }
    }

    fn text_of(&self, id: NodeId) -> String {
        let raw = self.dom.text_content(id);
        raw.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// A shaped token with glyph-box vertical extents.
    fn token(&self, text: &str, size: f64) -> B {
        if text.is_empty() {
            return B::default();
        }
        let run = shape_word(text, Scalar(size), self.face);
        let mut asc: f64 = 0.0;
        let mut desc: f64 = 0.0;
        let mut pen = 0.0;
        for g in &run.glyphs {
            let face = g.face.unwrap_or(self.face);
            if let Some((_, y0, _, y1)) = glyph_box(face, g.id, size) {
                if y1 > asc {
                    asc = y1;
                }
                if -y0 > desc {
                    desc = -y0;
                }
            }
            pen += g.x_advance.get();
        }
        let _ = pen;
        let w = run.width.get();
        let mut b = B {
            w,
            asc,
            desc,
            ops: Vec::new(),
        };
        b.ops.push(MathOp::Text {
            x: 0.0,
            y: 0.0,
            size,
            face: self.face,
            text: run.text,
            glyphs: run.glyphs,
        });
        b
    }

    fn lay(&self, id: NodeId, st: St) -> B {
        match self.tag(id) {
            "mi" => {
                let t = self.text_of(id);
                let mut it = t.chars();
                let t = match (it.next(), it.next()) {
                    (Some(c), None) => math_italic(c).to_string(),
                    _ => t,
                };
                self.token(&t, st.size)
            }
            "mn" | "mtext" | "ms" => self.token(&self.text_of(id), st.size),
            "mo" => self.lay_mo(id, st, 1.0).0,
            "mspace" => {
                let w = self
                    .attr(id, "width")
                    .and_then(|v| parse_len(v, st.size))
                    .unwrap_or(0.0);
                B {
                    w,
                    ..B::default()
                }
            }
            "mfrac" => self.frac(id, st),
            "msqrt" => {
                let kids = self.elems(id);
                let inner = self.row(&kids, st);
                self.radical(inner, None, st)
            }
            "mroot" => {
                let kids = self.elems(id);
                if kids.len() < 2 {
                    return self.row(&kids, st);
                }
                let inner = self.lay(kids[0], st);
                let ss = St {
                    size: (st.size * 0.5).max(self.root * 0.35),
                    display: false,
                };
                let idx = self.lay(kids[1], ss);
                self.radical(inner, Some(idx), st)
            }
            "msub" | "msup" | "msubsup" => self.scripts(id, st),
            "munder" | "mover" | "munderover" => self.under_over(id, st),
            "mtable" => self.table(id, st),
            "semantics" => {
                let kids = self.elems(id);
                match kids.first() {
                    Some(&k) => self.lay(k, st),
                    None => B::default(),
                }
            }
            "mstyle" => {
                let mut st2 = st;
                match self.attr(id, "displaystyle") {
                    Some("true") => st2.display = true,
                    Some("false") => st2.display = false,
                    _ => {}
                }
                let kids = self.elems(id);
                self.row(&kids, st2)
            }
            _ => {
                let kids = self.elems(id);
                self.row(&kids, st)
            }
        }
    }

    /// Operator text, with the `-` → U+2212 mapping.
    fn mo_text(&self, id: NodeId) -> String {
        self.text_of(id).replace('-', "\u{2212}")
    }

    /// Lay out an `mo`; returns the box and its first char. `mul` scales the size.
    fn lay_mo(&self, id: NodeId, st: St, mul: f64) -> (B, char) {
        let text = self.mo_text(id);
        let c = text.chars().next().unwrap_or(' ');
        let mut size = st.size * mul;
        let mut center = false;
        if st.display && is_large_op(c) {
            size = st.size * 1.4;
            center = true;
        } else if st.display && is_integral(c) {
            size = st.size * 1.6;
            center = true;
        }
        let mut b = self.token(&text, size);
        if center {
            // Center the glyph on the math axis.
            let dy = (b.asc - b.desc) / 2.0 - AXIS * st.size;
            let moved = std::mem::take(&mut b.ops);
            let mut nb = B {
                w: b.w,
                asc: b.asc - dy,
                desc: b.desc + dy,
                ops: Vec::new(),
            };
            nb.put(B { ops: moved, ..B::default() }, 0.0, dy);
            b = nb;
        }
        (b, c)
    }

    fn space_around(&self, id: NodeId, c: char, form: u8, st: St) -> (f64, f64) {
        let em = st.size;
        let mut l = 0.0;
        let mut r = 0.0;
        if form == 1 {
            // infix
            if is_relation(c) {
                l = 0.2778 * em;
                r = 0.2778 * em;
            } else if is_binary(c) {
                l = 0.2222 * em;
                r = 0.2222 * em;
            } else if c == ',' || c == ';' {
                r = 0.1667 * em;
            } else if is_large_op(c) || is_integral(c) {
                l = 0.1667 * em;
                r = 0.1667 * em;
            }
        } else if form == 0 && (is_large_op(c) || is_integral(c)) {
            r = 0.1667 * em;
        }
        if let Some(v) = self.attr(id, "lspace").and_then(|v| parse_len(v, em)) {
            l = v;
        }
        if let Some(v) = self.attr(id, "rspace").and_then(|v| parse_len(v, em)) {
            r = v;
        }
        (l, r)
    }

    /// Horizontal run of `ids` with operator spacing and stretchy fences.
    fn row(&self, ids: &[NodeId], st: St) -> B {
        let n = ids.len();
        if n == 1 && self.tag(ids[0]) != "mo" {
            return self.lay(ids[0], st);
        }
        // First pass: non-operators and non-stretchy operators.
        let mut laid: Vec<Option<B>> = Vec::with_capacity(n);
        let mut chars: Vec<char> = Vec::with_capacity(n);
        let mut asc: f64 = 0.0;
        let mut desc: f64 = 0.0;
        for &id in ids {
            if self.tag(id) == "mo" {
                let (b, c) = self.lay_mo(id, st, 1.0);
                chars.push(c);
                let stretchy = n > 1 && is_fence(c) && self.attr(id, "stretchy") != Some("false");
                if stretchy {
                    laid.push(None);
                } else {
                    asc = asc.max(b.asc);
                    desc = desc.max(b.desc);
                    laid.push(Some(b));
                }
            } else {
                chars.push(' ');
                let b = self.lay(id, st);
                asc = asc.max(b.asc);
                desc = desc.max(b.desc);
                laid.push(Some(b));
            }
        }
        // Stretch fences over the extent of the rest (centered on the axis).
        let axis = AXIS * st.size;
        let half = (asc - axis).max(desc + axis);
        let total = 2.0 * half;
        for (i, &id) in ids.iter().enumerate() {
            if laid[i].is_some() {
                continue;
            }
            let (natural, _) = self.lay_mo(id, st, 1.0);
            let nat_h = natural.asc + natural.desc;
            let mul = if total > 1.2 * st.size && nat_h > 0.0 {
                (total / nat_h).max(1.0)
            } else {
                1.0
            };
            let (b, _) = self.lay_mo(id, st, mul);
            // Center on the axis.
            let dy = (b.asc - b.desc) / 2.0 - axis;
            let mut nb = B {
                w: b.w,
                asc: b.asc - dy,
                desc: b.desc + dy,
                ops: Vec::new(),
            };
            let w = b.w;
            nb.put(b, 0.0, dy);
            nb.w = w;
            laid[i] = Some(nb);
        }
        let mut out = B::default();
        let mut x = 0.0;
        for (i, &id) in ids.iter().enumerate() {
            let b = laid[i].take().unwrap_or_default();
            let mut l = 0.0;
            let mut r = 0.0;
            if self.tag(id) == "mo" {
                let form = if n > 1 && i == 0 {
                    0
                } else if n > 1 && i == n - 1 {
                    2
                } else {
                    1
                };
                let (a, c) = self.space_around(id, chars[i], form, st);
                l = a;
                r = c;
            }
            x += l;
            out.asc = out.asc.max(b.asc);
            out.desc = out.desc.max(b.desc);
            let w = b.w;
            out.put(b, x, 0.0);
            x += w + r;
        }
        out.w = x;
        out
    }

    fn frac(&self, id: NodeId, st: St) -> B {
        let kids = self.elems(id);
        if kids.len() < 2 {
            return self.row(&kids, st);
        }
        let s = st.size;
        let cs = if st.display {
            St {
                size: s,
                display: false,
            }
        } else {
            self.script(st)
        };
        let num = self.lay(kids[0], cs);
        let den = self.lay(kids[1], cs);
        let t = match self.attr(id, "linethickness") {
            Some(v) => parse_len(v, s).unwrap_or(RULE * s),
            None => RULE * s,
        };
        let axis = AXIS * s;
        let gap = if st.display { 3.0 * t } else { t };
        let num_c = if st.display { NUM_UP_D } else { NUM_UP_T } * s;
        let den_c = if st.display { DEN_DN_D } else { DEN_DN_T } * s;
        let up = num_c.max(axis + t / 2.0 + gap + num.desc);
        let dn = den_c.max(den.asc + gap + t / 2.0 - axis);
        let w = num.w.max(den.w);
        let mut b = B {
            w,
            asc: (up + num.asc).max(axis + t / 2.0),
            desc: (dn + den.desc).max(t / 2.0 - axis),
            ops: Vec::new(),
        };
        let (nw, dw) = (num.w, den.w);
        b.put(num, (w - nw) / 2.0, -up);
        b.put(den, (w - dw) / 2.0, dn);
        if t > 0.0 {
            b.rule(0.0, -axis - t / 2.0, w, t);
        }
        b
    }

    fn scripts(&self, id: NodeId, st: St) -> B {
        let kids = self.elems(id);
        if kids.is_empty() {
            return B::default();
        }
        let base = self.lay(kids[0], st);
        let tag = self.tag(id);
        let (sub_id, sup_id) = match tag {
            "msub" => (kids.get(1).copied(), None),
            "msup" => (None, kids.get(1).copied()),
            _ => (kids.get(1).copied(), kids.get(2).copied()),
        };
        let ss = self.script(st);
        let sub = sub_id.map(|i| self.lay(i, ss));
        let sup = sup_id.map(|i| self.lay(i, ss));
        self.attach_scripts(base, sub, sup, st)
    }

    fn attach_scripts(&self, base: B, sub: Option<B>, sup: Option<B>, st: St) -> B {
        let s = st.size;
        let t = RULE * s;
        let scr = self.script(st).size;
        let mut up = (SUP_UP * s).max(base.asc - 0.386 * scr);
        let mut dn = (SUB_DOWN * s).max(base.desc + 0.05 * s);
        if let Some(p) = &sup {
            up = up.max(p.desc + 0.25 * 0.45 * s);
        }
        if let Some(b) = &sub {
            dn = dn.max(b.asc - 0.8 * 0.45 * s);
        }
        if let (Some(p), Some(b)) = (&sup, &sub) {
            let gap = (up - p.desc) - (b.asc - dn);
            if gap < 4.0 * t {
                dn += 4.0 * t - gap;
                let psi = 0.8 * 0.45 * s - (up - p.desc);
                if psi > 0.0 {
                    up += psi;
                    dn -= psi;
                }
            }
        }
        let bw = base.w;
        let sw = sup.as_ref().map_or(0.0, |p| p.w).max(sub.as_ref().map_or(0.0, |p| p.w));
        let mut out = B {
            w: bw + sw + SCRIPT_SPACE * s,
            asc: base.asc,
            desc: base.desc,
            ops: Vec::new(),
        };
        out.put(base, 0.0, 0.0);
        if let Some(p) = sup {
            out.asc = out.asc.max(up + p.asc);
            out.put(p, bw, -up);
        }
        if let Some(b) = sub {
            out.desc = out.desc.max(dn + b.desc);
            out.put(b, bw, dn);
        }
        out
    }

    fn under_over(&self, id: NodeId, st: St) -> B {
        let kids = self.elems(id);
        if kids.is_empty() {
            return B::default();
        }
        let tag = self.tag(id);
        let base_is_op = self
            .dom
            .nodes[kids[0]]
            .kind
            .element()
            .is_some_and(|e| e.tag == "mo");
        let base_ch = if base_is_op {
            self.mo_text(kids[0]).chars().next().unwrap_or(' ')
        } else {
            ' '
        };
        // Movable limits: large operators show limits as scripts outside
        // display style.
        if base_is_op
            && !st.display
            && (is_large_op(base_ch) || is_integral(base_ch))
            && self.attr(kids[0], "movablelimits") != Some("false")
        {
            let base = self.lay(kids[0], st);
            let ss = self.script(st);
            let (u, o) = match tag {
                "munder" => (kids.get(1).copied(), None),
                "mover" => (None, kids.get(1).copied()),
                _ => (kids.get(1).copied(), kids.get(2).copied()),
            };
            let sub = u.map(|i| self.lay(i, ss));
            let sup = o.map(|i| self.lay(i, ss));
            return self.attach_scripts(base, sub, sup, st);
        }
        let base = self.lay(kids[0], st);
        let accent = self.attr(id, "accent") == Some("true");
        let ls = if accent {
            St {
                size: st.size,
                display: false,
            }
        } else {
            self.script(st)
        };
        let (u, o) = match tag {
            "munder" => (kids.get(1).copied(), None),
            "mover" => (None, kids.get(1).copied()),
            _ => (kids.get(1).copied(), kids.get(2).copied()),
        };
        let under = u.map(|i| self.lay(i, ls));
        let over = o.map(|i| self.lay(i, ls));
        let gap = LIMIT_GAP * st.size;
        let mut w = base.w;
        if let Some(b) = &under {
            w = w.max(b.w);
        }
        if let Some(b) = &over {
            w = w.max(b.w);
        }
        let mut out = B {
            w,
            asc: base.asc,
            desc: base.desc,
            ops: Vec::new(),
        };
        let bw = base.w;
        let (basc, bdesc) = (base.asc, base.desc);
        out.put(base, (w - bw) / 2.0, 0.0);
        if let Some(b) = over {
            let y = -(basc + gap + b.desc);
            out.asc = basc + gap + b.desc + b.asc;
            let bwid = b.w;
            out.put(b, (w - bwid) / 2.0, y);
        }
        if let Some(b) = under {
            let y = bdesc + gap + b.asc;
            out.desc = bdesc + gap + b.asc + b.desc;
            let bwid = b.w;
            out.put(b, (w - bwid) / 2.0, y);
        }
        out
    }

    /// Square/nth root around `inner`.
    fn radical(&self, inner: B, index: Option<B>, st: St) -> B {
        let s = st.size;
        let t = RULE * s;
        let gap = if st.display { 0.15 * s } else { 0.05 * s };
        let need = inner.asc + inner.desc + gap + t;
        // Natural radical sign metrics at the current size.
        let probe = shape_word("\u{221A}", Scalar(s), self.face);
        let Some(g0) = probe.glyphs.first() else {
            return inner;
        };
        let gface = g0.face.unwrap_or(self.face);
        let Some((_, y0, _, y1)) = glyph_box(gface, g0.id, s) else {
            return inner;
        };
        let nat = y1 - y0;
        let k = if nat > 0.0 { (need / nat).max(1.0) } else { 1.0 };
        let gs = s * k;
        let sign = shape_word("\u{221A}", Scalar(gs), self.face);
        let Some(g) = sign.glyphs.first().cloned() else {
            return inner;
        };
        let gface = g.face.unwrap_or(self.face);
        let (_, gy0, gx1, gy1) = glyph_box(gface, g.id, gs).unwrap_or((0.0, 0.0, g.x_advance.get(), gs));
        let bar_top = -(inner.asc + gap + t);
        let baseline_y = bar_top + gy1;
        let sign_bottom = baseline_y - gy0;
        let mut x0 = 0.0;
        let mut out = B::default();
        let mut asc = inner.asc + gap + t;
        let mut desc = inner.desc.max(sign_bottom);
        if let Some(idx) = &index {
            let h = asc + desc;
            let raise = 0.6 * h;
            let iy = sign_bottom - raise;
            x0 = (idx.w - 0.17 * s).max(0.0);
            asc = asc.max(-iy + idx.asc);
            let ib = idx.clone();
            out.put(ib, 0.0, iy);
        }
        out.ops.push(MathOp::Text {
            x: x0,
            y: baseline_y,
            size: gs,
            face: self.face,
            text: sign.text.clone(),
            glyphs: sign.glyphs.clone(),
        });
        let bar_x = x0 + gx1 - 0.3;
        let iw = inner.w;
        out.rule(bar_x, bar_top, iw + (x0 + gx1 - bar_x), t);
        let content_x = x0 + gx1.max(sign.width.get() * 0.9);
        let (ia, id_) = (inner.asc, inner.desc);
        let _ = (ia, id_);
        out.put(inner, content_x, 0.0);
        out.w = content_x + iw;
        out.asc = asc;
        out.desc = desc;
        // The bar must span the content: recompute its width from the final
        // content origin.
        for op in out.ops.iter_mut() {
            if let MathOp::Rule { x, w, y, .. } = op {
                if (*y - bar_top).abs() < 1e-9 && *x == bar_x {
                    *w = content_x + iw - bar_x;
                }
            }
        }
        desc = out.desc;
        let _ = desc;
        out
    }

    fn table(&self, id: NodeId, st: St) -> B {
        let rows_ids = self.elems(id);
        let mut cells: Vec<Vec<B>> = Vec::new();
        for r in rows_ids {
            let row_cells: Vec<NodeId> = if matches!(self.tag(r), "mtr" | "mlabeledtr") {
                self.elems(r)
            } else {
                vec![r]
            };
            let mut row = Vec::new();
            for c in row_cells {
                let cst = St {
                    size: st.size,
                    display: false,
                };
                if self.tag(c) == "mtd" {
                    let kids = self.elems(c);
                    row.push(self.row(&kids, cst));
                } else {
                    row.push(self.lay(c, cst));
                }
            }
            cells.push(row);
        }
        let ncols = cells.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut colw = vec![0.0f64; ncols];
        let mut rasc = vec![0.0f64; cells.len()];
        let mut rdesc = vec![0.0f64; cells.len()];
        for (ri, r) in cells.iter().enumerate() {
            for (ci, c) in r.iter().enumerate() {
                colw[ci] = colw[ci].max(c.w);
                rasc[ri] = rasc[ri].max(c.asc);
                rdesc[ri] = rdesc[ri].max(c.desc);
            }
        }
        let cgap = 0.8 * st.size;
        let rgap = 0.4 * st.size;
        let total_w: f64 =
            colw.iter().sum::<f64>() + cgap * (ncols.saturating_sub(1)) as f64;
        let total_h: f64 = (0..cells.len()).map(|i| rasc[i] + rdesc[i]).sum::<f64>()
            + rgap * (cells.len().saturating_sub(1)) as f64;
        let axis = AXIS * st.size;
        let top = -(axis + total_h / 2.0);
        let mut out = B {
            w: total_w,
            asc: axis + total_h / 2.0,
            desc: total_h / 2.0 - axis,
            ops: Vec::new(),
        };
        let mut y = top;
        for (ri, r) in cells.into_iter().enumerate() {
            let base_y = y + rasc[ri];
            let mut x = 0.0;
            for (ci, c) in r.into_iter().enumerate() {
                let cw = c.w;
                out.put(c, x + (colw[ci] - cw) / 2.0, base_y);
                x += colw[ci] + cgap;
            }
            y += rasc[ri] + rdesc[ri] + rgap;
        }
        out
    }
}
