//! Fonts: the embedded Liberation Sans faces, text measurement, line
//! wrapping and per-Invoice subsetting (ADR 0001, ADR 0002).
//!
//! [`Fonts`] maps the Layout's [`Font`] names to [`FontFace`]s. Every width
//! and height is measured from the font file that is embedded in the Invoice,
//! so what is measured is what is drawn. Lengths are millimetres, font sizes
//! points, like everywhere in the Layout.
//!
//! Measurement uses the glyphs' advance widths only; kerning is not applied,
//! so the renderer can draw a line as one plain string of glyphs and get
//! exactly the measured width.

mod subset;
mod wrap;

use std::fmt;

use ttf_parser::{Face, GlyphId, name_id};

use crate::layout::Font;

pub use subset::{FontMetrics, FontSubset, UsedGlyphs};
pub use wrap::WrappedText;

/// Millimetres per PostScript point (1 pt = 1/72 in).
pub const MM_PER_PT: f64 = 25.4 / 72.0;

const LIBERATION_SANS_REGULAR: &[u8] = include_bytes!("../../fonts/LiberationSans-Regular.ttf");
const LIBERATION_SANS_BOLD: &[u8] = include_bytes!("../../fonts/LiberationSans-Bold.ttf");

/// The font faces available to a Layout, one per [`Font`] name.
///
/// Today these are always the embedded Liberation Sans faces. Tenant fonts
/// can later be supported by building a `Fonts` from other font programs;
/// the Layout keeps referring to faces by name.
#[derive(Debug, Clone)]
pub struct Fonts {
    regular: FontFace,
    bold: FontFace,
}

impl Fonts {
    /// The embedded default fonts: Liberation Sans Regular and Bold.
    pub fn embedded() -> Self {
        let face = |data| FontFace::from_static(data).expect("embedded font is valid");
        Self {
            regular: face(LIBERATION_SANS_REGULAR),
            bold: face(LIBERATION_SANS_BOLD),
        }
    }

    /// The face the Layout means by `font`.
    pub fn face(&self, font: Font) -> &FontFace {
        match font {
            Font::Regular => &self.regular,
            Font::Bold => &self.bold,
        }
    }
}

/// One font program (a TrueType face) that text is measured with and that is
/// embedded, as a subset, in the Invoice.
#[derive(Clone)]
pub struct FontFace {
    data: &'static [u8],
    face: Face<'static>,
}

impl fmt::Debug for FontFace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontFace")
            .field("postscript_name", &self.postscript_name())
            .finish_non_exhaustive()
    }
}

/// A font program could not be read or subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontError {
    pub message: String,
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "font error: {}", self.message)
    }
}

impl std::error::Error for FontError {}

impl FontFace {
    /// Reads a TrueType font program.
    pub fn from_static(data: &'static [u8]) -> Result<Self, FontError> {
        let face = Face::parse(data, 0).map_err(|error| FontError {
            message: error.to_string(),
        })?;
        if face.tables().glyf.is_none() {
            return Err(FontError {
                message: "only TrueType outlines (glyf) are supported".to_owned(),
            });
        }
        Ok(Self { data, face })
    }

