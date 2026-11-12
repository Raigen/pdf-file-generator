//! Core library of the Invoice PDF generator: the Layout and Invoice Data
//! models and the validation pass that runs before any rendering.
//!
//! Every error is a [`ValidationError`] with a JSON pointer into the render
//! request `{layout, invoiceData}`, so the CLI and the HTTP service report the
//! same paths.

mod e_invoice;
mod error;
pub mod font;
mod invoice_data;
mod layout;
#[cfg(test)]
mod test_support;
mod validate;

use serde_json::Value;

pub use e_invoice::EInvoiceProfile;
pub use error::{INVOICE_DATA_PATH, LAYOUT_PATH, ValidationError};
pub use invoice_data::{GiroCode, InvoiceData, LineItem, Values, text};
pub use layout::{
    Align, Column, Field, Font, Footer, GiroCodePlacement, Layout, LineItemTable, Margins,
    PAGE_HEIGHT_MM, PAGE_WIDTH_MM, PageNumber, PageSize, StaticText, Zone,
};
pub use validate::{validate, validate_layout};

/// Parses and validates a Layout and Invoice Data given as JSON text.
///
/// Returns every error found in both inputs. If one input cannot be parsed,
/// the other is still parsed (and a parsed Layout still validated on its own),
/// but the cross-checks between the two are skipped. These include resolving
/// the referenced keys (missing or non-string values), which needs the Layout.
pub fn load(
    layout_json: &str,
    invoice_data_json: &str,
) -> Result<(Layout, InvoiceData), Vec<ValidationError>> {
    combine(
        Layout::from_json(layout_json),
        InvoiceData::from_json(invoice_data_json),
    )
}

/// Like [`load`], for inputs that are already JSON values (e.g. the
/// `layout` and `invoiceData` members of a render request).
pub fn load_values(
    layout: &Value,
    invoice_data: &Value,
) -> Result<(Layout, InvoiceData), Vec<ValidationError>> {
    combine(
        Layout::from_value(layout),
        InvoiceData::from_value(invoice_data),
    )
}

fn combine(
    layout: Result<Layout, Vec<ValidationError>>,
    invoice_data: Result<InvoiceData, Vec<ValidationError>>,
) -> Result<(Layout, InvoiceData), Vec<ValidationError>> {
    match (layout, invoice_data) {
        (Ok(layout), Ok(invoice_data)) => {
            let errors = validate(&layout, &invoice_data);
            if errors.is_empty() {
                Ok((layout, invoice_data))
            } else {
                Err(errors)
            }
        }
        (Ok(layout), Err(data_errors)) => {
            let mut errors = validate_layout(&layout);
            errors.extend(data_errors);
            Err(errors)
        }
        (Err(errors), Ok(_)) => Err(errors),
        (Err(mut errors), Err(data_errors)) => {
            errors.extend(data_errors);
            Err(errors)
        }
    }
}
