//! The subset of a typical Invoice's glyphs stays small (issue #3: < 40 KB
//! per face).

use std::fs;
use std::path::PathBuf;

use invoice_pdf_core::Font;
use invoice_pdf_core::font::{Fonts, UsedGlyphs};
use serde_json::Value;

const MAX_SUBSET_BYTES: usize = 40 * 1024;

fn read_fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("fixture is JSON")
}

fn collect_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
        _ => {}
    }
}

/// All text of the small Invoice Data and the Layout, plus every digit,
/// common punctuation, German letters and the euro sign.
fn typical_invoice_text() -> Vec<String> {
    let mut texts = Vec::new();
    collect_strings(&read_fixture("invoice-data-small.json"), &mut texts);
    collect_strings(&read_fixture("layout.json"), &mut texts);
    texts.push("0123456789 .,;:-–—/()%&+*=#@!?\"'„“”‚‘’«»§ €".to_owned());
    texts.push("ÄÖÜäöüß".to_owned());
    texts
}

#[test]
fn subset_of_a_typical_invoice_is_below_40_kb_per_face() {
    let fonts = Fonts::embedded();
    let texts = typical_invoice_text();
    for font in [Font::Regular, Font::Bold] {
        let mut used = UsedGlyphs::new(fonts.face(font));
        for text in &texts {
            for line in text.lines() {
                used.add_text(line);
            }
        }
        let subset = used.subset().expect("subset");
        let size = subset.font_program.len();
        println!(
            "{:?}: {} glyphs, subset {size} bytes ({:.1} KB) of {} bytes",
            font,
            used.len(),
            size as f64 / 1024.0,
            fonts.face(font).data().len()
        );
        assert!(
            size < MAX_SUBSET_BYTES,
            "{font:?} subset is {size} bytes, limit {MAX_SUBSET_BYTES}"
        );
        assert!(used.len() > 60, "expected a realistic glyph set");
    }
}
