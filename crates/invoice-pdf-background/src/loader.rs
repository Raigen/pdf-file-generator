//! The [`BackgroundLoader`]: resolve, fetch, decode, cache.

use std::sync::Arc;

use crate::config::BackgroundConfig;
use crate::decode::Background;
use crate::error::BackgroundError;
use crate::fetch::{DefaultFetcher, Fetch};

/// Loads Backgrounds by reference and caches the decoded result.
///
/// One loader is meant to live for the whole process (per instance) and be
/// shared between requests. [`BackgroundLoader::load`] blocks while fetching
/// and decoding; call it from a blocking context (e.g. `spawn_blocking`).
pub struct BackgroundLoader {
    config: BackgroundConfig,
    fetcher: Arc<dyn Fetch>,
}

impl BackgroundLoader {
    /// A loader using `fetcher` for all sources.
    pub fn new(config: BackgroundConfig, fetcher: impl Fetch + 'static) -> Self {
        Self {
            config,
            fetcher: Arc::new(fetcher),
        }
    }

    /// A loader using the [`DefaultFetcher`].
    pub fn with_default_fetcher(config: BackgroundConfig) -> Self {
        Self::new(config, DefaultFetcher::new())
    }

    /// The configuration this loader resolves references with.
    pub fn config(&self) -> &BackgroundConfig {
        &self.config
    }

    /// Returns the decoded Background for `reference`, from the cache if present.
    pub fn load(&self, reference: &str) -> Result<Arc<Background>, BackgroundError> {
        let _ = (reference, &self.fetcher);
        unimplemented!("issue #5")
    }
}
