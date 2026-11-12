//! The loader end to end with a fake fetcher: every scheme, the cache and
//! single-flight loading.

mod support;

use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use invoice_pdf_background::{
    BackgroundConfig, BackgroundError, BackgroundLoader, ColorSpace, FetchError, Filter, Source,
};
use support::{FakeFetcher, cmyk_jpeg, rgb_jpeg, rgb_png, rgba_png};

const GS: &str = "gs://tenant-assets/acme.png";
const HTTPS: &str = "https://assets.example.com/acme.jpg";

fn gs_source() -> Source {
    Source::Gcs {
        bucket: "tenant-assets".into(),
        object: "acme.png".into(),
    }
}

fn https_source() -> Source {
    Source::Https {
        url: HTTPS.into(),
        host: "assets.example.com".into(),
    }
}

fn config() -> BackgroundConfig {
    BackgroundConfig {
        https_allowed_hosts: vec!["assets.example.com".into()],
        ..BackgroundConfig::default()
    }
}

fn loader(fetcher: &Arc<FakeFetcher>, config: BackgroundConfig) -> BackgroundLoader {
    BackgroundLoader::new(config, Arc::clone(fetcher))
}

#[test]
fn loads_gcs_background() {
    let fetcher = Arc::new(FakeFetcher::new().serve(gs_source(), rgba_png()));
    let background = loader(&fetcher, config()).load(GS).expect("gs background");
    assert_eq!(background.filter, Filter::FlateDecode);
    assert!(background.s_mask.is_some());
    assert_eq!(fetcher.fetched(), [gs_source()]);
}

#[test]
fn loads_https_background() {
    let fetcher = Arc::new(FakeFetcher::new().serve(https_source(), rgb_jpeg()));
    let background = loader(&fetcher, config())
        .load(HTTPS)
        .expect("https background");
    assert_eq!(background.filter, Filter::DctDecode);
    assert_eq!(background.color_space, ColorSpace::DeviceRgb);
    assert_eq!(fetcher.fetched(), [https_source()]);
}

#[test]
fn loads_file_background_with_default_fetcher() {
    let base = tempfile::tempdir().unwrap();
    fs::create_dir(base.path().join("backgrounds")).unwrap();
    fs::write(base.path().join("backgrounds/acme.png"), rgb_png()).unwrap();
    let loader = BackgroundLoader::with_default_fetcher(BackgroundConfig {
        base_dir: Some(base.path().to_path_buf()),
        ..BackgroundConfig::default()
    });
    let background = loader
        .load("file://backgrounds/acme.png")
        .expect("file background");
    assert_eq!((background.width, background.height), (2, 2));
}

#[cfg(not(feature = "gcs"))]
#[test]
fn default_fetcher_without_gcs_feature_reports_fetch_error() {
    let loader = BackgroundLoader::with_default_fetcher(config());
    let error = loader.load(GS).expect_err("gcs disabled");
    assert!(matches!(error, BackgroundError::Fetch { .. }), "{error:?}");
    assert!(error.to_string().contains("gcs"), "{error}");
}

#[test]
fn rejected_references_are_never_fetched() {
    let fetcher = Arc::new(FakeFetcher::new());
    let loader = loader(&fetcher, config());
    for reference in [
        "ftp://assets.example.com/acme.png",
        "https://evil.example.org/acme.png",
        "file://../secret.png",
    ] {
        let error = loader.load(reference).expect_err("rejected");
        assert_eq!(error.reference(), reference);
    }
    assert_eq!(fetcher.calls(), 0);
}

#[test]
fn rejects_cmyk_background_naming_the_reference() {
    let fetcher = Arc::new(FakeFetcher::new().serve(https_source(), cmyk_jpeg()));
    let error = loader(&fetcher, config()).load(HTTPS).expect_err("cmyk");
    assert!(
        matches!(error, BackgroundError::UnsupportedColorSpace { .. }),
        "{error:?}"
    );
    assert!(error.to_string().contains(HTTPS), "{error}");
}

#[test]
fn maps_fetch_errors() {
    let fetcher = Arc::new(FakeFetcher::new().respond(
        https_source(),
        vec![Err(FetchError::Failed("connection reset".into()))],
    ));
    let loader = loader(&fetcher, config());
    let error = loader.load(HTTPS).expect_err("fetch failed");
    assert_eq!(
        error,
        BackgroundError::Fetch {
            reference: HTTPS.into(),
            message: "connection reset".into()
        }
    );
    assert!(!error.is_rejected_reference());

    let error = loader.load(GS).expect_err("not served");
    assert_eq!(
        error,
        BackgroundError::NotFound {
            reference: GS.into()
        }
    );
}