    /// The font program as embedded before subsetting.
    pub fn data(&self) -> &'static [u8] {
        self.data
    }

    /// The PostScript name from the font's `name` table, e.g. `LiberationSans-Bold`.
    pub fn postscript_name(&self) -> String {
        self.face
            .names()
            .into_iter()
            .filter(|name| name.name_id == name_id::POST_SCRIPT_NAME)
            .find_map(|name| name.to_string())
            .unwrap_or_default()
    }

    /// Font design units per em.
    pub fn units_per_em(&self) -> u16 {
        self.face.units_per_em()
    }

    /// The glyph drawn for `c`; characters the font lacks map to `.notdef` (0).
    pub fn glyph_id(&self, c: char) -> GlyphId {
        self.face.glyph_index(c).unwrap_or(GlyphId(0))
    }

    /// Advance width of glyph `glyph` in font design units.
    fn glyph_advance(&self, glyph: GlyphId) -> u16 {
        self.face.glyph_hor_advance(glyph).unwrap_or(0)
    }

    /// Converts font design units at `size` pt to mm.
    fn units_to_mm(&self, units: f64, size: f64) -> f64 {
        units / f64::from(self.units_per_em()) * size * MM_PER_PT
    }

    /// Advance width of `c` in font design units.
    pub fn advance(&self, c: char) -> u16 {
        self.glyph_advance(self.glyph_id(c))
    }

    /// Width of `text` set at `size` pt, in mm.
    pub fn text_width(&self, text: &str, size: f64) -> f64 {
        let units: u64 = text.chars().map(|c| u64::from(self.advance(c))).sum();
        self.units_to_mm(units as f64, size)
    }

    /// Height of one line at `size` pt and line-height factor `line_height`
    /// (e.g. 1.2 = 120 % of the font size), in mm.
    pub fn line_height(&self, size: f64, line_height: f64) -> f64 {
        size * line_height * MM_PER_PT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sum of Helvetica AFM widths (1000 units per em); Liberation Sans is
    /// metric-compatible, its 2048-unit widths round to these.
    fn afm_width_mm(widths: &[u16], size: f64) -> f64 {
        widths.iter().map(|&w| f64::from(w)).sum::<f64>() / 1000.0 * size * MM_PER_PT
    }

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected} ± {tolerance}, got {actual}"
        );
    }

    #[test]
    fn faces_are_keyed_by_layout_font_name() {
        let fonts = Fonts::embedded();
        assert_eq!(
            fonts.face(Font::Regular).postscript_name(),
            "LiberationSans"
        );
        assert_eq!(
            fonts.face(Font::Bold).postscript_name(),
            "LiberationSans-Bold"
        );
        assert_eq!(fonts.face(Font::Regular).units_per_em(), 2048);
    }

    #[test]
    fn measures_ascii_against_helvetica_metrics() {
        let fonts = Fonts::embedded();
        // H e l l o space W o r l d
        let regular = [722, 556, 222, 222, 556, 278, 944, 556, 333, 222, 556];
        assert_close(
            fonts.face(Font::Regular).text_width("Hello World", 10.0),
            afm_width_mm(&regular, 10.0),
            0.01,
        );
        let bold = [722, 556, 278, 278, 611, 278, 944, 611, 389, 278, 611];
        assert_close(
            fonts.face(Font::Bold).text_width("Hello World", 10.0),
            afm_width_mm(&bold, 10.0),
            0.01,
        );
    }

    #[test]
    fn measures_umlauts_sharp_s_and_euro() {
        let fonts = Fonts::embedded();
        // G r ö ß e
        assert_close(
            fonts.face(Font::Regular).text_width("Größe", 10.0),
            afm_width_mm(&[778, 333, 556, 611, 556], 10.0),
            0.01,
        );
        // € space 1 . 2 3 4 , 5 6
        let euro = [556, 278, 556, 278, 556, 556, 556, 278, 556, 556];
        assert_close(
            fonts.face(Font::Regular).text_width("€ 1.234,56", 9.0),
            afm_width_mm(&euro, 9.0),
            0.01,
        );
        // Ü b e r s i c h t   Ä Ö
        let bold = [722, 611, 556, 389, 556, 278, 556, 611, 333, 278, 722, 778];
        assert_close(
            fonts.face(Font::Bold).text_width("Übersicht ÄÖ", 12.0),
            afm_width_mm(&bold, 12.0),
            0.01,
        );
    }

    #[test]
    fn widths_match_the_font_file_exactly() {
        // Independent reading of the hmtx table through ttf-parser.
        let face = Face::parse(LIBERATION_SANS_REGULAR, 0).expect("font");
        let units: u32 = "äöüÄÖÜß€"
            .chars()
            .map(|c| {
                u32::from(
                    face.glyph_hor_advance(face.glyph_index(c).unwrap())
                        .unwrap(),
                )
            })
            .sum();
        let expected = f64::from(units) / 2048.0 * 11.0 * MM_PER_PT;
        let fonts = Fonts::embedded();
        assert_close(
            fonts.face(Font::Regular).text_width("äöüÄÖÜß€", 11.0),
            expected,
            1e-9,
        );
    }

    #[test]
    fn width_scales_with_size_and_empty_text_is_zero() {
        let face = Fonts::embedded().face(Font::Regular).clone();
        assert_eq!(face.text_width("", 10.0), 0.0);
        assert_close(
            face.text_width("Rechnung", 20.0),
            2.0 * face.text_width("Rechnung", 10.0),
            1e-9,
        );
    }

    #[test]
    fn missing_characters_use_notdef() {
        let face = Fonts::embedded().face(Font::Regular).clone();
        let missing = '\u{1F600}';
        assert_eq!(face.glyph_id(missing), GlyphId(0));
        assert_eq!(face.advance(missing), face.advance('\u{FFFF}'));
        assert!(face.advance(missing) > 0);
    }

    #[test]
    fn line_height_is_size_times_factor_in_mm() {
        let face = Fonts::embedded().face(Font::Regular).clone();
        assert_close(face.line_height(10.0, 1.2), 12.0 * MM_PER_PT, 1e-12);
    }

    #[test]
    fn rejects_non_font_data() {
        assert!(FontFace::from_static(b"not a font").is_err());
    }
}
