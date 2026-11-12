//! The pagination pass: turns a validated Layout and Invoice Data into a
//! [`PagePlan`] before any PDF bytes are written.
//!
//! Every page is stacked top to bottom from the top margin: the First-Page
//! Header (page one) or the Continuation Header (later pages), then the Table
//! Header and the Line Items, then (on the last page) the Totals Block; the
//! Footer ends at the bottom margin. The Line Item Table may use the space
//! between the bottom of the Table Header and the top of the Footer.
//!
//! - A Line Item is as tall as its tallest wrapped cell plus the Layout's
//!   `rowPadding` above and below it, measured with the embedded fonts at
//!   [`LINE_HEIGHT`].
//! - Line Items are assigned to pages greedily in order and never split. A
//!   Line Item taller than the table area of an empty continuation page can
//!   never be placed and is reported as a [`ValidationError`] at its path,
//!   e.g. `/invoiceData/lineItems/417`.
//! - The Totals Block follows the last Line Item directly. If it does not
//!   fit there, it goes on a page of its own: Continuation Header, no Table
//!   Header, no Line Items.
//! - With no Line Items, page one still shows the Table Header and the Totals
//!   Block follows directly below it.
//!
//! All y positions are absolute, in mm from the top edge of the page (the
//! Layout's coordinate system).

use std::ops::Range;

use crate::error::{INVOICE_DATA_PATH, ValidationError, join};
use crate::font::Fonts;
use crate::invoice_data::{InvoiceData, text};
use crate::layout::{Layout, PAGE_HEIGHT_MM};

/// Line-height factor for all text: lines are `LINE_HEIGHT` × the font size
/// apart. The Layout has no line height of its own yet, so pagination and the
/// renderer share this one.
pub const LINE_HEIGHT: f64 = 1.2;

/// Where every zone and every Line Item of an Invoice goes.
#[derive(Debug, Clone, PartialEq)]
pub struct PagePlan {
    /// The pages in order; never empty.
    pub pages: Vec<Page>,
}

/// One page of a [`PagePlan`].
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// Which header zone the page starts with, at the top margin.
    pub header: PageHeader,
    /// Top of the Table Header, or `None` if the page has no Line Item Table
    /// (a page holding only the Totals Block).
    pub table_header_y: Option<f64>,
    /// The Line Items on this page, in order, with their positions.
    pub line_items: Vec<PlacedLineItem>,
    /// Top of the Totals Block, if it is on this page.
    pub totals_block_y: Option<f64>,
}

/// The header zone a page starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageHeader {
    /// The First-Page Header, on page one only.
    FirstPage,
    /// The Continuation Header, on every later page.
    Continuation,
}

/// A Line Item's place on its page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedLineItem {
    /// Index of the Line Item in the Invoice Data.
    pub index: usize,
    /// Top of the Line Item, including its padding.
    pub y: f64,
    /// Height of the Line Item, including padding above and below.
    pub height: f64,
}

impl PagePlan {
    /// Number of pages, the Y in "page X of Y".
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}

impl Page {
    /// Indices of the Line Items on this page (empty if there are none).
    pub fn line_item_range(&self) -> Range<usize> {
        match (self.line_items.first(), self.line_items.last()) {
            (Some(first), Some(last)) => first.index..last.index + 1,
            _ => 0..0,
        }
    }
}

/// Slack when checking that something fits above the Footer, in mm, so that
/// heights adding up to exactly the available space still fit despite
/// floating-point rounding. Far below anything visible (1 µm).
const FIT_TOLERANCE_MM: f64 = 1e-6;

/// Height of Line Item `index` in mm: its tallest wrapped cell plus the
/// `rowPadding` above and below.
pub fn line_item_height(
    layout: &Layout,
    invoice_data: &InvoiceData,
    fonts: &Fonts,
    index: usize,
) -> f64 {
    let line_item = &invoice_data.line_items[index];
    let tallest_cell = layout
        .table
        .columns
        .iter()
        .map(|column| {
            let value = text(line_item, &column.source).unwrap_or_default();
            fonts
                .face(column.font)
                .wrap(value, column.size, column.width, LINE_HEIGHT)
                .height
        })
        .fold(0.0, f64::max);
    tallest_cell + 2.0 * layout.table.row_padding
}

