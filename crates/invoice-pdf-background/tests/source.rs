//! Resolving Background references: each scheme, and every rejection.

use std::fs;
use std::path::Path;

use invoice_pdf_background::{BackgroundConfig, BackgroundError, Source};
use tempfile::TempDir;

/// A base directory with `backgrounds/acme.png`, plus a sibling `secret.png`
/// outside it. Returns (root, base dir).
fn base_dir() -> (TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("tempdir");
    let base = root.path().join("base");
    fs::create_dir_all(base.join("backgrounds")).unwrap();
    fs::write(base.join("backgrounds/acme.png"), b"png").unwrap();
    fs::write(root.path().join("secret.png"), b"secret").unwrap();
    (root, base)
}

fn file_config(base: &Path) -> BackgroundConfig {
    BackgroundConfig {
        base_dir: Some(base.to_path_buf()),
        ..BackgroundConfig::default()
    }
}

fn https_config() -> BackgroundConfig {
    BackgroundConfig {
        https_allowed_hosts: vec!["assets.example.com".into()],
        ..BackgroundConfig::default()
    }
}

fn reject(reference: &str, config: &BackgroundConfig) -> BackgroundError {
    let error = Source::resolve(reference, config).expect_err("reference should be rejected");
    assert_eq!(error.reference(), reference);
    assert!(error.to_string().contains(reference), "{error}");
    assert!(error.is_rejected_reference(), "{error:?}");
    error
}

// gs://

#[test]
fn resolves_gcs_reference() {
    let source = Source::resolve(
        "gs://tenant-assets/backgrounds/acme.png",
        &BackgroundConfig::default(),
    )
    .expect("valid gs reference");
    assert_eq!(
        source,
        Source::Gcs {
            bucket: "tenant-assets".into(),
            object: "backgrounds/acme.png".into()
        }
    );
}

#[test]
fn rejects_gcs_reference_without_object() {
    let config = BackgroundConfig::default();
    for reference in [
        "gs://tenant-assets",
        "gs://tenant-assets/",
        "gs:///acme.png",
    ] {
        let error = reject(reference, &config);
        assert!(
            matches!(error, BackgroundError::InvalidReference { .. }),
            "{reference}: {error:?}"
        );
    }
}

#[test]
fn rejects_gcs_reference_with_invalid_bucket_name() {
    let error = reject("gs://Tenant Assets/acme.png", &BackgroundConfig::default());
    assert!(
        matches!(error, BackgroundError::InvalidReference { .. }),
        "{error:?}"
    );
}

// file://

#[test]
fn resolves_file_reference_inside_base_dir() {
    let (_root, base) = base_dir();
    let source =
        Source::resolve("file://backgrounds/acme.png", &file_config(&base)).expect("valid file");
    assert_eq!(
        source,
        Source::File(base.join("backgrounds/acme.png").canonicalize().unwrap())
    );
}

#[test]
fn rejects_file_reference_with_parent_traversal() {
    let (_root, base) = base_dir();
    let config = file_config(&base);
    for reference in [
        "file://../secret.png",
        "file://backgrounds/../../secret.png",
        // Stays inside the base directory, but `..` is rejected outright.
        "file://backgrounds/../backgrounds/acme.png",
    ] {
        let error = reject(reference, &config);
        assert!(
            matches!(error, BackgroundError::OutsideBaseDir { .. }),
            "{reference}: {error:?}"
        );
    }
}

