//! The validation pass over a parsed Layout and Invoice Data.
//!
//! Errors are collected, never short-circuited, so one pass reports
//! everything that is wrong. Layout errors come first, then Invoice Data
//! errors in document order (header, Line Items, totals, GiroCode).

use std::collections::BTreeMap;

use crate::error::{INVOICE_DATA_PATH, LAYOUT_PATH, ValidationError, join};
use crate::invoice_data::InvoiceData;
use crate::layout::{Field, Layout, PAGE_HEIGHT_MM, PAGE_WIDTH_MM, StaticText, Zone};

/// Background reference schemes the Background loader can resolve (issue #5).
const BACKGROUND_SCHEMES: [&str; 3] = ["gs://", "file://", "https://"];

/// Checks the Layout on its own: sizes, positions and the vertical fit of the zones.
pub fn validate_layout(layout: &Layout) -> Vec<ValidationError> {
    let mut errors = Errors::default();
    errors.check_layout(layout);
    errors.0
}

/// Checks the Layout and that the Invoice Data supplies exactly the keys the
/// Layout references: a referenced key that is missing is an error (it is
/// never drawn as empty), and so is a key that no Field or column references.
/// Returns every error found; empty means valid.
pub fn validate(layout: &Layout, invoice_data: &InvoiceData) -> Vec<ValidationError> {
    let mut errors = Errors::default();
    errors.check_layout(layout);
    errors.check_references(layout, invoice_data);
    errors.0
}

/// The parts of a Field, Static Text or page number that are checked alike.
struct TextBox {
    x: f64,
    y: f64,
    width: f64,
    size: f64,
}

impl From<&Field> for TextBox {
    fn from(field: &Field) -> Self {
        Self {
            x: field.x,
            y: field.y,
            width: field.width,
            size: field.size,
        }
    }
}

impl From<&StaticText> for TextBox {
    fn from(text: &StaticText) -> Self {
        Self {
            x: text.x,
            y: text.y,
            width: text.width,
            size: text.size,
        }
    }
}

#[derive(Default)]
struct Errors(Vec<ValidationError>);

