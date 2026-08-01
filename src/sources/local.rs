//! Local filesystem source: scans folders for audio files.

use std::path::PathBuf;

use crate::config::expand_path;
use crate::library::Track;
use crate::library::scanner::Scanner;

/// Scans one or more local folders for playable audio tracks.
#[derive(Debug, Clone)]
pub struct LocalSource {
    paths: Vec<PathBuf>,
}

impl LocalSource {
    /// Creates a source that scans the given paths.
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self { paths }
    }

    /// Scans every configured path (with `~` expanded) into [`Track`]s.
    pub fn scan(&self) -> Vec<Track> {
        let paths: Vec<PathBuf> = self.paths.iter().map(|path| expand_path(path)).collect();
        Scanner::new().scan_paths(&paths)
    }
}
