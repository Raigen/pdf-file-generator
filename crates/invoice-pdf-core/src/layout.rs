//! The Layout: the reusable visual design of an Invoice.
//!
//! All lengths are millimetres, font sizes are points. The page is A4 portrait
//! (210 mm × 297 mm) with the origin in the top-left corner and y growing down.
//!
//! Zones are stacked vertically: the First-Page Header (page one) or the
//! Continuation Header (later pages) starts at the top margin, the Table Header
//! follows directly below it, the Footer ends at the bottom margin and the
//! Totals Block follows the last Line Item. Inside a zone, `x` is measured from
//! the left page edge and `y` from the top of the zone.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{LAYOUT_PATH, ValidationError, from_serde_path, syntax_error, type_error};

/// Width of an A4 portrait page in mm.
pub const PAGE_WIDTH_MM: f64 = 210.0;
/// Height of an A4 portrait page in mm.
pub const PAGE_HEIGHT_MM: f64 = 297.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Layout {
    /// Always A4 portrait; may be omitted.
    #[serde(default)]
    pub page_size: PageSize,
    pub margins: Margins,
    /// Reference to the Background image (`gs://`, `file://` or `https://`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    pub first_page_header: Zone,
    pub continuation_header: Zone,
    pub table: LineItemTable,
    pub totals_block: Zone,
    pub footer: Footer,
    /// Where the GiroCode is drawn on the last page; absent means no GiroCode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub giro_code: Option<GiroCodePlacement>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageSize {
    /// A4 portrait, 210 mm × 297 mm.
    #[default]
    A4,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Margins {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// A horizontal band of the page made of Fields and Static Texts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Zone {
    pub height: f64,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub static_texts: Vec<StaticText>,
}

/// The Footer zone: like [`Zone`], plus the page number ("page X of Y").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Footer {
    pub height: f64,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub static_texts: Vec<StaticText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_number: Option<PageNumber>,
}

/// A place where one value from the Invoice Data is drawn.
///
/// Fields of the First-Page Header, Continuation Header, Table Header and
/// Footer read from `invoiceData.header`; Fields of the Totals Block read from
/// `invoiceData.totals`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Field {
    /// Key of the value in the Invoice Data.
    pub source: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    #[serde(default)]
    pub font: Font,
    /// Font size in pt.
    pub size: f64,
    #[serde(default)]
    pub align: Align,
}

/// Fixed wording defined by the Layout itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StaticText {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    #[serde(default)]
    pub font: Font,
    /// Font size in pt.
    pub size: f64,
    #[serde(default)]
    pub align: Align,
}

/// The page number in the Footer. `text` must contain `{page}` and `{pages}`,
/// which are replaced by the current page number and the page count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageNumber {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    #[serde(default)]
    pub font: Font,
    /// Font size in pt.
    pub size: f64,
    #[serde(default)]
    pub align: Align,
}

/// The Line Item Table: its Table Header zone and its columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineItemTable {
    /// The Table Header, repeated at the top of the Line Item Table on every page.
    pub header: Zone,
    pub columns: Vec<Column>,
    /// Vertical space added above and below every Line Item, in mm.
    #[serde(default)]
    pub row_padding: f64,
}

/// One column of the Line Item Table; draws one value of every Line Item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Column {
    /// Key of the value in each Line Item.
    pub source: String,
    pub x: f64,
    pub width: f64,
    #[serde(default)]
    pub font: Font,
    /// Font size in pt.
    pub size: f64,
    #[serde(default)]
    pub align: Align,
}