impl Errors {
    fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.0.push(ValidationError::new(path, message));
    }

    fn check_layout(&mut self, layout: &Layout) {
        self.check_margins(layout);
        self.check_background(layout);

        self.check_zone(
            &join(LAYOUT_PATH, "firstPageHeader"),
            &layout.first_page_header,
        );
        self.check_zone(
            &join(LAYOUT_PATH, "continuationHeader"),
            &layout.continuation_header,
        );
        self.check_table(layout);
        self.check_zone(&join(LAYOUT_PATH, "totalsBlock"), &layout.totals_block);
        self.check_footer(layout);
        self.check_vertical_fit(layout);
        self.check_giro_code_placement(layout);
    }

    fn check_margins(&mut self, layout: &Layout) {
        let path = join(LAYOUT_PATH, "margins");
        let margins = &layout.margins;
        for (side, value) in [
            ("top", margins.top),
            ("right", margins.right),
            ("bottom", margins.bottom),
            ("left", margins.left),
        ] {
            self.non_negative(&join(&path, side), value);
        }
        if margins.left + margins.right >= PAGE_WIDTH_MM {
            self.push(
                &path,
                format!(
                    "left and right margins ({} mm + {} mm) leave no room on the {PAGE_WIDTH_MM} mm wide page",
                    margins.left, margins.right
                ),
            );
        }
        if margins.top + margins.bottom >= PAGE_HEIGHT_MM {
            self.push(
                &path,
                format!(
                    "top and bottom margins ({} mm + {} mm) leave no room on the {PAGE_HEIGHT_MM} mm high page",
                    margins.top, margins.bottom
                ),
            );
        }
    }

    fn check_background(&mut self, layout: &Layout) {
        let Some(reference) = &layout.background else {
            return;
        };
        let supported = BACKGROUND_SCHEMES
            .iter()
            .any(|scheme| reference.len() > scheme.len() && reference.starts_with(scheme));
        if !supported {
            self.push(
                join(LAYOUT_PATH, "background"),
                format!(
                    "Background reference `{reference}` must start with one of {} and name an image",
                    BACKGROUND_SCHEMES.join(", ")
                ),
            );
        }
    }

    fn check_zone(&mut self, path: &str, zone: &Zone) {
        self.check_zone_contents(path, zone.height, &zone.fields, &zone.static_texts);
    }

    fn check_zone_contents(
        &mut self,
        path: &str,
        height: f64,
        fields: &[Field],
        static_texts: &[StaticText],
    ) {
        self.non_negative(&join(path, "height"), height);
        for (index, field) in fields.iter().enumerate() {
            let field_path = join(&join(path, "fields"), index);
            self.not_blank(&join(&field_path, "source"), &field.source);
            self.check_text_box(&field_path, &field.into(), height);
        }
        for (index, text) in static_texts.iter().enumerate() {
            let text_path = join(&join(path, "staticTexts"), index);
            self.not_blank(&join(&text_path, "text"), &text.text);
            self.check_text_box(&text_path, &text.into(), height);
        }
    }

    fn check_footer(&mut self, layout: &Layout) {
        let path = join(LAYOUT_PATH, "footer");
        let footer = &layout.footer;
        self.check_zone_contents(&path, footer.height, &footer.fields, &footer.static_texts);
        if let Some(page_number) = &footer.page_number {
            let page_number_path = join(&path, "pageNumber");
            if !page_number.text.contains("{page}") || !page_number.text.contains("{pages}") {
                self.push(
                    join(&page_number_path, "text"),
                    "must contain `{page}` and `{pages}` for the form \"page X of Y\"",
                );
            }
            let text_box = TextBox {
                x: page_number.x,
                y: page_number.y,
                width: page_number.width,
                size: page_number.size,
            };
            self.check_text_box(&page_number_path, &text_box, footer.height);
        }
    }

    fn check_table(&mut self, layout: &Layout) {
        let path = join(LAYOUT_PATH, "table");
        let table = &layout.table;
        self.check_zone(&join(&path, "header"), &table.header);
        self.non_negative(&join(&path, "rowPadding"), table.row_padding);

        let columns_path = join(&path, "columns");
        if table.columns.is_empty() {
            self.push(
                &columns_path,
                "the Line Item Table needs at least one column",
            );
        }
        let left = layout.margins.left;
        let right = PAGE_WIDTH_MM - layout.margins.right;
        for (index, column) in table.columns.iter().enumerate() {
            let column_path = join(&columns_path, index);
            self.not_blank(&join(&column_path, "source"), &column.source);
            self.positive(&join(&column_path, "width"), column.width, "mm");
            self.positive(&join(&column_path, "size"), column.size, "pt");
            if column.x < left || column.x + column.width.max(0.0) > right {
                self.push(
                    join(&column_path, "x"),
                    format!(
                        "column from {} mm to {} mm lies outside the margins ({left} mm to {right} mm)",
                        column.x,
                        column.x + column.width
                    ),
                );
            }
            let overlapped = table.columns[..index].iter().position(|earlier| {
                column.width > 0.0
                    && earlier.width > 0.0
                    && column.x < earlier.x + earlier.width
                    && earlier.x < column.x + column.width
            });
            if let Some(earlier) = overlapped {
                self.push(
                    join(&column_path, "x"),
                    format!("overlaps column {earlier}"),
                );
            }
        }
    }

    /// The table needs room on every page, and the Totals Block must fit on a
    /// page of its own (Continuation Header, Totals Block, Footer).
    fn check_vertical_fit(&mut self, layout: &Layout) {
        let margins = layout.margins.top + layout.margins.bottom;
        let table_header = layout.table.header.height;
        let footer = layout.footer.height;

        for (zone, height, name, page) in [
            (
                "firstPageHeader",
                layout.first_page_header.height,
                "First-Page Header",
                "the first page",
            ),
            (
                "continuationHeader",
                layout.continuation_header.height,
                "Continuation Header",
                "continuation pages",
            ),
        ] {
            let used = margins + height + table_header + footer;
            if used >= PAGE_HEIGHT_MM {
                self.push(
                    join(&join(LAYOUT_PATH, zone), "height"),
                    format!(
                        "margins, {name}, Table Header and Footer take {used} mm of the {PAGE_HEIGHT_MM} mm page and leave no room for Line Items on {page}"
                    ),
                );
            }
        }

        let used =
            margins + layout.continuation_header.height + layout.totals_block.height + footer;
        if used > PAGE_HEIGHT_MM {
            self.push(
                join(&join(LAYOUT_PATH, "totalsBlock"), "height"),
                format!(
                    "margins, Continuation Header, Totals Block and Footer take {used} mm, so the Totals Block does not fit on a {PAGE_HEIGHT_MM} mm page"
                ),
            );
        }
    }

    fn check_giro_code_placement(&mut self, layout: &Layout) {
        let Some(placement) = &layout.giro_code else {
            return;
        };
        let path = join(LAYOUT_PATH, "giroCode");
        self.positive(&join(&path, "size"), placement.size, "mm");
        let size = placement.size.max(0.0);
        if placement.x < 0.0 || placement.x + size > PAGE_WIDTH_MM {
            self.push(
                join(&path, "x"),
                format!(
                    "GiroCode from {} mm to {} mm lies outside the page (0 mm to {PAGE_WIDTH_MM} mm)",
                    placement.x,
                    placement.x + size
                ),
            );
        }
        if placement.y < 0.0 || placement.y + size > PAGE_HEIGHT_MM {
            self.push(
                join(&path, "y"),
                format!(
                    "GiroCode from {} mm to {} mm lies outside the page (0 mm to {PAGE_HEIGHT_MM} mm)",
                    placement.y,
                    placement.y + size
                ),
            );
        }
    }

    /// `x` is measured from the left page edge, `y` from the top of the zone.
    fn check_text_box(&mut self, path: &str, text_box: &TextBox, zone_height: f64) {
        self.positive(&join(path, "width"), text_box.width, "mm");
        self.positive(&join(path, "size"), text_box.size, "pt");
        let end = text_box.x + text_box.width.max(0.0);
        if text_box.x < 0.0 || end > PAGE_WIDTH_MM {
            self.push(
                join(path, "x"),
                format!(
                    "box from {} mm to {end} mm lies outside the page (0 mm to {PAGE_WIDTH_MM} mm)",
                    text_box.x
                ),
            );
        }
        if text_box.y < 0.0 {
            self.push(
                join(path, "y"),
                format!("must not be negative (got {} mm)", text_box.y),
            );
        } else if zone_height >= 0.0 && text_box.y > zone_height {
            // A negative zone height is already reported on the zone itself.
            self.push(
                join(path, "y"),
                format!(
                    "lies below the zone (y = {} mm, zone height = {zone_height} mm)",
                    text_box.y
                ),
            );
        }
    }

    fn non_negative(&mut self, path: &str, value: f64) {
        if value < 0.0 {
            self.push(path, format!("must not be negative (got {value} mm)"));
        }
    }

    fn positive(&mut self, path: &str, value: f64, unit: &str) {
        if value <= 0.0 {
            self.push(
                path,
                format!("must be greater than 0 {unit} (got {value} {unit})"),
            );
        }
    }

    fn not_blank(&mut self, path: &str, value: &str) {
        if value.trim().is_empty() {
            self.push(path, "must not be empty");
        }
    }

    fn check_references(&mut self, layout: &Layout, invoice_data: &InvoiceData) {
        // Fields reading from `header`: both headers, the Table Header and the Footer.
        let mut header_sources = Sources::default();
        for (zone, fields) in [
            ("firstPageHeader", &layout.first_page_header.fields),
            ("continuationHeader", &layout.continuation_header.fields),
            ("table/header", &layout.table.header.fields),
            ("footer", &layout.footer.fields),
        ] {
            header_sources.add_fields(&format!("{LAYOUT_PATH}/{zone}"), fields);
        }
        let mut totals_sources = Sources::default();
        totals_sources.add_fields(
            &join(LAYOUT_PATH, "totalsBlock"),
            &layout.totals_block.fields,
        );
        let mut column_sources = Sources::default();
        let columns_path = join(&join(LAYOUT_PATH, "table"), "columns");
        for (index, column) in layout.table.columns.iter().enumerate() {
            column_sources.add(&column.source, join(&join(&columns_path, index), "source"));
        }

        self.check_keys(
            &join(INVOICE_DATA_PATH, "header"),
            invoice_data.header.keys(),
            &header_sources,
            "Field",
        );
        let line_items_path = join(INVOICE_DATA_PATH, "lineItems");
        for (index, line_item) in invoice_data.line_items.iter().enumerate() {
            self.check_keys(
                &join(&line_items_path, index),
                line_item.keys(),
                &column_sources,
                "Line Item Table column",
            );
        }
        self.check_keys(
            &join(INVOICE_DATA_PATH, "totals"),
            invoice_data.totals.keys(),
            &totals_sources,
            "Field of the Totals Block",
        );

        if invoice_data.giro_code.is_some() && layout.giro_code.is_none() {
            self.push(
                join(INVOICE_DATA_PATH, "giroCode"),
                "the Layout has no GiroCode placement (`giroCode`), so the GiroCode cannot be drawn",
            );
        }
    }

    /// Reports referenced keys missing from `keys` and keys nobody references.
    fn check_keys<'a>(
        &mut self,
        path: &str,
        keys: impl Iterator<Item = &'a String> + Clone,
        sources: &Sources,
        referenced_by: &str,
    ) {
        for (source, source_path) in &sources.0 {
            if !keys.clone().any(|key| key == source) {
                self.push(
                    join(path, source),
                    format!(
                        "missing value for the {referenced_by} at {source_path}; referenced keys must be present"
                    ),
                );
            }
        }
        for key in keys.filter(|key| !sources.0.contains_key(key.as_str())) {
            self.push(
                join(path, key),
                format!("not referenced by any {referenced_by} of the Layout"),
            );
        }
    }
}

