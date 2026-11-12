//! Reading the configuration from environment variables.

use std::collections::HashMap;
use std::path::PathBuf;

use invoice_pdf_background::{
    BASE_DIR_VAR, BackgroundConfig, CACHE_BYTES_VAR, DEFAULT_CACHE_BYTES, HTTPS_HOSTS_VAR,
};

fn from(vars: &[(&str, &str)]) -> Result<BackgroundConfig, invoice_pdf_background::ConfigError> {
    let vars: HashMap<String, String> = vars
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    BackgroundConfig::from_vars(|name| vars.get(name).cloned())
}

#[test]
fn defaults_when_nothing_is_set() {
    assert_eq!(from(&[]).unwrap(), BackgroundConfig::default());
    assert_eq!(
        BackgroundConfig::default().cache_max_bytes,
        DEFAULT_CACHE_BYTES
    );
}

#[test]
fn reads_all_variables() {
    let config = from(&[
        (BASE_DIR_VAR, "/srv/backgrounds"),
        (HTTPS_HOSTS_VAR, " Assets.Example.com, cdn.example.com,, "),
        (CACHE_BYTES_VAR, "1048576"),
    ])
    .unwrap();
    assert_eq!(
        config,
        BackgroundConfig {
            base_dir: Some(PathBuf::from("/srv/backgrounds")),
            https_allowed_hosts: vec!["assets.example.com".into(), "cdn.example.com".into()],
            cache_max_bytes: 1_048_576,
        }
    );
}

#[test]
fn treats_empty_values_as_unset() {
    let config = from(&[
        (BASE_DIR_VAR, ""),
        (HTTPS_HOSTS_VAR, ""),
        (CACHE_BYTES_VAR, ""),
    ])
    .unwrap();
    assert_eq!(config, BackgroundConfig::default());
}

#[test]
fn rejects_invalid_cache_size() {
    let error = from(&[(CACHE_BYTES_VAR, "64MB")]).expect_err("not a number");
    assert_eq!(error.var, CACHE_BYTES_VAR);
    assert!(error.to_string().contains("64MB"), "{error}");
}