/// Square area of the GiroCode (quiet zone included), on the last page.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GiroCodePlacement {
    pub x: f64,
    pub y: f64,
    /// Edge length in mm.
    pub size: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Font {
    #[default]
    Regular,
    Bold,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

impl Layout {
    /// Parses a Layout from JSON text. Errors carry paths below `/layout`.
    pub fn from_json(json: &str) -> Result<Self, Vec<ValidationError>> {
        let value: Value =
            serde_json::from_str(json).map_err(|error| vec![syntax_error(LAYOUT_PATH, &error)])?;
        Self::from_value(&value)
    }

    /// Parses a Layout from a JSON value. Errors carry paths below `/layout`.
    ///
    /// The structure is checked by serde, which stops at the first mismatch, so
    /// this returns at most one error; its path points at the offending value
    /// (or, for a missing key, at the object that lacks it).
    pub fn from_value(value: &Value) -> Result<Self, Vec<ValidationError>> {
        if !value.is_object() {
            return Err(vec![type_error(LAYOUT_PATH, "an object", value)]);
        }
        serde_path_to_error::deserialize(value).map_err(|error| {
            let path = from_serde_path(LAYOUT_PATH, error.path());
            vec![ValidationError::new(path, error.into_inner().to_string())]
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_support::{invoice_data, layout, remove, set};

    fn parse_errors(value: &Value) -> Vec<ValidationError> {
        Layout::from_value(value).expect_err("layout should be rejected")
    }

    #[test]
    fn parses_the_test_layout() {
        let parsed = Layout::from_value(&layout()).expect("valid layout");
        assert_eq!(parsed.page_size, PageSize::A4);
        assert_eq!(parsed.margins.left, 20.0);
        assert_eq!(
            parsed.background.as_deref(),
            Some("file://backgrounds/acme.png")
        );
        assert_eq!(parsed.first_page_header.fields[0].source, "invoiceNumber");
        assert_eq!(parsed.first_page_header.fields[0].font, Font::Regular);
        assert_eq!(parsed.first_page_header.fields[0].align, Align::Left);
        assert_eq!(parsed.first_page_header.static_texts[0].font, Font::Bold);
        assert_eq!(parsed.table.columns[1].align, Align::Right);
        assert_eq!(parsed.table.row_padding, 1.0);
        let page_number = parsed.footer.page_number.expect("page number");
        assert_eq!(page_number.text, "Seite {page} von {pages}");
        assert_eq!(
            parsed.giro_code,
            Some(GiroCodePlacement {
                x: 20.0,
                y: 230.0,
                size: 25.0
            })
        );
    }

    #[test]
    fn parses_from_json_text() {
        let parsed = Layout::from_json(&layout().to_string()).expect("valid layout");
        assert_eq!(parsed.table.columns.len(), 2);
    }

    #[test]
    fn accepts_explicit_a4_page_size() {
        let mut value = layout();
        set(&mut value, "/pageSize", json!("A4"));
        assert!(Layout::from_value(&value).is_ok());
    }

    #[test]
    fn rejects_other_page_sizes_with_path() {
        let mut value = layout();
        set(&mut value, "/pageSize", json!("A5"));
        let errors = parse_errors(&value);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/layout/pageSize");
        assert!(errors[0].message.contains("A5"), "{}", errors[0].message);
    }

    #[test]
    fn reports_wrong_type_with_path_of_the_value() {
        let mut value = layout();
        set(&mut value, "/table/columns/1/size", json!("9pt"));
        let errors = parse_errors(&value);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/layout/table/columns/1/size");
        assert!(
            errors[0].message.contains("invalid type"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn reports_unknown_font_with_path() {
        let mut value = layout();
        set(
            &mut value,
            "/firstPageHeader/staticTexts/0/font",
            json!("italic"),
        );
        let errors = parse_errors(&value);
        assert_eq!(errors[0].path, "/layout/firstPageHeader/staticTexts/0/font");
        assert!(
            errors[0].message.contains("italic"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn reports_unknown_key_with_its_path() {
        let mut value = layout();
        set(&mut value, "/footer/colour", json!("red"));
        let errors = parse_errors(&value);
        assert_eq!(errors[0].path, "/layout/footer/colour");
        assert!(
            errors[0].message.contains("unknown field `colour`"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn reports_missing_key_at_its_object() {
        let mut value = layout();
        remove(&mut value, "/table/columns/0/source");
        let errors = parse_errors(&value);
        assert_eq!(errors[0].path, "/layout/table/columns/0");
        assert!(
            errors[0].message.contains("missing field `source`"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn reports_non_object_layout_at_root() {
        let errors = parse_errors(&json!([1, 2]));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/layout");
        assert_eq!(errors[0].message, "expected an object, found an array");
    }

    #[test]
    fn reports_json_syntax_error_at_root() {
        let errors = Layout::from_json("{\"margins\": ").expect_err("syntax error");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/layout");
        assert!(
            errors[0].message.contains("line 1"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn test_fixtures_are_consistent() {
        // Guards the shared test inputs themselves.
        assert!(crate::load_values(&layout(), &invoice_data()).is_ok());
    }
}
