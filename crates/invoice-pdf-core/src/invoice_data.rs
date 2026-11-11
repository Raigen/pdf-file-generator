//! The Invoice Data: the per-run content of one Invoice.
//!
//! Every value the Layout draws is a pre-formatted string; the generator never
//! calculates. The Invoice Data may also carry data meant for other services,
//! so parsing only checks the required structure (`header` and `totals` are
//! objects, `lineItems` is an array of objects, `giroCode` has its known
//! fields as strings) and keeps the values of `header`, `totals` and the Line
//! Items as raw JSON. Unknown top-level members and unknown `giroCode` keys are
//! ignored.
//!
//! Which keys must be present, and must be strings, is decided by the Layout:
//! [`crate::validate`] resolves every referenced key and reports a missing or
//! non-string value at its exact path. Keys the Layout does not reference are
//! ignored, whatever their value. After a successful [`crate::load`], every
//! referenced value is a string (see [`text`]).
//!
//! Parsing walks the JSON by hand instead of using derived `Deserialize`, so
//! that every structural error is reported with its own JSON pointer.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{INVOICE_DATA_PATH, ValidationError, join, syntax_error, type_error};

/// Values by key, as given by the Platform. Values the Layout references are
/// checked to be strings by the validation pass; the others may be anything.
pub type Values = BTreeMap<String, Value>;

/// One Line Item: column source key → pre-formatted value.
pub type LineItem = Values;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceData {
    /// Values for the Fields of the headers, Table Header and Footer; may be multi-line.
    pub header: Values,
    pub line_items: Vec<LineItem>,
    /// Values for the Fields of the Totals Block.
    pub totals: Values,
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

/// The string value of `key`, or `None` if it is absent or not a string.
/// For keys the Layout references, validated Invoice Data always has a string.
pub fn text<'a>(values: &'a Values, key: &str) -> Option<&'a str> {
    values.get(key).and_then(Value::as_str)
}

impl InvoiceData {
    /// Parses Invoice Data from JSON text. Errors carry paths below `/invoiceData`.
    pub fn from_json(json: &str) -> Result<Self, Vec<ValidationError>> {
        let value: Value = serde_json::from_str(json)
            .map_err(|error| vec![syntax_error(INVOICE_DATA_PATH, &error)])?;
        Self::from_value(&value)
    }

    /// Parses Invoice Data from a JSON value, reporting every structural error.
    pub fn from_value(value: &Value) -> Result<Self, Vec<ValidationError>> {
        let mut errors = Vec::new();
        let invoice_data = parse_invoice_data(value, &mut errors);
        match invoice_data {
            Some(invoice_data) if errors.is_empty() => Ok(invoice_data),
            _ => Err(errors),
        }
    }
}

fn parse_invoice_data(value: &Value, errors: &mut Vec<ValidationError>) -> Option<InvoiceData> {
    let path = INVOICE_DATA_PATH;
    let members = expect_object(value, path, errors)?;

    let header = required(members, "header", path, errors)
        .and_then(|value| values(value, &join(path, "header"), errors));
    let line_items = required(members, "lineItems", path, errors)
        .and_then(|value| line_items(value, &join(path, "lineItems"), errors));
    let totals = required(members, "totals", path, errors)
        .and_then(|value| values(value, &join(path, "totals"), errors));
    let giro_code = match members.get("giroCode") {
        None => Some(None),
        Some(value) => giro_code(value, &join(path, "giroCode"), errors).map(Some),
    };

    Some(InvoiceData {
        header: header?,
        line_items: line_items?,
        totals: totals?,
        giro_code: giro_code?,
    })
}

fn line_items(
    value: &Value,
    path: &str,
    errors: &mut Vec<ValidationError>,
) -> Option<Vec<LineItem>> {
    let Value::Array(items) = value else {
        errors.push(type_error(path, "an array", value));
        return None;
    };
    // Check every Line Item even after a failure, to report all errors.
    let parsed: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(index, value)| values(value, &join(path, index), errors))
        .collect();
    parsed.into_iter().collect()
}