/// Keys referenced by the Layout, each with the path of its first reference.
#[derive(Default)]
struct Sources(BTreeMap<String, String>);

impl Sources {
    fn add(&mut self, source: &str, path: String) {
        if !source.trim().is_empty() {
            self.0.entry(source.to_owned()).or_insert(path);
        }
    }

    fn add_fields(&mut self, zone_path: &str, fields: &[Field]) {
        for (index, field) in fields.iter().enumerate() {
            let path = join(&join(&join(zone_path, "fields"), index), "source");
            self.add(&field.source, path);
        }
    }
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
        let mut data = invoice_data();
        data["header"]["net/gross~ratio"] = json!("x");
        assert_eq!(
            error_paths(&layout(), &data),
            ["/invoiceData/header/net~1gross~0ratio"]
        );
    }

    // --- Unreferenced Invoice Data is ignored --------------------------------

    #[test]
    fn unreferenced_keys_are_ignored_whatever_their_value() {
        let mut data = invoice_data();
        data["header"]["orderNumber"] = json!("B-2026-17");
        data["header"]["erp"] = json!({ "id": 4711, "tags": ["a"] });
        data["totals"]["currency"] = json!("EUR");
        data["totals"]["taxRate"] = json!(0.19);
        data["lineItems"][0]["discount"] = json!(null);
        data["lineItems"][1]["sku"] = json!("SKU-1");
        data["lineItems"][1]["tags"] = json!(["travel", true]);
        assert_eq!(error_paths(&layout(), &data), Vec::<String>::new());
    }

    #[test]
    fn unknown_top_level_and_giro_code_keys_are_ignored() {
        let mut data = invoice_data();
        data["meta"] = json!({ "requestId": 17 });
        data["dunning"] = json!([1, 2]);
        data["giroCode"]["currency"] = json!("EUR");
        data["giroCode"]["internal"] = json!({ "x": null });
        assert_eq!(error_paths(&layout(), &data), Vec::<String>::new());
    }

    #[test]
    fn referenced_non_string_value_is_reported_at_its_exact_path() {
        let mut data = invoice_data();
        let line_item = json!({ "description": "Beratung", "amount": "1,00 €", "erpId": 1 });
        set(&mut data, "/lineItems", Value::Array(vec![line_item; 500]));
        data["lineItems"][417]["description"] = json!(4711);
        data["header"]["invoiceNumber"] = json!(null);
        data["totals"]["gross"] = json!({ "value": "119,00 €" });
        data["totals"]["note"] = json!(false);
        let found = errors(&layout(), &data);
        let reported: Vec<_> = found
            .iter()
            .map(|error| (error.path.as_str(), error.message.as_str()))
            .collect();
        assert_eq!(
            reported,
            [
                (
                    "/invoiceData/header/invoiceNumber",
                    "expected a string, found null"
                ),
                (
                    "/invoiceData/lineItems/417/description",
                    "expected a string, found a number"
                ),
                (
                    "/invoiceData/totals/gross",
                    "expected a string, found an object"
                ),
            ]
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
