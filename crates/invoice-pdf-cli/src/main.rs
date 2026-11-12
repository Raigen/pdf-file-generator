//! `invoice-pdf`: renders an Invoice from a Layout and Invoice Data.
//!
//! ```text
//! invoice-pdf render layout.json data.json [--einvoice x.xml --profile EN16931] -o out.pdf
//! ```
//!
//! Exit codes: 0 success, 1 invalid inputs or I/O failure, 2 usage error.
//! Rendering is not implemented yet (issue #6): `render` parses and validates
//! its inputs, reports every error, and then stops without writing a PDF.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use invoice_pdf_core::EInvoiceProfile;

#[derive(Debug, Parser)]
#[command(
    name = "invoice-pdf",
    version,
    about = "Renders Invoices as PDF/A-3 documents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Render an Invoice from a Layout and Invoice Data.
    Render(RenderArgs),
}

#[derive(Debug, clap::Args)]
struct RenderArgs {
    /// Layout JSON file.
    layout: PathBuf,
    /// Invoice Data JSON file.
    invoice_data: PathBuf,
    /// E-Invoice XML (CII) to embed; requires --profile.
    #[arg(long = "einvoice", value_name = "XML", requires = "profile")]
    e_invoice: Option<PathBuf>,
    /// ZUGFeRD profile of the E-Invoice XML, e.g. EN16931; requires --einvoice.
    #[arg(long, value_name = "PROFILE", requires = "e_invoice")]
    profile: Option<EInvoiceProfile>,
    /// Where to write the Invoice PDF.
    #[arg(short, long, value_name = "PDF")]
    output: PathBuf,
}

fn main() -> ExitCode {
    let Cli { command } = Cli::parse();
    let result = match command {
        Command::Render(args) => render(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn render(args: &RenderArgs) -> Result<(), String> {
    let layout = read(&args.layout, "Layout")?;
    let invoice_data = read(&args.invoice_data, "Invoice Data")?;
    let e_invoice = match (&args.e_invoice, args.profile) {
        (Some(path), Some(profile)) => Some((read(path, "E-Invoice XML")?, profile)),
        _ => None,
    };

    let (_layout, invoice_data) =
        invoice_pdf_core::load(&layout, &invoice_data).map_err(|errors| {
            let mut report: Vec<String> = errors
                .iter()
                .map(|error| format!("error: {error}"))
                .collect();
            let count = errors.len();
            report.push(format!(
                "invoice-pdf: {count} error{} in the inputs",
                if count == 1 { "" } else { "s" }
            ));
            report.join("\n")
        })?;

    let mut summary = format!(
        "invoice-pdf: inputs are valid ({} Line Items",
        invoice_data.line_items.len()
    );
    if let Some((_xml, profile)) = &e_invoice {
        summary.push_str(&format!(", E-Invoice XML (profile {profile})"));
    }
    summary.push(')');
    eprintln!("{summary}");
    Err(format!(
        "invoice-pdf: rendering is not implemented yet (issue #6); {} was not written",
        args.output.display()
    ))
}

fn read(path: &Path, what: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| {
        format!(
            "invoice-pdf: cannot read {what} {}: {error}",
            path.display()
        )
    })
}
