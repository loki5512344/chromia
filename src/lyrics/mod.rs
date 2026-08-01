//! Lyrics: LRC parsing and the lrclib.net API client.

pub mod lrclib;

/// Convenience re-exports for consumers; temporarily unused while the UI
/// agents wire the lyrics widget up in parallel.
#[allow(unused_imports)]
pub use lrclib::{Lrclib, LyricLine, Lyrics};
