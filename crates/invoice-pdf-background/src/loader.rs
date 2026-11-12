//! The [`BackgroundLoader`]: resolve, fetch, decode, cache.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::config::BackgroundConfig;
use crate::decode::Background;
use crate::error::BackgroundError;
use crate::fetch::{DefaultFetcher, Fetch, FetchError};
use crate::source::Source;

type LoadResult = Result<Arc<Background>, BackgroundError>;

/// One load of a reference in progress; every concurrent caller waits on the
/// same cell, and only the first one runs the load.
type Flight = Arc<OnceLock<LoadResult>>;

/// Loads Backgrounds by reference and caches the decoded result.
///
/// One loader is meant to live for the whole process (per instance) and be
/// shared between requests. The cache is keyed by the reference string and
/// never revalidated, since references are immutable by contract. It is an
/// LRU bounded by [`BackgroundConfig::cache_max_bytes`], counted as
/// [`Background::byte_size`]; a Background larger than the bound is returned
/// but not cached. Failed loads are not cached.
///
/// Concurrent loads of the same reference are coalesced: one caller fetches
/// and decodes, the others wait for its result. Loads of different
/// references do not wait for each other.
///
/// [`BackgroundLoader::load`] blocks while fetching and decoding; call it
/// from a blocking context (e.g. `spawn_blocking`).
pub struct BackgroundLoader {
    config: BackgroundConfig,
    fetcher: Arc<dyn Fetch>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    cache: Lru,
    in_flight: HashMap<String, Flight>,
}

impl BackgroundLoader {
    /// A loader using `fetcher` for all sources.
    pub fn new(config: BackgroundConfig, fetcher: impl Fetch + 'static) -> Self {
        let cache = Lru::new(config.cache_max_bytes);
        Self {
            config,
            fetcher: Arc::new(fetcher),
            state: Mutex::new(State {
                cache,
                in_flight: HashMap::new(),
            }),
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
    pub fn load(&self, reference: &str) -> LoadResult {
        let flight = {
            let mut state = self.state();
            if let Some(background) = state.cache.get(reference) {
                return Ok(background);
            }
            Arc::clone(state.in_flight.entry(reference.to_owned()).or_default())
        };

        let result = flight.get_or_init(|| self.fetch_and_decode(reference).map(Arc::new));

        let mut state = self.state();
        // The first finished caller retires the flight and caches a success;
        // the others find it gone (or replaced by a newer flight) and skip this.
        if state
            .in_flight
            .get(reference)
            .is_some_and(|current| Arc::ptr_eq(current, &flight))
        {
            state.in_flight.remove(reference);
            if let Ok(background) = result {
                state.cache.insert(reference, Arc::clone(background));
            }
        }
        result.clone()
    }

    fn fetch_and_decode(&self, reference: &str) -> Result<Background, BackgroundError> {
        let source = Source::resolve(reference, &self.config)?;
        let bytes = self.fetcher.fetch(&source).map_err(|error| match error {
            FetchError::NotFound => BackgroundError::NotFound {
                reference: reference.to_owned(),
            },
            FetchError::Failed(message) => BackgroundError::Fetch {
                reference: reference.to_owned(),
                message,
            },
        })?;
        Background::decode(reference, &bytes)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // The lock is never held across user code, so poisoning only means a
        // panic inside the cache bookkeeping itself; the state is still usable.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Least-recently-used map of decoded Backgrounds, bounded by total bytes.
#[derive(Default)]
struct Lru {
    max_bytes: usize,
    bytes: usize,
    /// Monotonic use counter; higher is more recent.
    clock: u64,
    entries: HashMap<String, (Arc<Background>, u64)>,
    /// Last use → reference, oldest first.
    by_use: BTreeMap<u64, String>,
}

impl Lru {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            ..Self::default()
        }
    }

    fn get(&mut self, reference: &str) -> Option<Arc<Background>> {
        self.clock += 1;
        let (background, last_use) = self.entries.get_mut(reference)?;
        let key = self.by_use.remove(last_use).expect("indexed entry");
        *last_use = self.clock;
        self.by_use.insert(self.clock, key);
        Some(Arc::clone(background))
    }

    fn insert(&mut self, reference: &str, background: Arc<Background>) {
        let size = background.byte_size();
        if size > self.max_bytes || self.entries.contains_key(reference) {
            return;
        }
        while self.bytes + size > self.max_bytes {
            let (_, oldest) = self.by_use.pop_first().expect("bytes > 0 means entries");
            let (evicted, _) = self.entries.remove(&oldest).expect("indexed entry");
            self.bytes -= evicted.byte_size();
        }
        self.clock += 1;
        self.bytes += size;
        self.by_use.insert(self.clock, reference.to_owned());
        self.entries
            .insert(reference.to_owned(), (background, self.clock));
    }
}
