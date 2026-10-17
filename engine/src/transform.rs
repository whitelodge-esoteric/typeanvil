// SPDX-License-Identifier: AGPL-3.0-only

//! CSS 2D transforms (css-transforms-1, CORE-481).
//!
//! `transform` and `transform-origin` are parsed here, not read from stylo:
//! the stylo seam ignores inline `style=""`, and the WPT fixtures set
//! `transform` inline. The parsed operation list rides on `ComputedStyle`;
//! layout resolves it against the border box into an [`Affine`] and stores
//! it on the fragment; the PDF emitter paints the subtree under it.
//!
//! Spec: `docs/specifications/css-transforms.spec.md`.

/// A 2D affine matrix `[a, b, c, d, e, f]`: `x' = a*x + c*y + e`,
/// `y' = b*x + d*y + f` (the CSS `matrix()` / PDF `cm` layout).
pub type Affine = [f64; 6];

/// The identity matrix.
pub const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// A length-or-percentage, kept unresolved until the box size and font size
/// are known. Absolute part in points, `em` part as a font-size multiple,
/// `pct` as a fraction of the reference size.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LenPct {
    pub pt: f64,
    pub em: f64,
    pub pct: f64,
}

impl LenPct {
    pub const ZERO: LenPct = LenPct { pt: 0.0, em: 0.0, pct: 0.0 };
    pub const HALF: LenPct = LenPct { pt: 0.0, em: 0.0, pct: 0.5 };

    /// Resolve to points. `basis` is the reference size in points.
    pub fn resolve(&self, font_pt: f64, basis: f64) -> f64 {
        self.pt + self.em * font_pt + self.pct * basis
    }
}

/// One transform function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformOp {
    Translate(LenPct, LenPct),
    Scale(f64, f64),
    /// Radians.
    Rotate(f64),
    /// Radians (x angle, y angle).
    Skew(f64, f64),
    /// `matrix(a, b, c, d, e, f)` with `e`/`f` already in points.
    Matrix(Affine),
}

/// `outer ∘ inner`: apply `inner` first, then `outer`.
pub fn mul(outer: &Affine, inner: &Affine) -> Affine {
    let [a1, b1, c1, d1, e1, f1] = *outer;
    let [a2, b2, c2, d2, e2, f2] = *inner;
    [
        a1 * a2 + c1 * b2,
        b1 * a2 + d1 * b2,
        a1 * c2 + c1 * d2,
        b1 * c2 + d1 * d2,
        a1 * e2 + c1 * f2 + e1,
        b1 * e2 + d1 * f2 + f1,
    ]
}

