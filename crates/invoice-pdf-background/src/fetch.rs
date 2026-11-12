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

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for FetchError {}

/// Reads `file://` Backgrounds from disk, and `gs://` / `https://`
/// Backgrounds over the network when built with the `gcs` / `https` features.
///
/// Network fetches never follow redirects, time out after 30 s and accept at
/// most 32 MiB. `gs://` objects are read through the GCS JSON API with an
/// access token from the GCE metadata server.
#[derive(Debug, Default)]
pub struct DefaultFetcher {
    #[cfg(feature = "gcs")]
    gcs: crate::gcs::GcsClient,
    #[cfg(feature = "https")]
    https: HttpsClient,
}

#[cfg(feature = "https")]
struct HttpsClient(ureq::Agent);

#[cfg(feature = "https")]
impl Default for HttpsClient {
    fn default() -> Self {
        Self(crate::http::agent(true))
    }
}

#[cfg(feature = "https")]
impl std::fmt::Debug for HttpsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpsClient").finish_non_exhaustive()
    }
}

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
            #[cfg(feature = "gcs")]
            Source::Gcs { bucket, object } => self.gcs.fetch(bucket, object),
            #[cfg(not(feature = "gcs"))]
            Source::Gcs { .. } => Err(FetchError::Failed(
                "gs:// Backgrounds need the `gcs` feature of invoice-pdf-background".into(),
            )),
            #[cfg(feature = "https")]
            Source::Https { url, .. } => crate::http::get(&self.https.0, url, &[]),
            #[cfg(not(feature = "https"))]
            Source::Https { .. } => Err(FetchError::Failed(
                "https:// Backgrounds need the `https` feature of invoice-pdf-background".into(),
            )),
        }
    }
}