#[test]
fn rejects_absolute_file_reference() {
    let (root, base) = base_dir();
    let inside = base.join("backgrounds/acme.png");
    let outside = root.path().join("secret.png");
    for path in [inside, outside] {
        let reference = format!("file://{}", path.display());
        let error = reject(&reference, &file_config(&base));
        assert!(
            matches!(error, BackgroundError::OutsideBaseDir { .. }),
            "{reference}: {error:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn rejects_symlink_pointing_outside_base_dir() {
    let (root, base) = base_dir();
    std::os::unix::fs::symlink(root.path().join("secret.png"), base.join("escape.png")).unwrap();
    std::os::unix::fs::symlink(root.path(), base.join("up")).unwrap();
    for reference in ["file://escape.png", "file://up/secret.png"] {
        let error = reject(reference, &file_config(&base));
        assert!(
            matches!(error, BackgroundError::OutsideBaseDir { .. }),
            "{reference}: {error:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn follows_symlink_that_stays_inside_base_dir() {
    let (_root, base) = base_dir();
    std::os::unix::fs::symlink(base.join("backgrounds/acme.png"), base.join("current.png"))
        .unwrap();
    let source = Source::resolve("file://current.png", &file_config(&base)).expect("inside");
    assert_eq!(
        source,
        Source::File(base.join("backgrounds/acme.png").canonicalize().unwrap())
    );
}

#[test]
fn reports_missing_file_as_not_found() {
    let (_root, base) = base_dir();
    let error = reject("file://backgrounds/missing.png", &file_config(&base));
    assert!(
        matches!(error, BackgroundError::NotFound { .. }),
        "{error:?}"
    );
}

#[test]
fn rejects_file_reference_without_path() {
    let (_root, base) = base_dir();
    let error = reject("file://", &file_config(&base));
    assert!(
        matches!(error, BackgroundError::InvalidReference { .. }),
        "{error:?}"
    );
}

#[test]
fn rejects_file_reference_naming_a_directory() {
    let (_root, base) = base_dir();
    let error = reject("file://backgrounds", &file_config(&base));
    assert!(
        matches!(error, BackgroundError::InvalidReference { .. }),
        "{error:?}"
    );
}

#[test]
fn rejects_file_reference_without_configured_base_dir() {
    let error = reject("file://backgrounds/acme.png", &BackgroundConfig::default());
    assert!(
        matches!(
            error,
            BackgroundError::SchemeNotConfigured {
                scheme: "file://",
                ..
            }
        ),
        "{error:?}"
    );
}

// https://

#[test]
fn resolves_https_reference_on_allowed_host() {
    let source = Source::resolve("https://assets.example.com/bg/acme.png", &https_config())
        .expect("allowed host");
    assert_eq!(
        source,
        Source::Https {
            url: "https://assets.example.com/bg/acme.png".into(),
            host: "assets.example.com".into()
        }
    );
}

#[test]
fn matches_allowed_host_case_insensitively() {
    let source = Source::resolve("https://Assets.Example.COM/acme.png", &https_config())
        .expect("allowed host");
    assert!(matches!(source, Source::Https { host, .. } if host == "assets.example.com"));
}

#[test]
fn rejects_https_reference_on_other_host() {
    for (reference, host) in [
        ("https://evil.example.org/acme.png", "evil.example.org"),
        (
            "https://assets.example.com.evil.example.org/acme.png",
            "assets.example.com.evil.example.org",
        ),
        ("https://example.com/acme.png", "example.com"),
    ] {
        let error = reject(reference, &https_config());
        assert_eq!(
            error,
            BackgroundError::HostNotAllowed {
                reference: reference.into(),
                host: host.into()
            }
        );
    }
}

#[test]
fn rejects_https_reference_with_userinfo_port_or_odd_authority() {
    for reference in [
        "https://assets.example.com@evil.example.org/acme.png",
        "https://user@assets.example.com/acme.png",
        "https://assets.example.com:8443/acme.png",
        "https://assets.example.com\\@evil.example.org/acme.png",
        "https://assets.example.com%2F@evil.example.org/acme.png",
        "https:///acme.png",
    ] {
        let error = reject(reference, &https_config());
        assert!(
            matches!(
                error,
                BackgroundError::InvalidReference { .. } | BackgroundError::HostNotAllowed { .. }
            ),
            "{reference}: {error:?}"
        );
        assert!(Source::resolve(reference, &https_config()).is_err());
    }
}

#[test]
fn rejects_https_reference_without_path() {
    for reference in ["https://assets.example.com", "https://assets.example.com/"] {
        let error = reject(reference, &https_config());
        assert!(
            matches!(error, BackgroundError::InvalidReference { .. }),
            "{reference}: {error:?}"
        );
    }
}

#[test]
fn rejects_https_reference_without_allow_list() {
    let error = reject(
        "https://assets.example.com/acme.png",
        &BackgroundConfig::default(),
    );
    assert!(
        matches!(
            error,
            BackgroundError::SchemeNotConfigured {
                scheme: "https://",
                ..
            }
        ),
        "{error:?}"
    );
}

// Other schemes

#[test]
fn rejects_unknown_schemes() {
    let (_root, base) = base_dir();
    let config = BackgroundConfig {
        base_dir: Some(base),
        https_allowed_hosts: vec!["assets.example.com".into()],
        ..BackgroundConfig::default()
    };
    for reference in [
        "",
        "acme.png",
        "/srv/backgrounds/acme.png",
        "http://assets.example.com/acme.png",
        "ftp://assets.example.com/acme.png",
        "s3://bucket/acme.png",
        "data:image/png;base64,iVBORw0KGgo=",
        "GS://tenant-assets/acme.png",
        "HTTPS://assets.example.com/acme.png",
    ] {
        let error = reject(reference, &config);
        assert_eq!(
            error,
            BackgroundError::UnsupportedScheme {
                reference: reference.into()
            }
        );
    }
}
