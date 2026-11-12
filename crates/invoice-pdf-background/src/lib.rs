//! Loading of the Background: the single raster image a Layout references and
//! that is drawn behind every page of an Invoice.
//!
//! A [`BackgroundLoader`] turns a Background reference into a decoded,
//! ready-to-embed [`Background`]:
//!
//! 1. The reference is resolved against the [`BackgroundConfig`] into a
//!    [`Source`]: `gs://bucket/object`, `file://relative/path` (inside the
//!    configured base directory) or `https://host/path` (host on the
//!    allow-list). Anything else is rejected before any I/O.
//! 2. The bytes are fetched through a [`Fetch`] implementation.
//! 3. The image is decoded once: a JPEG passes through as `DCTDecode`, a PNG is
//!    recompressed as `FlateDecode` with its alpha channel split into an
//!    SMask. Only RGB and grayscale images are accepted (ADR 0002: CMYK
//!    Backgrounds are rejected, not colour-converted).
//! 4. The result is kept in an in-memory LRU cache bounded by bytes. References
//!    are immutable by contract, so cached Backgrounds are never revalidated,
//!    and concurrent loads of one reference fetch and decode it only once.

mod config;
mod decode;
mod error;
mod fetch;
mod loader;
mod source;

pub use config::{
    BASE_DIR_VAR, BackgroundConfig, CACHE_BYTES_VAR, ConfigError, DEFAULT_CACHE_BYTES,
    HTTPS_HOSTS_VAR,
};
pub use decode::{Background, ColorSpace, Filter, SMask};
pub use error::BackgroundError;
pub use fetch::{DefaultFetcher, Fetch, FetchError};
pub use loader::BackgroundLoader;
pub use source::Source;
