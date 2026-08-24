//! Typeanvil engine library.
//!
//! The CLI (`src/main.rs`) is a thin driver over these modules. Exposing them
//! as a library lets integration tests (`tests/`) assert on the fragment tree
//! directly — pagination, break tokens, orphans/widows — instead of only
//! observing PDF bytes through the CLI.

pub mod css;
pub mod dom;
pub mod fonts;
pub mod frag;
pub mod geom;
pub mod images;
pub mod layout;
pub mod licensing;
pub mod metadata;
pub mod paged;
pub mod pdf;
pub mod stylo_dom;
pub mod table;
pub mod tags;
pub mod typography;