/// Map a point through `m`.
pub fn apply(m: &Affine, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// The matrix of a function list for a border box of `w` × `h` points.
/// The first function is the outermost (css-transforms-1 §13).
pub fn build(ops: &[TransformOp], font_pt: f64, w: f64, h: f64) -> Affine {
    let mut m = IDENTITY;
    for op in ops {
        let o = match *op {
            TransformOp::Translate(x, y) => {
                [1.0, 0.0, 0.0, 1.0, x.resolve(font_pt, w), y.resolve(font_pt, h)]
            }
            TransformOp::Scale(sx, sy) => [sx, 0.0, 0.0, sy, 0.0, 0.0],
            TransformOp::Rotate(t) => [t.cos(), t.sin(), -t.sin(), t.cos(), 0.0, 0.0],
            TransformOp::Skew(ax, ay) => [1.0, ay.tan(), ax.tan(), 1.0, 0.0, 0.0],
            TransformOp::Matrix(mm) => mm,
        };
        m = mul(&m, &o);
    }
    m
}

/// Conjugate `m` with a translation to the origin `(ox, oy)`:
/// `p -> m(p - o) + o` (css-transforms-1 §6 `transform-origin`).
pub fn about_origin(m: &Affine, ox: f64, oy: f64) -> Affine {
    [
        m[0],
        m[1],
        m[2],
        m[3],
        m[4] + ox - (m[0] * ox + m[2] * oy),
        m[5] + oy - (m[1] * ox + m[3] * oy),
    ]
}

/// A fragment-local matrix (origin = the fragment's top-left at `(fx, fy)`)
/// expressed in page coordinates: `T(f) ∘ m ∘ T(-f)`.
pub fn to_abs(m: &Affine, fx: f64, fy: f64) -> Affine {
    about_frame(m, fx, fy)
}

/// The inverse of [`to_abs`]: page-space matrix to fragment-local.
pub fn to_local(m: &Affine, fx: f64, fy: f64) -> Affine {
    about_frame(m, -fx, -fy)
}

/// `T(d) ∘ m ∘ T(-d)` for `d = (dx, dy)`.
fn about_frame(m: &Affine, dx: f64, dy: f64) -> Affine {
    let t_in = [1.0, 0.0, 0.0, 1.0, -dx, -dy];
    let t_out = [1.0, 0.0, 0.0, 1.0, dx, dy];
    mul(&t_out, &mul(m, &t_in))
}

/// True when `m` is the identity (within rounding).
pub fn is_identity(m: &Affine) -> bool {
    m.iter().zip(IDENTITY.iter()).all(|(a, b)| (a - b).abs() < 1e-9)
}

fn parse_number(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Parse a length or percentage token.
pub fn parse_len(tok: &str) -> Option<LenPct> {
    let t = tok.trim().to_ascii_lowercase();
    if t.is_empty() {
        return None;
    }
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(t.len());
    // Exponent notation is not supported: the `e` of `em` starts a unit.
    let (num_s, unit) = (&t[..split], &t[split..]);
    let n = parse_number(num_s)?;
    let mut out = LenPct::ZERO;
    match unit {
        "" if n == 0.0 => {}
        "px" => out.pt = n * 0.75,
        "pt" => out.pt = n,
        "pc" => out.pt = n * 12.0,
        "in" => out.pt = n * 72.0,
        "cm" => out.pt = n * 72.0 / 2.54,
        "mm" => out.pt = n * 72.0 / 25.4,
        "q" => out.pt = n * 72.0 / 101.6,
        "em" => out.em = n,
        "rem" => out.pt = n * 12.0,
        "%" => out.pct = n / 100.0,
        _ => return None,
    }
    Some(out)
}

/// Parse an angle token to radians. Unitless `0` is accepted.
pub fn parse_angle(tok: &str) -> Option<f64> {
    let t = tok.trim().to_ascii_lowercase();
    for (unit, to_rad) in [
        ("deg", std::f64::consts::PI / 180.0),
        ("grad", std::f64::consts::PI / 200.0),
        ("rad", 1.0),
        ("turn", std::f64::consts::TAU),
    ] {
        if let Some(n) = t.strip_suffix(unit) {
            return parse_number(n).map(|v| v * to_rad);
        }
    }
    (parse_number(&t)? == 0.0).then_some(0.0)
}

fn args(body: &str) -> Vec<&str> {
    body.split(',').map(str::trim).collect()
}

/// Parse a `transform` value. `Some(vec![])` is `none`; `None` is an
/// invalid value (the declaration is ignored). 3D functions are invalid.
pub fn parse_transform(value: &str) -> Option<Vec<TransformOp>> {
    let value = value.trim();
    let value = value.strip_suffix("!important").map_or(value, str::trim);
    if value.eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    let mut ops = Vec::new();
    let mut rest = value;
    while !rest.trim().is_empty() {
        rest = rest.trim_start();
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        if close < open {
            return None;
        }
        let name = rest[..open].trim().to_ascii_lowercase();
        let a = args(&rest[open + 1..close]);
        rest = &rest[close + 1..];
        let op = match (name.as_str(), a.len()) {
            ("translate", 1) => TransformOp::Translate(parse_len(a[0])?, LenPct::ZERO),
            ("translate", 2) => TransformOp::Translate(parse_len(a[0])?, parse_len(a[1])?),
            ("translatex", 1) => TransformOp::Translate(parse_len(a[0])?, LenPct::ZERO),
            ("translatey", 1) => TransformOp::Translate(LenPct::ZERO, parse_len(a[0])?),
            ("scale", 1) => {
                let s = parse_number(a[0])?;
                TransformOp::Scale(s, s)
            }
            ("scale", 2) => TransformOp::Scale(parse_number(a[0])?, parse_number(a[1])?),
            ("scalex", 1) => TransformOp::Scale(parse_number(a[0])?, 1.0),
            ("scaley", 1) => TransformOp::Scale(1.0, parse_number(a[0])?),
            ("rotate" | "rotatez", 1) => TransformOp::Rotate(parse_angle(a[0])?),
            ("skew", 1) => TransformOp::Skew(parse_angle(a[0])?, 0.0),
            ("skew", 2) => TransformOp::Skew(parse_angle(a[0])?, parse_angle(a[1])?),
            ("skewx", 1) => TransformOp::Skew(parse_angle(a[0])?, 0.0),
            ("skewy", 1) => TransformOp::Skew(0.0, parse_angle(a[0])?),
            ("matrix", 6) => {
                let mut m = [0.0; 6];
                for (slot, tok) in m.iter_mut().zip(a.iter()) {
                    *slot = parse_number(tok)?;
                }
                // e and f are CSS pixels.
                m[4] *= 0.75;
                m[5] *= 0.75;
                TransformOp::Matrix(m)
            }
            _ => return None,
        };
        ops.push(op);
    }
    if ops.is_empty() {
        None
    } else {
        Some(ops)
    }
}

/// Parse `transform-origin` (x and y components; a third z token is
/// accepted and ignored).
pub fn parse_origin(value: &str) -> Option<(LenPct, LenPct)> {
    let value = value.trim();
    let value = value.strip_suffix("!important").map_or(value, str::trim);
    let toks: Vec<&str> = value.split_whitespace().collect();
    if toks.is_empty() || toks.len() > 3 {
        return None;
    }
    #[derive(Clone, Copy, PartialEq)]
    enum K {
        Left,
        Right,
        Top,
        Bottom,
        Center,
        Len(LenPct),
    }
    let kind = |t: &str| -> Option<K> {
        match t.to_ascii_lowercase().as_str() {
            "left" => Some(K::Left),
            "right" => Some(K::Right),
            "top" => Some(K::Top),
            "bottom" => Some(K::Bottom),
            "center" => Some(K::Center),
            other => parse_len(other).map(K::Len),
        }
    };
    let first = kind(toks[0])?;
    let second = if toks.len() >= 2 { kind(toks[1])? } else { K::Center };
    // Keyword order is free when both are keywords: `top left` == `left top`.
    let (hx, vy) = match (first, second) {
        (K::Top | K::Bottom, _) | (_, K::Left | K::Right) => (second, first),
        _ => (first, second),
    };
    let x = match hx {
        K::Left => LenPct::ZERO,
        K::Right => LenPct { pct: 1.0, ..LenPct::ZERO },
        K::Center => LenPct::HALF,
        K::Len(l) => l,
        K::Top | K::Bottom => return None,
    };
    let y = match vy {
        K::Top => LenPct::ZERO,
        K::Bottom => LenPct { pct: 1.0, ..LenPct::ZERO },
        K::Center => LenPct::HALF,
        K::Len(l) => l,
        K::Left | K::Right => return None,
    };
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_px_is_points() {
        let ops = parse_transform("translateX(49px)").unwrap();
        let m = build(&ops, 12.0, 0.0, 0.0);
        assert!((m[4] - 36.75).abs() < 1e-9 && m[5] == 0.0);
    }

    #[test]
    fn list_order_first_is_outermost() {
        // translate then scale: scale applies to the point first.
        let ops = parse_transform("translate(10px, 0) scale(2)").unwrap();
        let m = build(&ops, 12.0, 0.0, 0.0);
        let (x, _) = apply(&m, 1.0, 0.0);
        assert!((x - (7.5 + 2.0)).abs() < 1e-9);
    }

    #[test]
    fn rotate_quarter_turn() {
        let ops = parse_transform("rotate(90deg)").unwrap();
        let m = build(&ops, 12.0, 0.0, 0.0);
        let (x, y) = apply(&m, 1.0, 0.0);
        assert!(x.abs() < 1e-9 && (y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn origin_center_keeps_center_fixed() {
        let ops = parse_transform("scale(2)").unwrap();
        let m = about_origin(&build(&ops, 12.0, 0.0, 0.0), 50.0, 25.0);
        let (x, y) = apply(&m, 50.0, 25.0);
        assert!((x - 50.0).abs() < 1e-9 && (y - 25.0).abs() < 1e-9);
    }

    #[test]
    fn invalid_and_none() {
        assert_eq!(parse_transform("none"), Some(Vec::new()));
        assert!(parse_transform("translate3d(1px,2px,3px)").is_none());
        assert!(parse_transform("bogus(1)").is_none());
        assert!(parse_transform("translate(1foo)").is_none());
    }

    #[test]
    fn origin_keywords() {
        let (x, y) = parse_origin("top left").unwrap();
        assert_eq!((x, y), (LenPct::ZERO, LenPct::ZERO));
        let (x, y) = parse_origin("right 10px").unwrap();
        assert_eq!(x.pct, 1.0);
        assert!((y.pt - 7.5).abs() < 1e-9);
    }
}
