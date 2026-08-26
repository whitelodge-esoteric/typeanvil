// SPDX-License-Identifier: AGPL-3.0-only

//! Arithmetic and geometry primitives.
//!
//! ## Arithmetic decision (walking skeleton)
//!
//! Typeanvil's core product promise is determinism: identical input → identical
//! PDF bytes, forever, across platforms. The open question was fixed-point
//! (TeX-style scaled points) vs. strictly-controlled `f64`.
//!
//! **Decision (2026-08-16): strictly-controlled `f64` wrapped in a [`Scalar`]
//! newtype (Typst-style).** Rationale:
//! - Rust/LLVM do not fuse multiply-add or reassociate floats unless explicitly
//!   asked (no `-ffast-math` equivalent is on by default; we never pass
//!   `-C target-feature=+fma`-driven contraction or fast-math flags).
//! - Routing all layout arithmetic through `Scalar` gives us a single choke
//!   point: if cross-platform bit-divergence ever appears, we can swap the
//!   inner representation to fixed-point behind this same API without touching
//!   layout code.
//! - Fixed-point is deferred until a concrete divergence is observed.
//!
//! Everything downstream measures in **PostScript points** (1 pt = 1/72 in).

/// A strictly-controlled `f64`. All layout arithmetic flows through this type so
/// the arithmetic model is swappable behind one boundary (see module docs).
///
/// We deliberately do *not* implement `Ord`/`Hash`; use the explicit helpers.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Scalar(pub f64);

impl Scalar {
    pub const ZERO: Scalar = Scalar(0.0);

    #[inline]
    pub fn get(self) -> f64 {
        self.0
    }

    /// Value as `f32`, for the PDF backend (krilla takes `f32`).
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32
    }
}

impl From<f64> for Scalar {
    #[inline]
    fn from(v: f64) -> Self {
        Scalar(v)
    }
}

impl std::ops::Add for Scalar {
    type Output = Scalar;
    #[inline]
    fn add(self, rhs: Scalar) -> Scalar {
        Scalar(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Scalar {
    type Output = Scalar;
    #[inline]
    fn sub(self, rhs: Scalar) -> Scalar {
        Scalar(self.0 - rhs.0)
    }
}

impl std::ops::Mul<f64> for Scalar {
    type Output = Scalar;
    #[inline]
    fn mul(self, rhs: f64) -> Scalar {
        Scalar(self.0 * rhs)
    }
}

impl std::ops::AddAssign for Scalar {
    #[inline]
    fn add_assign(&mut self, rhs: Scalar) {
        self.0 += rhs.0;
    }
}

/// Convert a CSS length in `px` to points. CSS defines 1 px = 1/96 in, so
/// 1 px = 72/96 pt = 0.75 pt.
#[inline]
pub fn px_to_pt(px: f64) -> Scalar {
    Scalar(px * 0.75)
}

/// A point in PDF user space (points), origin top-left in *our* coordinate
/// system (y grows downward). The PDF backend flips y at draw time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: Scalar,
    pub y: Scalar,
}

impl Point {
    #[inline]
    pub fn new(x: Scalar, y: Scalar) -> Self {
        Point { x, y }
    }
}

/// An axis-aligned rectangle in points (top-left origin, y down).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: Scalar,
    pub y: Scalar,
    pub width: Scalar,
    pub height: Scalar,
}

impl Rect {
    #[inline]
    pub fn new(x: Scalar, y: Scalar, width: Scalar, height: Scalar) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
}

/// Page geometry parsed from the CLI, all in points.
#[derive(Clone, Copy, Debug)]
pub struct PageGeometry {
    pub width: Scalar,
    pub height: Scalar,
    pub margin_top: Scalar,
    pub margin_right: Scalar,
    pub margin_bottom: Scalar,
    pub margin_left: Scalar,
}

impl PageGeometry {
    /// The content box: page size minus margins.
    pub fn content_rect(&self) -> Rect {
        Rect::new(
            self.margin_left,
            self.margin_top,
            self.width - self.margin_left - self.margin_right,
            self.height - self.margin_top - self.margin_bottom,
        )
    }
}