/// Plans the pages of an Invoice. Pure: no I/O.
///
/// Expects a Layout and Invoice Data that passed [`crate::validate`]. Returns
/// one error per Line Item that is too tall for any page.
pub fn paginate(
    layout: &Layout,
    invoice_data: &InvoiceData,
    fonts: &Fonts,
) -> Result<PagePlan, Vec<ValidationError>> {
    let geometry = Geometry::new(layout);
    let heights: Vec<f64> = (0..invoice_data.line_items.len())
        .map(|index| line_item_height(layout, invoice_data, fonts, index))
        .collect();

    let continuation_area =
        geometry.table_bottom - geometry.line_items_top(PageHeader::Continuation);
    let errors: Vec<ValidationError> = heights
        .iter()
        .enumerate()
        .filter(|&(_, &height)| height > continuation_area + FIT_TOLERANCE_MM)
        .map(|(index, &height)| {
            ValidationError::new(
                join(&join(INVOICE_DATA_PATH, "lineItems"), index),
                format!(
                    "Line Item is {height:.2} mm tall, but a continuation page has room for \
                     only {continuation_area:.2} mm of Line Items; it cannot be placed on any page"
                ),
            )
        })
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut pages = vec![geometry.table_page(PageHeader::FirstPage)];
    let mut y = geometry.line_items_top(PageHeader::FirstPage);
    for (index, &height) in heights.iter().enumerate() {
        if !geometry.fits(y, height) {
            pages.push(geometry.table_page(PageHeader::Continuation));
            y = geometry.line_items_top(PageHeader::Continuation);
        }
        let page = pages.last_mut().expect("at least one page");
        page.line_items.push(PlacedLineItem { index, y, height });
        y += height;
    }

    if !geometry.fits(y, layout.totals_block.height) {
        pages.push(Page {
            header: PageHeader::Continuation,
            table_header_y: None,
            line_items: Vec::new(),
            totals_block_y: None,
        });
        y = geometry.header_bottom(PageHeader::Continuation);
    }
    pages.last_mut().expect("at least one page").totals_block_y = Some(y);

    Ok(PagePlan { pages })
}

/// The vertical positions every page is built from.
struct Geometry {
    top_margin: f64,
    first_page_header: f64,
    continuation_header: f64,
    table_header: f64,
    /// Top of the Footer: nothing else may extend below it.
    table_bottom: f64,
}

impl Geometry {
    fn new(layout: &Layout) -> Self {
        Self {
            top_margin: layout.margins.top,
            first_page_header: layout.first_page_header.height,
            continuation_header: layout.continuation_header.height,
            table_header: layout.table.header.height,
            table_bottom: PAGE_HEIGHT_MM - layout.margins.bottom - layout.footer.height,
        }
    }

    /// Bottom of the page's header zone: the Table Header, or on a page
    /// holding only the Totals Block, the Totals Block, starts here.
    fn header_bottom(&self, header: PageHeader) -> f64 {
        self.top_margin
            + match header {
                PageHeader::FirstPage => self.first_page_header,
                PageHeader::Continuation => self.continuation_header,
            }
    }

    /// Top of the first Line Item on a page, below the Table Header.
    fn line_items_top(&self, header: PageHeader) -> f64 {
        self.header_bottom(header) + self.table_header
    }

    /// An empty page with a Table Header.
    fn table_page(&self, header: PageHeader) -> Page {
        Page {
            header,
            table_header_y: Some(self.header_bottom(header)),
            line_items: Vec::new(),
            totals_block_y: None,
        }
    }

