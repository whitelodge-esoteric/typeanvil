//! Typeanvil engine library.
//!
//! The CLI (`src/main.rs`) is a thin driver over these modules. Exposing them
//! as a library lets integration tests (`tests/`) assert on the fragment tree
//! directly — pagination, break tokens, orphans/widows — instead of only
//! observing PDF bytes through the CLI.

pub mod css;
pub mod dom;
pub mod frag;
pub mod geom;
pub mod layout;
pub mod pdf;
pub mod stylo_dom;
