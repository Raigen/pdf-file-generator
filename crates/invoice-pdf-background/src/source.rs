//! Resolving a Background reference into a checked [`Source`].

use std::io;
use std::path::{Component, Path, PathBuf};

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

const GS: &str = "gs://";
const FILE: &str = "file://";
const HTTPS: &str = "https://";

impl Source {
    /// Resolves `reference` against `config`.
    ///
    /// Schemes are matched case-sensitively and nothing is percent-decoded.
    ///
    /// - `gs://bucket/object`: the bucket must be a valid GCS bucket name and
    ///   the object non-empty.
    /// - `file://relative/path`: relative to the base directory. A leading
    ///   `/` (absolute path) or any `..` component is rejected, then the path
    ///   is canonicalized (following symlinks) and must still lie inside the
    ///   canonical base directory and name a regular file. This touches the
    ///   filesystem.
    /// - `https://host/path`: the authority must be a bare host name (no
    ///   user info, no port) that is on the allow-list, compared
    ///   case-insensitively, and the path must not be empty.
    pub fn resolve(reference: &str, config: &BackgroundConfig) -> Result<Self, BackgroundError> {
        if let Some(rest) = reference.strip_prefix(GS) {
            resolve_gcs(reference, rest)
        } else if let Some(rest) = reference.strip_prefix(FILE) {
            resolve_file(reference, rest, config)
        } else if let Some(rest) = reference.strip_prefix(HTTPS) {
            resolve_https(reference, rest, config)
        } else {
            Err(BackgroundError::UnsupportedScheme {
                reference: reference.to_owned(),
            })
        }
    }
}

fn invalid(reference: &str, reason: impl Into<String>) -> BackgroundError {
    BackgroundError::InvalidReference {
        reference: reference.to_owned(),
        reason: reason.into(),
    }
}

fn resolve_gcs(reference: &str, rest: &str) -> Result<Source, BackgroundError> {
    let (bucket, object) = rest.split_once('/').unwrap_or((rest, ""));
    // https://cloud.google.com/storage/docs/buckets#naming (dotted names may be up to 222).
    let valid_bucket = (3..=222).contains(&bucket.len())
        && bucket
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
        && bucket.as_bytes()[0].is_ascii_alphanumeric()
        && bucket.as_bytes()[bucket.len() - 1].is_ascii_alphanumeric();
    if !valid_bucket {
        return Err(invalid(
            reference,
            format!("invalid bucket name `{bucket}`"),
        ));
    }
    if object.is_empty() {
        return Err(invalid(reference, "missing object name after the bucket"));
    }
    Ok(Source::Gcs {
        bucket: bucket.to_owned(),
        object: object.to_owned(),
    })
}

fn resolve_file(
    reference: &str,
    rest: &str,
    config: &BackgroundConfig,
) -> Result<Source, BackgroundError> {
    let Some(base_dir) = &config.base_dir else {
        return Err(BackgroundError::SchemeNotConfigured {
            reference: reference.to_owned(),
            scheme: FILE,
        });
    };
    if rest.is_empty() {
        return Err(invalid(reference, "missing path"));
    }
    if rest.contains('\0') {
        return Err(invalid(reference, "path contains a NUL byte"));
    }
    let outside = || BackgroundError::OutsideBaseDir {
        reference: reference.to_owned(),
    };
    let relative = Path::new(rest);
    let only_normal_components = relative
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
    if !only_normal_components {
        // `..`, an absolute path (root or, on Windows, a prefix).
        return Err(outside());
    }

    let base = base_dir
        .canonicalize()
        .map_err(|error| BackgroundError::Fetch {
            reference: reference.to_owned(),
            message: format!(
                "Background base directory {} is not usable: {error}",
                base_dir.display()
            ),
        })?;
    let path = base
        .join(relative)
        .canonicalize()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => BackgroundError::NotFound {
                reference: reference.to_owned(),
            },
            _ => BackgroundError::Fetch {
                reference: reference.to_owned(),
                message: error.to_string(),
            },
        })?;
    // Symlinks are resolved by now, so this catches links pointing outside.
    if !path.starts_with(&base) {
        return Err(outside());
    }
    if !path.is_file() {
        return Err(invalid(reference, "not a regular file"));
    }
    Ok(Source::File(path))
}

fn resolve_https(
    reference: &str,
    rest: &str,
    config: &BackgroundConfig,
) -> Result<Source, BackgroundError> {
    if config.https_allowed_hosts.is_empty() {
        return Err(BackgroundError::SchemeNotConfigured {
            reference: reference.to_owned(),
            scheme: HTTPS,
        });
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    // Only a bare host name is accepted, so the HTTP client cannot read a
    // different host out of the URL than the one checked here (no user info,
    // port, IPv6 literal, backslash or percent-encoding tricks).
    let bare_host = !authority.is_empty()
        && authority
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    if !bare_host {
        return Err(invalid(
            reference,
            format!("`{authority}` is not a bare host name (user info and ports are not allowed)"),
        ));
    }
    let host = authority.to_ascii_lowercase();
    if !config
        .https_allowed_hosts
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(&host))
    {
        return Err(BackgroundError::HostNotAllowed {
            reference: reference.to_owned(),
            host,
        });
    }
    if !path.starts_with('/') || path.len() < 2 {
        return Err(invalid(reference, "missing path after the host"));
    }
    if path
        .bytes()
        .any(|b| b.is_ascii_control() || b == b' ' || b == b'\\')
    {
        return Err(invalid(
            reference,
            "path contains whitespace, control characters or backslashes",
        ));
    }
    Ok(Source::Https {
        url: reference.to_owned(),
        host,
    })
}
