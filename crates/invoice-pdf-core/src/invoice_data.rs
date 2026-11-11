//! The Invoice Data: the per-run content of one Invoice.
//!
//! Every value is a pre-formatted string; the generator never calculates.
//! Parsing walks the JSON by hand instead of using derived `Deserialize`, so
//! that every non-string value (and every other shape error) is reported with
//! its own JSON pointer rather than as the first serde failure.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::error::ValidationError;

/// One Line Item: column source key → pre-formatted value.
pub type LineItem = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceData {
    /// Values for the Fields of the headers, Table Header and Footer; may be multi-line.
    pub header: BTreeMap<String, String>,
    pub line_items: Vec<LineItem>,
    /// Values for the Fields of the Totals Block.
    pub totals: BTreeMap<String, String>,
    /// Structured fields of the GiroCode; absent means no GiroCode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub giro_code: Option<GiroCode>,
}

/// Structured fields of the GiroCode (EPC QR code). Checked against the EPC
/// rules when the GiroCode is drawn (issue #8); here only presence and type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GiroCode {
    pub beneficiary_name: String,
    pub iban: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bic: Option<String>,
    /// Decimal amount in EUR, e.g. `"357.00"`.
    pub amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remittance_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creditor_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}

impl InvoiceData {
    /// Parses Invoice Data from JSON text. Errors carry paths below `/invoiceData`.
    pub fn from_json(json: &str) -> Result<Self, Vec<ValidationError>> {
        let _ = json;
        Err(Vec::new())
    }

    /// Parses Invoice Data from a JSON value, reporting every shape error.
    pub fn from_value(value: &Value) -> Result<Self, Vec<ValidationError>> {
        let _ = value;
        Err(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_support::{invoice_data, remove, set};

    fn parse_errors(value: &Value) -> Vec<ValidationError> {
        InvoiceData::from_value(value).expect_err("invoice data should be rejected")
    }

    fn paths(errors: &[ValidationError]) -> Vec<&str> {
        errors.iter().map(|error| error.path.as_str()).collect()
    }

    #[test]
    fn parses_the_test_invoice_data() {
        let parsed = InvoiceData::from_value(&invoice_data()).expect("valid invoice data");
        assert_eq!(parsed.header["invoiceNumber"], "RE-2026-0001");
        assert_eq!(parsed.line_items.len(), 2);
        assert_eq!(parsed.line_items[1]["description"], "Reisekosten");
        assert_eq!(parsed.totals["gross"], "119,00 €");
        let giro_code = parsed.giro_code.expect("giro code");
        assert_eq!(giro_code.beneficiary_name, "ACME GmbH");
        assert_eq!(giro_code.iban, "DE89370400440532013000");
        assert_eq!(giro_code.bic, None);
        assert_eq!(giro_code.amount, "119.00");
        assert_eq!(giro_code.remittance_text.as_deref(), Some("RE-2026-0001"));
    }

    #[test]
    fn keeps_multi_line_header_values() {
        let parsed = InvoiceData::from_value(&invoice_data()).expect("valid invoice data");
        assert_eq!(
            parsed.header["recipient"],
            "Muster GmbH\nHauptstraße 1\n12345 Musterstadt"
        );
    }

    #[test]
    fn parses_from_json_text() {
        let parsed = InvoiceData::from_json(&invoice_data().to_string()).expect("valid");
        assert_eq!(parsed.line_items.len(), 2);
    }

    #[test]
    fn giro_code_is_optional() {
        let mut value = invoice_data();
        remove(&mut value, "/giroCode");
        let parsed = InvoiceData::from_value(&value).expect("valid invoice data");
        assert_eq!(parsed.giro_code, None);
    }

    #[test]
    fn reports_non_string_header_value_with_path() {
        let mut value = invoice_data();
        set(&mut value, "/header/invoiceNumber", json!(2026001));
        let errors = parse_errors(&value);
        assert_eq!(paths(&errors), ["/invoiceData/header/invoiceNumber"]);
        assert_eq!(errors[0].message, "expected a string, found a number");
    }

    #[test]
    fn reports_every_non_string_value_with_its_path() {
        let mut value = invoice_data();
        set(&mut value, "/header/invoiceNumber", json!(null));
        set(&mut value, "/lineItems/0/amount", json!(100.0));
        set(&mut value, "/lineItems/1/description", json!(["Reise"]));
        set(&mut value, "/totals/gross", json!({ "value": "119,00 €" }));
        set(&mut value, "/giroCode/iban", json!(true));
        let errors = parse_errors(&value);
        assert_eq!(
            paths(&errors),
            [
                "/invoiceData/header/invoiceNumber",
                "/invoiceData/lineItems/0/amount",
                "/invoiceData/lineItems/1/description",
                "/invoiceData/totals/gross",
                "/invoiceData/giroCode/iban",
            ]
        );
        let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "expected a string, found null",
                "expected a string, found a number",
                "expected a string, found an array",
                "expected a string, found an object",
                "expected a string, found a boolean",
            ]
        );
    }

