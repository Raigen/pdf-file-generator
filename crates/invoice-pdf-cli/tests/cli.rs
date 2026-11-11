//! End-to-end tests of the `invoice-pdf` binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
        .display()
        .to_string()
}

/// A path in a directory private to the named test.
fn scratch(test: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

fn invoice_pdf(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_invoice-pdf"))
        .args(args)
        .output()
        .expect("run invoice-pdf")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn valid_inputs_are_reported_valid_but_not_rendered_yet() {
    let out = scratch("valid_inputs", "out.pdf");
    let output = invoice_pdf(&[
        "render",
        &fixture("layout.json"),
        &fixture("invoice-data-small.json"),
        "-o",
        out.to_str().unwrap(),
    ]);
    let stderr = stderr(&output);
    assert!(
        stderr.contains("inputs are valid (3 Line Items)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("rendering is not implemented yet"),
        "{stderr}"
    );
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(!out.exists(), "no PDF must be written yet");
}

#[test]
fn every_validation_error_is_reported_with_its_path() {
    let output = invoice_pdf(&[
        "render",
        &fixture("layout.json"),
        &fixture("malformed/invoice-data-missing-keys.json"),
        "-o",
        "unused.pdf",
    ]);
    let stderr = stderr(&output);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    for path in [
        "/invoiceData/header/dueDate",
        "/invoiceData/lineItems/2/description",
        "/invoiceData/totals/tax",
    ] {
        assert!(
            stderr.contains(&format!("error: {path}: ")),
            "{path} in {stderr}"
        );
    }
    assert!(stderr.contains("3 errors"), "{stderr}");
    // Keys the Layout does not reference are ignored.
    assert!(!stderr.contains("orderNumber"), "{stderr}");
    assert!(!stderr.contains("discount"), "{stderr}");
    assert!(!stderr.contains("inputs are valid"), "{stderr}");
}

#[test]
fn layout_and_invoice_data_errors_are_reported_together() {
    let output = invoice_pdf(&[
        "render",
        &fixture("malformed/layout-wrong-type.json"),
        &fixture("malformed/invoice-data-syntax-error.json"),
        "-o",
        "unused.pdf",
    ]);
    let stderr = stderr(&output);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("error: /layout/table/columns/2/source: "),
        "{stderr}"
    );
    assert!(
        stderr.contains("error: /invoiceData: invalid JSON"),
        "{stderr}"
    );
}

#[test]
fn unreadable_input_file_is_reported() {
    let output = invoice_pdf(&[
        "render",
        &fixture("does-not-exist.json"),
        &fixture("invoice-data-small.json"),
        "-o",
        "unused.pdf",
    ]);
    let stderr = stderr(&output);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("cannot read Layout"), "{stderr}");
    assert!(stderr.contains("does-not-exist.json"), "{stderr}");
}

#[test]
fn e_invoice_xml_and_profile_are_accepted() {
    let xml = scratch("e_invoice_accepted", "factur-x.xml");
    std::fs::write(&xml, "<rsm:CrossIndustryInvoice/>").unwrap();
    let output = invoice_pdf(&[
        "render",
        &fixture("layout.json"),
        &fixture("invoice-data-small.json"),
        "--einvoice",
        xml.to_str().unwrap(),
        "--profile",
        "EN16931",
        "-o",
        "unused.pdf",
    ]);
    let stderr = stderr(&output);
    assert!(stderr.contains("inputs are valid"), "{stderr}");
    assert!(
        stderr.contains("E-Invoice XML (profile EN 16931)"),
        "{stderr}"
    );
}

#[test]
fn unreadable_e_invoice_xml_is_reported() {
    let output = invoice_pdf(&[
        "render",
        &fixture("layout.json"),
        &fixture("invoice-data-small.json"),
        "--einvoice",
        "missing.xml",
        "--profile",
        "EN16931",
        "-o",
        "unused.pdf",
    ]);
    let stderr = stderr(&output);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("cannot read E-Invoice XML missing.xml"),
        "{stderr}"
    );
}

#[test]
fn e_invoice_xml_requires_a_profile() {
    let output = invoice_pdf(&[
        "render",
        "layout.json",
        "data.json",
        "--einvoice",
        "x.xml",
        "-o",
        "out.pdf",
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("--profile"), "{}", stderr(&output));
}

#[test]
fn profile_requires_e_invoice_xml() {
    let output = invoice_pdf(&[
        "render",
        "layout.json",
        "data.json",
        "--profile",
        "EN16931",
        "-o",
        "out.pdf",
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("--einvoice"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn unknown_profile_is_a_usage_error() {
    let output = invoice_pdf(&[
        "render",
        "layout.json",
        "data.json",
        "--einvoice",
        "x.xml",
        "--profile",
        "COMFORT",
        "-o",
        "out.pdf",
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("unknown ZUGFeRD profile `COMFORT`"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn output_path_is_required() {
    let output = invoice_pdf(&["render", "layout.json", "data.json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("--output"), "{}", stderr(&output));
}
