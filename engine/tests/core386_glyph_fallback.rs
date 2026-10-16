//! CORE-386: a codepoint missing from the primary face is shaped with the
//! first installed face that covers it, not painted as `.notdef` (gid 0).

use typeanvil::fonts::FACE_REGULAR;
use typeanvil::geom::Scalar;
use typeanvil::typography::shape_word;

#[test]
fn uncovered_codepoint_gets_fallback_face_and_real_glyph() {
    // U+207B (superscript minus) is absent from Arial and Liberation Sans.
    let word = "s\u{207B}\u{00B9}";
    let run = shape_word(word, Scalar(12.0), FACE_REGULAR);
    let fallback: Vec<_> = run.glyphs.iter().filter(|g| g.face.is_some()).collect();
    assert_eq!(fallback.len(), 1, "only U+207B needs a fallback face");
    assert_eq!(&word[fallback[0].range.clone()], "\u{207B}");
    for g in &run.glyphs {
        assert_ne!(g.id, 0, "{word:?}: .notdef glyph in output");
        assert!(g.range.start < g.range.end && g.range.end <= word.len());
    }
    assert!(run.width.get() > 0.0);
}

#[test]
fn symbol_never_renders_notdef() {
    // U+2665 is covered by some primary faces and not others; either way the
    // result must not be .notdef.
    let run = shape_word("\u{2665}", Scalar(12.0), FACE_REGULAR);
    assert!(run.glyphs.iter().all(|g| g.id != 0));
}

#[test]
fn covered_text_never_uses_fallback() {
    let run = shape_word("Hello, world", Scalar(12.0), FACE_REGULAR);
    assert!(run.glyphs.iter().all(|g| g.face.is_none()));
}
