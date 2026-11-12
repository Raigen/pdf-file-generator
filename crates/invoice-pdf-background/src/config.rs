//! Configuration of Background loading, read from environment variables.

use std::fmt;
use std::path::PathBuf;

/// Directory that `file://` Background references are resolved in. Unset
/// means `file://` references are rejected.
pub const BASE_DIR_VAR: &str = "INVOICE_PDF_BACKGROUND_BASE_DIR";
/// Comma-separated host names that `https://` Background references may
/// name, e.g. `assets.example.com,cdn.example.com`. Unset or empty means
/// `https://` references are rejected.
pub const HTTPS_HOSTS_VAR: &str = "INVOICE_PDF_BACKGROUND_HTTPS_HOSTS";
/// Upper bound of the Background cache, in bytes of decoded image data.
/// `0` disables caching. Defaults to [`DEFAULT_CACHE_BYTES`].
pub const CACHE_BYTES_VAR: &str = "INVOICE_PDF_BACKGROUND_CACHE_BYTES";

/// Default cache bound: 64 MiB.
pub const DEFAULT_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundConfig {
    /// Base directory of `file://` references; `None` disables the scheme.
    pub base_dir: Option<PathBuf>,
    /// Allowed hosts of `https://` references, lower-case. Empty disables the scheme.
    pub https_allowed_hosts: Vec<String>,
    /// Bound of the cache in bytes of decoded data (image plus SMask).
    pub cache_max_bytes: usize,
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            base_dir: None,
            https_allowed_hosts: Vec::new(),
            cache_max_bytes: DEFAULT_CACHE_BYTES,
        }
    }
}

/// An environment variable that holds an unusable value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub var: &'static str,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.var, self.message)
    }
}

impl std::error::Error for ConfigError {}

impl BackgroundConfig {
    /// Reads the configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// Reads the configuration through `var`, which returns the value of an
    /// environment variable by name (so tests need not touch the process
    /// environment). Empty values count as unset.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let _ = var;
        unimplemented!("issue #5")
    }
}
