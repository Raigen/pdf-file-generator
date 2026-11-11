//! Minimal valid inputs and helpers shared by the unit tests.

use serde_json::{Value, json};

use crate::error::ValidationError;

/// A small valid Layout: margins top 10 / right 15 / bottom 10 / left 20.
pub fn layout() -> Value {
    json!({
        "margins": { "top": 10, "right": 15, "bottom": 10, "left": 20 },
        "background": "file://backgrounds/acme.png",
        "firstPageHeader": {
            "height": 80,
            "fields": [
                { "source": "invoiceNumber", "x": 120, "y": 40, "width": 70, "size": 10 },
                { "source": "recipient", "x": 20, "y": 30, "width": 80, "size": 10 }
            ],
            "staticTexts": [
                { "text": "Rechnung", "x": 20, "y": 10, "width": 80, "size": 16, "font": "bold" }
            ]
        },
        "continuationHeader": {
            "height": 20,
            "fields": [
                { "source": "invoiceNumber", "x": 20, "y": 5, "width": 80, "size": 9 }
            ]
        },
        "table": {
            "header": {
                "height": 8,
                "staticTexts": [
                    { "text": "Beschreibung", "x": 20, "y": 1, "width": 100, "size": 9, "font": "bold" },
                    { "text": "Betrag", "x": 150, "y": 1, "width": 45, "size": 9, "font": "bold", "align": "right" }
                ]
            },
            "columns": [
                { "source": "description", "x": 20, "width": 100, "size": 9 },
                { "source": "amount", "x": 150, "width": 45, "size": 9, "align": "right" }
            ],
            "rowPadding": 1
        },
        "totalsBlock": {
            "height": 20,
            "fields": [
                { "source": "gross", "x": 150, "y": 2, "width": 45, "size": 10, "align": "right" }
            ]
        },
        "footer": {
            "height": 15,
            "pageNumber": {
                "text": "Seite {page} von {pages}", "x": 150, "y": 5, "width": 45, "size": 8, "align": "right"
            }
        },
        "giroCode": { "x": 20, "y": 230, "size": 25 }
    })
}

/// Invoice Data matching [`layout`].
pub fn invoice_data() -> Value {
    json!({
        "header": {
            "invoiceNumber": "RE-2026-0001",
            "recipient": "Muster GmbH\nHauptstraße 1\n12345 Musterstadt"
        },
        "lineItems": [
            { "description": "Beratung", "amount": "100,00 €" },
            { "description": "Reisekosten", "amount": "19,00 €" }
        ],
        "totals": { "gross": "119,00 €" },
        "giroCode": {
            "beneficiaryName": "ACME GmbH",
            "iban": "DE89370400440532013000",
            "amount": "119.00",
            "remittanceText": "RE-2026-0001"
        }
    })
}

/// Replaces the value at `pointer` (relative to `value`), creating object members as needed.
pub fn set(value: &mut Value, pointer: &str, new: Value) {
    let (parent, key) = pointer.rsplit_once('/').expect("pointer must contain '/'");
    let parent = value
        .pointer_mut(parent)
        .unwrap_or_else(|| panic!("no value at {parent}"));
    match parent {
        Value::Object(map) => {
            map.insert(key.to_owned(), new);
        }
        Value::Array(items) => items[key.parse::<usize>().expect("array index")] = new,
        other => panic!("cannot set {key} on {other}"),
    }
}

/// Removes the member at `pointer` (relative to `value`).
pub fn remove(value: &mut Value, pointer: &str) {
    let (parent, key) = pointer.rsplit_once('/').expect("pointer must contain '/'");
    let removed = value
        .pointer_mut(parent)
        .and_then(Value::as_object_mut)
        .and_then(|map| map.remove(key));
    assert!(removed.is_some(), "nothing to remove at {pointer}");
}

/// Loads both inputs and returns the paths of all errors (empty when valid).
pub fn error_paths(layout: &Value, invoice_data: &Value) -> Vec<String> {
    errors(layout, invoice_data)
        .into_iter()
        .map(|error| error.path)
        .collect()
}

/// Loads both inputs and returns all errors (empty when valid).
pub fn errors(layout: &Value, invoice_data: &Value) -> Vec<ValidationError> {
    match crate::load_values(layout, invoice_data) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}