#[test]
fn second_load_does_no_fetch_or_decode() {
    let fetcher = Arc::new(FakeFetcher::new().serve(gs_source(), rgba_png()));
    let loader = loader(&fetcher, config());
    let first = loader.load(GS).unwrap();
    let second = loader.load(GS).unwrap();
    assert_eq!(fetcher.calls(), 1);
    // The very same decoded Background is handed out, so nothing was decoded again.
    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn concurrent_loads_of_one_reference_fetch_once() {
    let fetcher = Arc::new(FakeFetcher::gated_on(gs_source()).serve(gs_source(), rgba_png()));
    let loader = loader(&fetcher, config());
    let results = thread::scope(|scope| {
        let handles: Vec<_> = (0..8).map(|_| scope.spawn(|| loader.load(GS))).collect();
        // Give every thread time to reach the loader while the first fetch is
        // held at the gate; without single-flight they would all fetch.
        thread::sleep(Duration::from_millis(200));
        fetcher.open_gate();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap().expect("loaded"))
            .collect::<Vec<_>>()
    });
    assert_eq!(fetcher.calls(), 1);
    assert!(
        results
            .iter()
            .all(|background| Arc::ptr_eq(background, &results[0]))
    );
}

#[test]
fn a_held_load_does_not_block_other_references() {
    let fetcher = Arc::new(
        FakeFetcher::gated_on(gs_source())
            .serve(gs_source(), rgba_png())
            .serve(https_source(), rgb_jpeg()),
    );
    let loader = loader(&fetcher, config());
    thread::scope(|scope| {
        let held = scope.spawn(|| loader.load(GS));
        thread::sleep(Duration::from_millis(50));
        loader.load(HTTPS).expect("not blocked by the held GS load");
        fetcher.open_gate();
        held.join().unwrap().expect("loaded");
    });
    assert_eq!(fetcher.calls(), 2);
}

#[test]
fn failed_loads_are_not_cached() {
    let fetcher = Arc::new(FakeFetcher::new().respond(
        gs_source(),
        vec![Err(FetchError::Failed("timeout".into())), Ok(rgba_png())],
    ));
    let loader = loader(&fetcher, config());
    assert!(loader.load(GS).is_err());
    assert!(loader.load(GS).is_ok());
    assert!(loader.load(GS).is_ok());
    assert_eq!(fetcher.calls(), 2);
}

/// Three references whose decoded Backgrounds have the same byte size.
fn three_backgrounds() -> (Arc<FakeFetcher>, [&'static str; 3], usize) {
    let references = [
        "gs://tenant-assets/a.png",
        "gs://tenant-assets/b.png",
        "gs://tenant-assets/c.png",
    ];
    let mut fetcher = FakeFetcher::new();
    for reference in references {
        let object = reference.trim_start_matches("gs://tenant-assets/");
        fetcher = fetcher.serve(
            Source::Gcs {
                bucket: "tenant-assets".into(),
                object: object.into(),
            },
            rgba_png(),
        );
    }
    let size = invoice_pdf_background::Background::decode("x", &rgba_png())
        .unwrap()
        .byte_size();
    (Arc::new(fetcher), references, size)
}

#[test]
fn evicts_least_recently_used_background_beyond_byte_bound() {
    let (fetcher, [a, b, c], size) = three_backgrounds();
    let loader = loader(
        &fetcher,
        BackgroundConfig {
            cache_max_bytes: 2 * size,
            ..config()
        },
    );
    loader.load(a).unwrap();
    loader.load(b).unwrap();
    loader.load(a).unwrap(); // hit; b is now least recently used
    assert_eq!(fetcher.calls(), 2);
    loader.load(c).unwrap(); // evicts b
    assert_eq!(fetcher.calls(), 3);
    loader.load(a).unwrap(); // still cached
    loader.load(c).unwrap(); // still cached
    assert_eq!(fetcher.calls(), 3);
    loader.load(b).unwrap(); // fetched again
    assert_eq!(fetcher.calls(), 4);
}

#[test]
fn zero_cache_bound_disables_caching() {
    let (fetcher, [a, _, _], _) = three_backgrounds();
    let loader = loader(
        &fetcher,
        BackgroundConfig {
            cache_max_bytes: 0,
            ..config()
        },
    );
    loader.load(a).unwrap();
    loader.load(a).unwrap();
    assert_eq!(fetcher.calls(), 2);
}
