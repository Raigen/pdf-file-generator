//! Resolving a Background reference into a checked [`Source`].

use std::path::PathBuf;

use crate::config::BackgroundConfig;
use crate::error::BackgroundError;

/// Where a Background is fetched from, after the reference passed every check.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    /// An object in Google Cloud Storage.
    Gcs { bucket: String, object: String },
    /// A canonical path inside the configured base directory.
    File(PathBuf),
    /// An `https://` URL on an allow-listed host.
    Https { url: String, host: String },
}

impl Source {
    /// Resolves `reference` against `config`.
    ///
    /// `file://` references are relative to the base directory; they are
    /// canonicalized (following symlinks), so this touches the filesystem.
    pub fn resolve(reference: &str, config: &BackgroundConfig) -> Result<Self, BackgroundError> {
        let _ = (reference, config);
        unimplemented!("issue #5")
    }
}
