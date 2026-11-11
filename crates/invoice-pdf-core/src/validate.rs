//! The validation pass over a parsed Layout and Invoice Data.

use crate::error::ValidationError;
use crate::invoice_data::InvoiceData;
use crate::layout::Layout;

/// Checks the Layout on its own: sizes, positions and the vertical fit of the zones.
pub fn validate_layout(layout: &Layout) -> Vec<ValidationError> {
    let _ = layout;
    Vec::new()
}

/// Checks the Layout and that the Invoice Data supplies exactly the keys the
/// Layout references. Returns every error found; empty means valid.
pub fn validate(layout: &Layout, invoice_data: &InvoiceData) -> Vec<ValidationError> {
    let _ = invoice_data;
    validate_layout(layout)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::test_support::{error_paths, errors, invoice_data, layout, remove, set};

    /// Paths of all errors after changing one value of the test Layout.
    fn layout_error_paths(pointer: &str, new: Value) -> Vec<String> {
        let mut value = layout();
        set(&mut value, pointer, new);
        error_paths(&value, &invoice_data())
    }

    /// Paths of all errors after changing one value of the test Invoice Data.
    fn data_error_paths(pointer: &str, new: Value) -> Vec<String> {
        let mut value = invoice_data();
        set(&mut value, pointer, new);
        error_paths(&layout(), &value)
    }

    fn message_at(layout: &Value, invoice_data: &Value, path: &str) -> String {
        errors(layout, invoice_data)
            .into_iter()
            .find(|error| error.path == path)
            .unwrap_or_else(|| panic!("no error at {path}"))
            .message
    }

    #[test]
    fn valid_inputs_have_no_errors() {
        assert_eq!(
            error_paths(&layout(), &invoice_data()),
            Vec::<String>::new()
        );
    }

    // --- Layout: margins and Background -------------------------------------

    #[test]
    fn negative_margin_is_an_error() {
        assert_eq!(
            layout_error_paths("/margins/left", json!(-1)),
            ["/layout/margins/left"]
        );
    }

    #[test]
    fn margins_wider_than_the_page_are_an_error() {
        // Columns now also lie outside the content area; the margins error comes first.
        let paths = layout_error_paths("/margins/right", json!(200));
        assert_eq!(paths[0], "/layout/margins");
    }

    #[test]
    fn margins_taller_than_the_page_are_an_error() {
        let paths = layout_error_paths("/margins/bottom", json!(290));
        assert_eq!(paths[0], "/layout/margins");
    }

    #[test]
    fn background_must_use_a_supported_scheme() {
        assert_eq!(
            layout_error_paths("/background", json!("ftp://example.com/acme.png")),
            ["/layout/background"]
        );
        assert_eq!(
            layout_error_paths("/background", json!("")),
            ["/layout/background"]
        );
        for reference in [
            "gs://bucket/acme.png",
            "file://acme.png",
            "https://cdn.example.com/acme.jpg",
        ] {
            assert_eq!(
                layout_error_paths("/background", json!(reference)),
                Vec::<String>::new(),
                "{reference}"
            );
        }
    }

    // --- Layout: sizes and positions of Fields and Static Texts -------------

    #[test]
    fn negative_zone_height_is_an_error() {
        assert_eq!(
            layout_error_paths("/totalsBlock/height", json!(-5)),
            ["/layout/totalsBlock/height"]
        );
    }

    #[test]
    fn font_size_must_be_positive() {
        assert_eq!(
            layout_error_paths("/firstPageHeader/fields/0/size", json!(-10)),
            ["/layout/firstPageHeader/fields/0/size"]
        );
        assert_eq!(
            layout_error_paths("/table/header/staticTexts/1/size", json!(0)),
            ["/layout/table/header/staticTexts/1/size"]
        );
    }

    #[test]
    fn width_must_be_positive() {
        assert_eq!(
            layout_error_paths("/continuationHeader/fields/0/width", json!(-80)),
            ["/layout/continuationHeader/fields/0/width"]
        );
    }

    #[test]
    fn negative_x_is_outside_the_page() {
        assert_eq!(
            layout_error_paths("/firstPageHeader/staticTexts/0/x", json!(-1)),
            ["/layout/firstPageHeader/staticTexts/0/x"]
        );
    }

    #[test]
    fn box_beyond_the_right_page_edge_is_an_error() {
        // x 150 + width 70 = 220 mm > 210 mm.
        let mut value = layout();
        set(&mut value, "/firstPageHeader/fields/0/x", json!(150));
        assert_eq!(
            error_paths(&value, &invoice_data()),
            ["/layout/firstPageHeader/fields/0/x"]
        );
        let message = message_at(
            &value,
            &invoice_data(),
            "/layout/firstPageHeader/fields/0/x",
        );
        assert!(message.contains("outside the page"), "{message}");
    }

    #[test]
    fn negative_y_is_an_error() {
        assert_eq!(
            layout_error_paths("/totalsBlock/fields/0/y", json!(-2)),
            ["/layout/totalsBlock/fields/0/y"]
        );
    }

    #[test]
    fn y_below_the_zone_is_an_error() {
        // The Footer is 15 mm high.
        assert_eq!(
            layout_error_paths("/footer/pageNumber/y", json!(16)),
            ["/layout/footer/pageNumber/y"]
        );
    }

    #[test]
    fn empty_field_source_is_an_error() {
        // The emptied source no longer references `invoiceNumber`, but the
        // Continuation Header still does, so no Invoice Data error follows.
        assert_eq!(
            layout_error_paths("/firstPageHeader/fields/0/source", json!(" ")),
            ["/layout/firstPageHeader/fields/0/source"]
        );
    }

    #[test]
    fn empty_static_text_is_an_error() {
        assert_eq!(
            layout_error_paths("/firstPageHeader/staticTexts/0/text", json!("")),
            ["/layout/firstPageHeader/staticTexts/0/text"]
        );
    }

    #[test]
    fn page_number_must_contain_page_and_pages() {
        assert_eq!(
            layout_error_paths("/footer/pageNumber/text", json!("Seite {page}")),
            ["/layout/footer/pageNumber/text"]
        );
        assert_eq!(
            layout_error_paths("/footer/pageNumber/text", json!("{pages} Seiten")),
            ["/layout/footer/pageNumber/text"]
        );
    }

    // --- Layout: Line Item Table ---------------------------------------------

    #[test]
    fn table_needs_a_column() {
        let mut value = layout();
        set(&mut value, "/table/columns", json!([]));
        let mut data = invoice_data();
        set(&mut data, "/lineItems", json!([]));
        assert_eq!(error_paths(&value, &data), ["/layout/table/columns"]);
    }

    #[test]
    fn column_must_lie_between_the_margins() {
        // Left margin is 20 mm.
        assert_eq!(
            layout_error_paths("/table/columns/0/x", json!(10)),
            ["/layout/table/columns/0/x"]
        );
        // Right margin is 15 mm: 150 + 50 = 200 mm > 195 mm.
        assert_eq!(
            layout_error_paths("/table/columns/1/width", json!(50)),
            ["/layout/table/columns/1/x"]
        );
    }

    #[test]
    fn column_size_and_width_must_be_positive() {
        let mut value = layout();
        set(&mut value, "/table/columns/1/size", json!(-9));
        set(&mut value, "/table/columns/0/width", json!(0));
        assert_eq!(
            error_paths(&value, &invoice_data()),
            [
                "/layout/table/columns/0/width",
                "/layout/table/columns/1/size"
            ]
        );
    }

    #[test]
    fn empty_column_source_is_an_error() {
        let mut value = layout();
        set(&mut value, "/table/columns/1/source", json!(""));
        let mut data = invoice_data();
        remove(&mut data, "/lineItems/0/amount");
        remove(&mut data, "/lineItems/1/amount");
        assert_eq!(
            error_paths(&value, &data),
            ["/layout/table/columns/1/source"]
        );
    }

    #[test]
    fn overlapping_columns_are_an_error() {
        // Column 0 spans 20–120 mm.
        assert_eq!(
            layout_error_paths("/table/columns/1/x", json!(110)),
            ["/layout/table/columns/1/x"]
        );
    }

    #[test]
    fn negative_row_padding_is_an_error() {
        assert_eq!(
            layout_error_paths("/table/rowPadding", json!(-1)),
            ["/layout/table/rowPadding"]
        );
    }

    // --- Layout: vertical fit of the zones -----------------------------------

    #[test]
    fn first_page_needs_room_for_line_items() {
        // 10 + 260 + 8 + 15 + 10 = 303 mm > 297 mm.
        assert_eq!(
            layout_error_paths("/firstPageHeader/height", json!(260)),
            ["/layout/firstPageHeader/height"]
        );
    }

    #[test]
    fn continuation_pages_need_room_for_line_items() {
        // 10 + 254 + 8 + 15 + 10 = 297 mm leaves no room. The Totals Block is
        // made small enough to still fit (10 + 254 + 5 + 15 + 10 = 294 mm).
        let mut value = layout();
        set(&mut value, "/continuationHeader/height", json!(254));
        set(&mut value, "/totalsBlock/height", json!(5));
        assert_eq!(
            error_paths(&value, &invoice_data()),
            ["/layout/continuationHeader/height"]
        );
    }

    #[test]
    fn totals_block_must_fit_on_a_continuation_page() {
        // 10 + 20 + 250 + 15 + 10 = 305 mm > 297 mm.
        assert_eq!(
            layout_error_paths("/totalsBlock/height", json!(250)),
            ["/layout/totalsBlock/height"]
        );
    }

    // --- Layout: GiroCode -----------------------------------------------------

    #[test]
    fn giro_code_must_lie_on_the_page() {
        assert_eq!(
            layout_error_paths("/giroCode/x", json!(190)),
            ["/layout/giroCode/x"]
        );
        assert_eq!(
            layout_error_paths("/giroCode/y", json!(280)),
            ["/layout/giroCode/y"]
        );
        assert_eq!(
            layout_error_paths("/giroCode/size", json!(-25)),
            ["/layout/giroCode/size"]
        );
    }

    // --- Referenced keys ------------------------------------------------------

    #[test]
    fn missing_header_value_is_an_error() {
        let mut data = invoice_data();
        remove(&mut data, "/header/invoiceNumber");
        assert_eq!(
            error_paths(&layout(), &data),
            ["/invoiceData/header/invoiceNumber"]
        );
        let message = message_at(&layout(), &data, "/invoiceData/header/invoiceNumber");
        assert!(
            message.contains("/layout/firstPageHeader/fields/0/source"),
            "{message}"
        );
    }

    #[test]
    fn unknown_header_key_is_an_error() {
        assert_eq!(
            data_error_paths("/header/orderNumber", json!("B-17")),
            ["/invoiceData/header/orderNumber"]
        );
    }

    #[test]
    fn missing_line_item_value_is_reported_with_its_index() {
        let mut data = invoice_data();
        let line_item = json!({ "description": "Beratung", "amount": "1,00 €" });
        set(&mut data, "/lineItems", Value::Array(vec![line_item; 500]));
        remove(&mut data, "/lineItems/417/description");
        assert_eq!(
            error_paths(&layout(), &data),
            ["/invoiceData/lineItems/417/description"]
        );
        let message = message_at(&layout(), &data, "/invoiceData/lineItems/417/description");
        assert!(
            message.contains("/layout/table/columns/0/source"),
            "{message}"
        );
    }

    #[test]
    fn unknown_line_item_key_is_an_error() {
        assert_eq!(
            data_error_paths("/lineItems/1/discount", json!("10 %")),
            ["/invoiceData/lineItems/1/discount"]
        );
    }

    #[test]
    fn missing_and_unknown_totals_keys_are_errors() {
        let mut data = invoice_data();
        remove(&mut data, "/totals/gross");
        set(&mut data, "/totals/brutto", json!("119,00 €"));
        assert_eq!(
            error_paths(&layout(), &data),
            ["/invoiceData/totals/gross", "/invoiceData/totals/brutto"]
        );
    }

    #[test]
    fn table_header_and_footer_fields_read_from_header() {
        let mut value = layout();
        set(
            &mut value,
            "/table/header/fields",
            json!([{ "source": "currency", "x": 20, "y": 1, "width": 20, "size": 8 }]),
        );
        set(
            &mut value,
            "/footer/fields",
            json!([{ "source": "bankDetails", "x": 20, "y": 2, "width": 100, "size": 7 }]),
        );
        assert_eq!(
            error_paths(&value, &invoice_data()),
            [
                "/invoiceData/header/bankDetails",
                "/invoiceData/header/currency"
            ]
        );
    }

    #[test]
    fn giro_code_data_needs_a_giro_code_placement() {
        let mut value = layout();
        remove(&mut value, "/giroCode");
        assert_eq!(
            error_paths(&value, &invoice_data()),
            ["/invoiceData/giroCode"]
        );
    }

    #[test]
    fn giro_code_placement_without_giro_code_data_is_fine() {
        let mut data = invoice_data();
        remove(&mut data, "/giroCode");
        assert_eq!(error_paths(&layout(), &data), Vec::<String>::new());
    }

    #[test]
    fn keys_are_escaped_in_paths() {
        assert_eq!(
            data_error_paths("/header/a~1b", json!("x")),
            ["/invoiceData/header/a~1b"]
        );
    }

    // --- Reporting ------------------------------------------------------------

    #[test]
    fn all_errors_are_reported_together() {
        let mut value = layout();
        set(&mut value, "/margins/top", json!(-1));
        set(&mut value, "/table/columns/0/size", json!(0));
        let mut data = invoice_data();
        remove(&mut data, "/lineItems/0/description");
        set(&mut data, "/lineItems/1/note", json!("x"));
        assert_eq!(
            error_paths(&value, &data),
            [
                "/layout/margins/top",
                "/layout/table/columns/0/size",
                "/invoiceData/lineItems/0/description",
                "/invoiceData/lineItems/1/note",
            ]
        );
    }

    #[test]
    fn layout_is_still_validated_when_invoice_data_has_type_errors() {
        let mut value = layout();
        set(&mut value, "/margins/top", json!(-1));
        let mut data = invoice_data();
        set(&mut data, "/header/invoiceNumber", json!(1));
        assert_eq!(
            error_paths(&value, &data),
            ["/layout/margins/top", "/invoiceData/header/invoiceNumber"]
        );
    }

    #[test]
    fn invoice_data_is_still_parsed_when_the_layout_is_malformed() {
        let mut value = layout();
        set(&mut value, "/margins/top", json!("10mm"));
        let mut data = invoice_data();
        set(&mut data, "/totals/gross", json!(119));
        assert_eq!(
            error_paths(&value, &data),
            ["/layout/margins/top", "/invoiceData/totals/gross"]
        );
    }
}