    /// Whether something `height` mm tall starting at `y` ends above the Footer.
    fn fits(&self, y: f64, height: f64) -> bool {
        y + height <= self.table_bottom + FIT_TOLERANCE_MM
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::layout::Font;
    use crate::test_support::{invoice_data, layout, set};

    // The test Layout: margins top 10 / bottom 10, First-Page Header 80,
    // Continuation Header 20, Table Header 8, Totals Block 20, Footer 15,
    // columns at 9 pt, rowPadding 1.
    const FIRST_TABLE_HEADER_Y: f64 = 90.0;
    const FIRST_LINE_ITEM_Y: f64 = 98.0;
    const CONTINUATION_TABLE_HEADER_Y: f64 = 30.0;
    const CONTINUATION_LINE_ITEM_Y: f64 = 38.0;
    const FOOTER_HEIGHT: f64 = 15.0;
    const TOTALS_BLOCK_HEIGHT: f64 = 20.0;
    /// Top of the Footer with the test Layout.
    const TABLE_BOTTOM: f64 = PAGE_HEIGHT_MM - 10.0 - FOOTER_HEIGHT;

    const EPSILON: f64 = 1e-9;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    /// Height of one line of 9 pt text.
    fn line() -> f64 {
        Fonts::embedded()
            .face(Font::Regular)
            .line_height(9.0, LINE_HEIGHT)
    }

    /// Height of a Line Item whose tallest cell has `lines` lines.
    fn line_item(lines: usize) -> f64 {
        lines as f64 * line() + 2.0
    }

    /// A Line Item whose description has `lines` short lines.
    fn line_item_with_lines(lines: usize) -> Value {
        let description = vec!["Beratung"; lines].join("\n");
        json!({ "description": description, "amount": "1,00 €" })
    }

    fn line_items(count: usize) -> Value {
        Value::Array((0..count).map(|_| line_item_with_lines(1)).collect())
    }

    /// Loads (and so validates) the inputs, then paginates.
    fn plan(layout: &Value, invoice_data: &Value) -> Result<PagePlan, Vec<ValidationError>> {
        let (layout, invoice_data) =
            crate::load_values(layout, invoice_data).expect("valid test inputs");
        paginate(&layout, &invoice_data, &Fonts::embedded())
    }

    fn plan_ok(layout: &Value, invoice_data: &Value) -> PagePlan {
        plan(layout, invoice_data).expect("paginates")
    }

    fn with_line_items(items: Value) -> Value {
        let mut value = invoice_data();
        set(&mut value, "/lineItems", items);
        value
    }

    /// The test Layout with the Footer raised or lowered so that page one's
    /// table area ends exactly `extra` mm below `count` one-line Line Items.
    fn layout_fitting(count: usize, extra: f64) -> Value {
        let table_bottom = FIRST_LINE_ITEM_Y + count as f64 * line_item(1) + extra;
        let mut value = layout();
        set(
            &mut value,
            "/footer/height",
            json!(PAGE_HEIGHT_MM - 10.0 - table_bottom),
        );
        value
    }

    // --- Measuring --------------------------------------------------------

    #[test]
    fn line_item_height_is_one_line_plus_padding() {
        let (layout, invoice_data) = crate::load_values(&layout(), &invoice_data()).expect("valid");
        let height = line_item_height(&layout, &invoice_data, &Fonts::embedded(), 0);
        assert_close(height, line() + 2.0);
    }

    #[test]
    fn line_item_height_is_the_tallest_cell() {
        let items = json!([
            { "description": "eins\nzwei\ndrei", "amount": "1,00 €" },
            { "description": "eins", "amount": "1,00 €\n2,00 €\n3,00 €\n4,00 €" }
        ]);
        let (layout, invoice_data) =
            crate::load_values(&layout(), &with_line_items(items)).expect("valid");
        let fonts = Fonts::embedded();
        assert_close(
            line_item_height(&layout, &invoice_data, &fonts, 0),
            line_item(3),
        );
        assert_close(
            line_item_height(&layout, &invoice_data, &fonts, 1),
            line_item(4),
        );
    }

    #[test]
    fn line_item_height_counts_wrapped_lines() {
        // The description column is 100 mm wide; this wraps to several lines.
        let description = "Wartungsvertrag Klimaanlage, Quartalspauschale gemäß Vertrag \
                           Nr. 2026-117, inklusive Anfahrt, Prüfprotokoll und Prüfplakette";
        let items = json!([{ "description": description, "amount": "1,00 €" }]);
        let (layout, invoice_data) =
            crate::load_values(&layout(), &with_line_items(items)).expect("valid");
        let fonts = Fonts::embedded();
        let wrapped = fonts
            .face(Font::Regular)
            .wrap(description, 9.0, 100.0, LINE_HEIGHT);
        assert!(wrapped.lines.len() > 1, "{:?}", wrapped.lines);
        assert_close(
            line_item_height(&layout, &invoice_data, &fonts, 0),
            line_item(wrapped.lines.len()),
        );
    }

    #[test]
    fn empty_cells_leave_only_the_padding() {
        let items = json!([{ "description": "", "amount": "" }]);
        let (layout, invoice_data) =
            crate::load_values(&layout(), &with_line_items(items)).expect("valid");
        assert_close(
            line_item_height(&layout, &invoice_data, &Fonts::embedded(), 0),
            2.0,
        );
    }

    // --- Placing ----------------------------------------------------------

    #[test]
    fn places_a_short_invoice_on_one_page() {
        let plan = plan_ok(&layout(), &invoice_data());
        assert_eq!(plan.page_count(), 1);
        let page = &plan.pages[0];
        assert_eq!(page.header, PageHeader::FirstPage);
        assert_eq!(page.table_header_y, Some(FIRST_TABLE_HEADER_Y));
        assert_eq!(page.line_item_range(), 0..2);
        assert_close(page.line_items[0].y, FIRST_LINE_ITEM_Y);
        assert_close(page.line_items[0].height, line_item(1));
        assert_close(page.line_items[1].y, FIRST_LINE_ITEM_Y + line_item(1));
        assert_close(
            page.totals_block_y.expect("Totals Block on page one"),
            FIRST_LINE_ITEM_Y + 2.0 * line_item(1),
        );
    }

    #[test]
    fn zero_line_items_keep_the_table_header_and_totals_block_follows_it() {
        let plan = plan_ok(&layout(), &with_line_items(json!([])));
        assert_eq!(plan.page_count(), 1);
        let page = &plan.pages[0];
        assert_eq!(page.header, PageHeader::FirstPage);
        assert_eq!(page.table_header_y, Some(FIRST_TABLE_HEADER_Y));
        assert!(page.line_items.is_empty());
        assert_eq!(page.line_item_range(), 0..0);
        assert_close(
            page.totals_block_y.expect("Totals Block"),
            FIRST_LINE_ITEM_Y,
        );
    }

    #[test]
    fn zero_line_items_move_a_totals_block_that_does_not_fit_page_one() {
        let mut value = layout();
        // Leaves 10 mm below the Table Header on page one.
        set(&mut value, "/firstPageHeader/height", json!(244));
        let plan = plan_ok(&value, &with_line_items(json!([])));
        assert_eq!(plan.page_count(), 2);
        assert_eq!(plan.pages[0].table_header_y, Some(254.0));
        assert_eq!(plan.pages[0].totals_block_y, None);
        let last = &plan.pages[1];
        assert_eq!(last.header, PageHeader::Continuation);
        assert_eq!(last.table_header_y, None);
        assert_close(last.totals_block_y.expect("Totals Block"), 30.0);
    }

    #[test]
    fn line_item_that_exactly_fits_stays_on_the_page() {
        let plan = plan_ok(&layout_fitting(30, 0.0), &with_line_items(line_items(30)));
        let first = &plan.pages[0];
        assert_eq!(first.line_item_range(), 0..30);
        let last = first.line_items.last().expect("Line Items");
        assert_close(
            last.y + last.height,
            FIRST_LINE_ITEM_Y + 30.0 * line_item(1),
        );
        // No room is left for the Totals Block, so it gets a page of its own.
        assert_eq!(first.totals_block_y, None);
        assert_eq!(plan.page_count(), 2);
    }

    #[test]
    fn line_item_that_misses_by_a_hair_moves_to_the_next_page() {
        let plan = plan_ok(
            &layout_fitting(30, -0.001),
            &with_line_items(line_items(30)),
        );
        assert_eq!(plan.page_count(), 2);
        assert_eq!(plan.pages[0].line_item_range(), 0..29);
        let next = &plan.pages[1];
        assert_eq!(next.header, PageHeader::Continuation);
        assert_eq!(next.table_header_y, Some(CONTINUATION_TABLE_HEADER_Y));
        assert_eq!(next.line_item_range(), 29..30);
        assert_close(next.line_items[0].y, CONTINUATION_LINE_ITEM_Y);
        assert_close(
            next.totals_block_y.expect("Totals Block"),
            CONTINUATION_LINE_ITEM_Y + line_item(1),
        );
    }

    #[test]
    fn continuation_pages_repeat_the_table_header() {
        let plan = plan_ok(&layout(), &with_line_items(line_items(100)));
        // Page one holds floor(174 / h) Line Items, later pages floor(234 / h).
        let first = ((TABLE_BOTTOM - FIRST_LINE_ITEM_Y) / line_item(1)).floor() as usize;
        let later = ((TABLE_BOTTOM - CONTINUATION_LINE_ITEM_Y) / line_item(1)).floor() as usize;
        assert_eq!(plan.pages[0].line_item_range(), 0..first);
        assert_eq!(plan.pages[1].line_item_range(), first..first + later);
        for page in &plan.pages[1..] {
            assert_eq!(page.header, PageHeader::Continuation);
            assert_eq!(page.table_header_y, Some(CONTINUATION_TABLE_HEADER_Y));
            assert_close(page.line_items[0].y, CONTINUATION_LINE_ITEM_Y);
        }
        assert_eq!(plan.pages.last().unwrap().line_item_range().end, 100);
        assert_eq!(plan.page_count(), 1 + (100 - first).div_ceil(later));
    }

    #[test]
    fn totals_block_that_exactly_fits_stays_after_the_last_line_item() {
        let value = layout_fitting(10, TOTALS_BLOCK_HEIGHT);
        let plan = plan_ok(&value, &with_line_items(line_items(10)));
        assert_eq!(plan.page_count(), 1);
        assert_close(
            plan.pages[0].totals_block_y.expect("Totals Block"),
            FIRST_LINE_ITEM_Y + 10.0 * line_item(1),
        );
    }

    #[test]
    fn totals_block_that_overflows_gets_a_page_of_its_own() {
        let value = layout_fitting(10, TOTALS_BLOCK_HEIGHT - 0.001);
        let plan = plan_ok(&value, &with_line_items(line_items(10)));
        assert_eq!(plan.page_count(), 2);
        assert_eq!(plan.pages[0].line_item_range(), 0..10);
        assert_eq!(plan.pages[0].totals_block_y, None);
        let last = &plan.pages[1];
        assert_eq!(last.header, PageHeader::Continuation);
        assert_eq!(
            last.table_header_y, None,
            "no Table Header without Line Items"
        );
        assert!(last.line_items.is_empty());
        assert_close(last.totals_block_y.expect("Totals Block"), 30.0);
    }

    #[test]
    fn line_item_too_tall_for_page_one_starts_on_page_two() {
        // Page one has 174 mm for Line Items, continuation pages 234 mm.
        let lines = ((200.0 - 2.0) / line()).floor() as usize;
        let items = json!([line_item_with_lines(lines)]);
        let plan = plan_ok(&layout(), &with_line_items(items));
        assert_eq!(plan.pages[0].table_header_y, Some(FIRST_TABLE_HEADER_Y));
        assert!(plan.pages[0].line_items.is_empty());
        assert_eq!(plan.pages[1].line_item_range(), 0..1);
        assert_close(plan.pages[1].line_items[0].y, CONTINUATION_LINE_ITEM_Y);
    }

    #[test]
    fn oversized_line_item_is_an_error_at_its_path() {
        // Taller than the 234 mm of a continuation page.
        let lines = ((240.0 - 2.0) / line()).ceil() as usize;
        let items = json!([
            line_item_with_lines(1),
            line_item_with_lines(lines),
            line_item_with_lines(1),
            line_item_with_lines(lines)
        ]);
        let errors = plan(&layout(), &with_line_items(items)).expect_err("too tall");
        let paths: Vec<_> = errors.iter().map(|error| error.path.as_str()).collect();
        assert_eq!(
            paths,
            ["/invoiceData/lineItems/1", "/invoiceData/lineItems/3"]
        );
        assert!(
            errors[0].message.contains("234"),
            "names the available height: {}",
            errors[0].message
        );
    }

    #[test]
    fn line_item_as_tall_as_a_continuation_page_is_accepted() {
        let lines = ((234.0 - 2.0) / line()).floor() as usize;
        let mut value = layout();
        // Make the Line Item fill the continuation page's table area exactly.
        let height = line_item(lines);
        set(
            &mut value,
            "/footer/height",
            json!(PAGE_HEIGHT_MM - 10.0 - CONTINUATION_LINE_ITEM_Y - height),
        );
        let items = json!([line_item_with_lines(lines)]);
        let plan = plan_ok(&value, &with_line_items(items));
        assert_eq!(plan.pages[1].line_item_range(), 0..1);
    }
}