    #[test]
    fn reports_non_object_line_item() {
        let mut value = invoice_data();
        set(&mut value, "/lineItems/1", json!("Reisekosten"));
        let errors = parse_errors(&value);
        assert_eq!(paths(&errors), ["/invoiceData/lineItems/1"]);
        assert_eq!(errors[0].message, "expected an object, found a string");
    }

    #[test]
    fn reports_line_items_that_are_not_an_array() {
        let mut value = invoice_data();
        set(&mut value, "/lineItems", json!({ "0": {} }));
        let errors = parse_errors(&value);
        assert_eq!(paths(&errors), ["/invoiceData/lineItems"]);
        assert_eq!(errors[0].message, "expected an array, found an object");
    }

    #[test]
    fn reports_header_and_totals_that_are_not_objects() {
        let mut value = invoice_data();
        set(&mut value, "/header", json!("RE-1"));
        set(&mut value, "/totals", json!([]));
        let errors = parse_errors(&value);
        assert_eq!(
            paths(&errors),
            ["/invoiceData/header", "/invoiceData/totals"]
        );
    }

    #[test]
    fn reports_missing_top_level_members() {
        let errors = parse_errors(&json!({}));
        assert_eq!(
            paths(&errors),
            [
                "/invoiceData/header",
                "/invoiceData/lineItems",
                "/invoiceData/totals",
            ]
        );
        assert_eq!(errors[0].message, "missing required member");
    }

    #[test]
    fn reports_unknown_top_level_member() {
        let mut value = invoice_data();
        set(&mut value, "/meta", json!({}));
        let errors = parse_errors(&value);
        assert_eq!(paths(&errors), ["/invoiceData/meta"]);
        assert!(
            errors[0].message.starts_with("unknown member"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn reports_non_object_invoice_data_at_root() {
        let errors = parse_errors(&json!("invoice"));
        assert_eq!(paths(&errors), ["/invoiceData"]);
        assert_eq!(errors[0].message, "expected an object, found a string");
    }

    #[test]
    fn reports_missing_and_unknown_giro_code_fields() {
        let mut value = invoice_data();
        remove(&mut value, "/giroCode/iban");
        remove(&mut value, "/giroCode/amount");
        set(&mut value, "/giroCode/currency", json!("EUR"));
        let errors = parse_errors(&value);
        assert_eq!(
            paths(&errors),
            [
                "/invoiceData/giroCode/currency",
                "/invoiceData/giroCode/iban",
                "/invoiceData/giroCode/amount",
            ]
        );
    }

    #[test]
    fn reports_non_object_giro_code() {
        let mut value = invoice_data();
        set(&mut value, "/giroCode", json!("BCD"));
        let errors = parse_errors(&value);
        assert_eq!(paths(&errors), ["/invoiceData/giroCode"]);
    }

    #[test]
    fn reports_json_syntax_error_at_root() {
        let errors = InvoiceData::from_json("{\"header\": {,}}").expect_err("syntax error");
        assert_eq!(paths(&errors), ["/invoiceData"]);
        assert!(
            errors[0].message.contains("line 1"),
            "{}",
            errors[0].message
        );
    }
}