fn giro_code(value: &Value, path: &str, errors: &mut Vec<ValidationError>) -> Option<GiroCode> {
    let members = expect_object(value, path, errors)?;

    let mut member = |key: &str, is_required: bool| -> Option<Option<String>> {
        match members.get(key) {
            None if is_required => {
                errors.push(missing(&join(path, key)));
                None
            }
            None => Some(None),
            Some(value) => string(value, &join(path, key), errors).map(Some),
        }
    };
    let beneficiary_name = member("beneficiaryName", true);
    let iban = member("iban", true);
    let bic = member("bic", false);
    let amount = member("amount", true);
    let remittance_text = member("remittanceText", false);
    let creditor_reference = member("creditorReference", false);
    let purpose = member("purpose", false);

    Some(GiroCode {
        beneficiary_name: beneficiary_name??,
        iban: iban??,
        bic: bic?,
        amount: amount??,
        remittance_text: remittance_text?,
        creditor_reference: creditor_reference?,
        purpose: purpose?,
    })
}

/// An object whose values are kept as given; their types are checked only for
/// the keys the Layout references.
fn values(value: &Value, path: &str, errors: &mut Vec<ValidationError>) -> Option<Values> {
    let members = expect_object(value, path, errors)?;
    Some(
        members
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

fn string(value: &Value, path: &str, errors: &mut Vec<ValidationError>) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        other => {
            errors.push(type_error(path, "a string", other));
            None
        }
    }
}

fn expect_object<'a>(
    value: &'a Value,
    path: &str,
    errors: &mut Vec<ValidationError>,
) -> Option<&'a Map<String, Value>> {
    match value {
        Value::Object(members) => Some(members),
        other => {
            errors.push(type_error(path, "an object", other));
            None
        }
    }
}

fn required<'a>(
    members: &'a Map<String, Value>,
    key: &str,
    path: &str,
    errors: &mut Vec<ValidationError>,
) -> Option<&'a Value> {
    let value = members.get(key);
    if value.is_none() {
        errors.push(missing(&join(path, key)));
    }
    value
}

fn missing(path: &str) -> ValidationError {
    ValidationError::new(path, "missing required member")
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
    fn keeps_values_of_any_type_for_the_validation_pass() {
        // Whether a value must be a string depends on the Layout: only
        // referenced keys are checked, in `validate`.
        let mut value = invoice_data();
        set(&mut value, "/header/invoiceNumber", json!(2026001));
        set(&mut value, "/lineItems/0/erp", json!({ "id": 1 }));
        set(&mut value, "/totals/taxRate", json!(null));
        let parsed = InvoiceData::from_value(&value).expect("structure is valid");
        assert_eq!(parsed.header["invoiceNumber"], json!(2026001));
        assert_eq!(parsed.line_items[0]["erp"], json!({ "id": 1 }));
        assert_eq!(parsed.totals["taxRate"], Value::Null);
        assert_eq!(text(&parsed.header, "invoiceNumber"), None);
        assert_eq!(
            text(&parsed.header, "recipient"),
            Some("Muster GmbH\nHauptstraße 1\n12345 Musterstadt")
        );
    }

    #[test]
    fn reports_every_non_string_giro_code_field_with_its_path() {
        let mut value = invoice_data();
        set(&mut value, "/giroCode/iban", json!(true));
        set(&mut value, "/giroCode/bic", json!(null));
        set(&mut value, "/giroCode/amount", json!(119.0));
        let errors = parse_errors(&value);
        let reported: Vec<_> = errors
            .iter()
            .map(|error| (error.path.as_str(), error.message.as_str()))
            .collect();
        assert_eq!(
            reported,
            [
                (
                    "/invoiceData/giroCode/iban",
                    "expected a string, found a boolean"
                ),
                ("/invoiceData/giroCode/bic", "expected a string, found null"),
                (
                    "/invoiceData/giroCode/amount",
                    "expected a string, found a number"
                ),
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
    fn ignores_unknown_top_level_members() {
        let mut value = invoice_data();
        set(&mut value, "/meta", json!({ "requestId": 17 }));
        set(&mut value, "/dunning", json!([1, 2]));
        assert!(InvoiceData::from_value(&value).is_ok());
    }

    #[test]
    fn reports_non_object_invoice_data_at_root() {
        let errors = parse_errors(&json!("invoice"));
        assert_eq!(paths(&errors), ["/invoiceData"]);
        assert_eq!(errors[0].message, "expected an object, found a string");
    }

    #[test]
    fn reports_missing_giro_code_fields_and_ignores_unknown_ones() {
        let mut value = invoice_data();
        remove(&mut value, "/giroCode/iban");
        remove(&mut value, "/giroCode/amount");
        set(&mut value, "/giroCode/currency", json!(["EUR"]));
        let errors = parse_errors(&value);
        assert_eq!(
            paths(&errors),
            ["/invoiceData/giroCode/iban", "/invoiceData/giroCode/amount"]
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
