//! Fetching the bytes of a resolved Background [`Source`].

use std::sync::Arc;

use crate::source::Source;

/// Fetches the raw bytes of a Background. Implemented by [`DefaultFetcher`]
/// and by fakes in tests.
pub trait Fetch: Send + Sync {
    fn fetch(&self, source: &Source) -> Result<Vec<u8>, FetchError>;
}

/// Lets a caller keep a handle on a fetcher that a loader also owns.
impl<F: Fetch + ?Sized> Fetch for Arc<F> {
    fn fetch(&self, source: &Source) -> Result<Vec<u8>, FetchError> {
        (**self).fetch(source)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// Nothing exists at the source.
    NotFound,
    /// Any other failure, described for the error message.
    Failed(String),
}

/// Reads `file://` Backgrounds from disk, and `gs://` / `https://`
/// Backgrounds over the network when built with the `gcs` / `https` features.
#[derive(Debug, Default)]
pub struct DefaultFetcher {}

impl DefaultFetcher {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Fetch for DefaultFetcher {
    fn fetch(&self, source: &Source) -> Result<Vec<u8>, FetchError> {
        match source {
            Source::File(path) => std::fs::read(path).map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => FetchError::NotFound,
                _ => FetchError::Failed(error.to_string()),
            }),
            Source::Gcs { .. } => Err(FetchError::Failed(
                "gs:// Backgrounds need the `gcs` feature of invoice-pdf-background".into(),
            )),
            Source::Https { .. } => Err(FetchError::Failed(
                "https:// Backgrounds need the `https` feature of invoice-pdf-background".into(),
            )),
        }
    }
}
