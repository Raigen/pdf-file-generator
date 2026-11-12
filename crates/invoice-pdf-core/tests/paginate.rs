//! Pagination of the fixture Invoices: stable page counts and a complete,
//! ordered page plan.
//!
//! The release-mode timing check is `#[ignore]`d so that debug-mode
//! `cargo test` never depends on timing; CI runs it with
//! `cargo test --release -p invoice-pdf-core --test paginate -- --ignored`.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use invoice_pdf_core::font::Fonts;
use invoice_pdf_core::{InvoiceData, Layout, PAGE_HEIGHT_MM, PageHeader, PagePlan, load, paginate};

fn read_fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn load_fixtures(invoice_data: &str) -> (Layout, InvoiceData) {
    load(&read_fixture("layout.json"), &read_fixture(invoice_data)).expect("valid fixtures")
}

fn plan(invoice_data: &str) -> (Layout, InvoiceData, PagePlan) {
    let (layout, invoice_data) = load_fixtures(invoice_data);
    let plan = paginate(&layout, &invoice_data, &Fonts::embedded()).expect("paginates");
    (layout, invoice_data, plan)
}

/// Every Line Item is placed once, in order, inside the table area of its
/// page, and the Totals Block is on the last page only.
fn assert_consistent(layout: &Layout, invoice_data: &InvoiceData, plan: &PagePlan) {
    let table_bottom = PAGE_HEIGHT_MM - layout.margins.bottom - layout.footer.height;
    let mut next = 0;
    for (number, page) in plan.pages.iter().enumerate() {
        let expected_header = if number == 0 {
            PageHeader::FirstPage
        } else {
            PageHeader::Continuation
        };
        assert_eq!(page.header, expected_header, "page {}", number + 1);
        let mut y = page.table_header_y.map(|y| y + layout.table.header.height);
        for item in &page.line_items {
            assert_eq!(item.index, next, "page {}", number + 1);
            next += 1;
            let top = y.expect("Line Items need a Table Header");
            assert!((item.y - top).abs() < 1e-9, "Line Item {} gaps", item.index);
            assert!(item.y + item.height <= table_bottom + 1e-6);
            y = Some(item.y + item.height);
        }
        let is_last = number + 1 == plan.pages.len();
        assert_eq!(
            page.totals_block_y.is_some(),
            is_last,
            "page {}",
            number + 1
        );
        if let Some(totals_y) = page.totals_block_y {
            assert!(totals_y + layout.totals_block.height <= table_bottom + 1e-6);
        }
    }
    assert_eq!(next, invoice_data.line_items.len());
}

#[test]
fn small_invoice_fits_on_one_page() {
    let (layout, invoice_data, plan) = plan("invoice-data-small.json");
    assert_eq!(plan.page_count(), 1);
    assert_consistent(&layout, &invoice_data, &plan);
}

#[test]
fn invoice_with_300_line_items_has_a_stable_page_count() {
    let (layout, invoice_data, plan) = plan("invoice-data-300.json");
    assert_consistent(&layout, &invoice_data, &plan);
    assert_eq!(plan.page_count(), PAGES_300);
}

#[test]
fn invoice_with_10k_line_items_has_a_stable_page_count() {
    let (layout, invoice_data, plan) = plan("invoice-data-10k.json");
    assert_consistent(&layout, &invoice_data, &plan);
    assert_eq!(plan.page_count(), PAGES_10K);
}

#[test]
fn pagination_is_deterministic() {
    let (layout, invoice_data) = load_fixtures("invoice-data-300.json");
    let fonts = Fonts::embedded();
    let first = paginate(&layout, &invoice_data, &fonts).expect("paginates");
    let second = paginate(&layout, &invoice_data, &fonts).expect("paginates");
    assert_eq!(first, second);
}

/// Page counts of the fixtures with `fixtures/layout.json`. A change here
/// means pagination (or the fonts' metrics) changed.
const PAGES_300: usize = 11;
const PAGES_10K: usize = 334;

#[test]
#[ignore = "timing check; run in release mode"]
fn paginates_10k_line_items_in_under_100_ms_in_release_mode() {
    if cfg!(debug_assertions) {
        panic!(
            "the timing check is meaningful in release mode only: \
             cargo test --release -p invoice-pdf-core --test paginate -- --ignored"
        );
    }
    let (layout, invoice_data) = load_fixtures("invoice-data-10k.json");
    let fonts = Fonts::embedded();
    // Warm up once, then take the best of a few runs to be robust against a
    // busy CI machine.
    paginate(&layout, &invoice_data, &fonts).expect("paginates");
    let best = (0..5)
        .map(|_| {
            let start = Instant::now();
            let plan = paginate(&layout, &invoice_data, &fonts).expect("paginates");
            let elapsed = start.elapsed();
            std::hint::black_box(plan);
            elapsed
        })
        .min()
        .expect("runs");
    println!("paginated 10k Line Items in {best:?}");
    assert!(best < Duration::from_millis(100), "took {best:?}");
}
