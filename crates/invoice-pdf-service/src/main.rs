//! HTTP service for rendering Invoices. Stub until `POST /render` lands (issue #10).

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!(
        "invoice-pdf-service {}: the HTTP service is not implemented yet (see issue #10)",
        env!("CARGO_PKG_VERSION")
    );
    ExitCode::FAILURE
}
