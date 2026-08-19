//! Font-face selection and path mapping.

use crate::css::FontStyle;

/// The four bundled Arial faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFace {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

/// Resolve a font face from computed weight/style.
pub fn face_for(weight: f32, style: FontStyle) -> FontFace {
    let italic = matches!(style, FontStyle::Italic);
    if weight >= 600.0 && italic {
        FontFace::BoldItalic
    } else if weight >= 600.0 {
        FontFace::Bold
    } else if italic {
        FontFace::Italic
    } else {
        FontFace::Regular
    }
}

/// The filesystem path for a bundled face.
pub fn face_path(face: FontFace) -> &'static str {
    match face {
        FontFace::Regular => "/System/Library/Fonts/Supplemental/Arial.ttf",
        FontFace::Bold => "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        FontFace::Italic => "/System/Library/Fonts/Supplemental/Arial Italic.ttf",
        FontFace::BoldItalic => "/System/Library/Fonts/Supplemental/Arial Bold Italic.ttf",
    }
}
