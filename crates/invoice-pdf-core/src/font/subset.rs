//! Subsetting: the glyphs one Invoice uses from a face, and everything the
//! renderer needs to embed them as a CID font (Type0 / CIDFontType2 with
//! Identity-H encoding and a ToUnicode CMap).

use std::collections::BTreeMap;

use subsetter::GlyphRemapper;
use ttf_parser::GlyphId;

use super::{FontError, FontFace};

/// Collects the glyphs one Invoice draws with one face and assigns each a
/// CID (its glyph id in the subset font program).
///
/// CIDs are assigned in order of first use; `.notdef` is always CID 0.
#[derive(Debug, Clone)]
pub struct UsedGlyphs<'a> {
    face: &'a FontFace,
    remapper: GlyphRemapper,
    to_unicode: BTreeMap<u16, String>,
}

/// A subset font program and the data for its PDF font dictionaries.
///
/// All metrics are in PDF glyph space: 1000 units per em.
#[derive(Debug, Clone, PartialEq)]
pub struct FontSubset {
    /// Subset tag and PostScript name, e.g. `KQXWZB+LiberationSans`, for
    /// `/BaseFont` and `/FontName`. The tag is derived from the glyph set, so
    /// the same glyphs always give the same name.
    pub base_font: String,
    /// The subset TrueType font program (for `/FontFile2`). Its glyph ids are
    /// the CIDs, so `/CIDToGIDMap /Identity` applies.
    pub font_program: Vec<u8>,
    /// Advance width of every CID, indexed by CID (for `/W`).
    pub widths: Vec<f64>,
    /// The text each CID stands for (for the ToUnicode CMap). `.notdef` and
    /// glyphs drawn for characters the font lacks have no entry.
    pub to_unicode: BTreeMap<u16, String>,
    /// Metrics for the FontDescriptor.
    pub metrics: FontMetrics,
}

/// Face-wide metrics for a PDF FontDescriptor, in PDF glyph space (1000
/// units per em) except for the angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    pub ascent: f64,
    pub descent: f64,
    pub cap_height: f64,
    /// Font bounding box: `[x_min, y_min, x_max, y_max]`.
    pub bbox: [f64; 4],
    /// Degrees counter-clockwise from vertical; 0 for upright faces.
    pub italic_angle: f64,
    pub is_fixed_pitch: bool,
    pub is_italic: bool,
    pub is_bold: bool,
}

impl<'a> UsedGlyphs<'a> {
    /// No glyphs used yet (only `.notdef`).
    pub fn new(face: &'a FontFace) -> Self {
        Self {
            face,
            remapper: GlyphRemapper::new(),
            to_unicode: BTreeMap::new(),
        }
    }

    /// The face the glyphs come from.
    pub fn face(&self) -> &'a FontFace {
        self.face
    }

    /// Records the glyphs of `text` (one line, as returned by wrapping) and
    /// returns its CIDs, to be drawn as two-byte codes (Identity-H).
    pub fn add_text(&mut self, text: &str) -> Vec<u16> {
        text.chars()
            .map(|c| {
                let glyph = self.face.glyph_id(c).0;
                let cid = self.remapper.remap(glyph);
                if glyph != 0 {
                    self.to_unicode.entry(cid).or_insert_with(|| c.to_string());
                }
                cid
            })
            .collect()
    }

    /// Number of glyphs in the subset, `.notdef` included.
    pub fn len(&self) -> usize {
        usize::from(self.remapper.num_gids())
    }

    /// Whether no glyph besides `.notdef` has been used.
    pub fn is_empty(&self) -> bool {
        self.len() <= 1
    }

    /// Builds the subset font program and its PDF data.
    pub fn subset(&self) -> Result<FontSubset, FontError> {
        let font_program =
            subsetter::subset(self.face.data(), 0, &self.remapper).map_err(|error| FontError {
                message: format!("cannot subset {}: {error}", self.face.postscript_name()),
            })?;
        let widths = self
            .remapper
            .remapped_gids()
            .map(|glyph| {
                self.face
                    .to_glyph_space(f64::from(self.face.glyph_advance(GlyphId(glyph))))
            })
            .collect();
        Ok(FontSubset {
            base_font: format!("{}+{}", self.subset_tag(), self.face.postscript_name()),
            font_program,
            widths,
            to_unicode: self.to_unicode.clone(),
            metrics: self.face.metrics(),
        })
    }

    /// Six uppercase letters derived from the glyph set (FNV-1a hash), as
    /// PDF requires for the names of subset fonts.
    fn subset_tag(&self) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for glyph in self.remapper.remapped_gids() {
            for byte in glyph.to_be_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        (0..6)
            .map(|_| {
                let letter = b'A' + (hash % 26) as u8;
                hash /= 26;
                char::from(letter)
            })
            .collect()
    }
}

impl FontFace {
    /// Face-wide metrics for the FontDescriptor.
    pub fn metrics(&self) -> FontMetrics {
        let face = &self.face;
        let scale = |units: i16| self.to_glyph_space(f64::from(units));
        let bbox = face.global_bounding_box();
        FontMetrics {
            ascent: scale(face.ascender()),
            descent: scale(face.descender()),
            cap_height: scale(face.capital_height().unwrap_or(face.ascender())),
            bbox: [
                scale(bbox.x_min),
                scale(bbox.y_min),
                scale(bbox.x_max),
                scale(bbox.y_max),
            ],
            italic_angle: f64::from(face.italic_angle()),
            is_fixed_pitch: face.is_monospaced(),
            is_italic: face.is_italic(),
            is_bold: face.is_bold(),
        }
    }

    /// Converts font design units to PDF glyph space (1000 units per em).
    fn to_glyph_space(&self, units: f64) -> f64 {
        units * 1000.0 / f64::from(self.units_per_em())
    }
}

#[cfg(test)]
mod tests {
    use ttf_parser::Face;

    use super::super::Fonts;
    use super::*;
    use crate::layout::Font;

    #[test]
    fn assigns_cids_in_order_of_first_use() {
        let fonts = Fonts::embedded();
        let mut used = UsedGlyphs::new(fonts.face(Font::Regular));
        assert!(used.is_empty());
        assert_eq!(used.add_text("abba"), [1, 2, 2, 1]);
        assert_eq!(used.add_text("€ ab"), [3, 4, 1, 2]);
        assert_eq!(used.len(), 5);
        assert!(!used.is_empty());
    }

    #[test]
    fn missing_characters_are_drawn_as_notdef() {
        let fonts = Fonts::embedded();
        let mut used = UsedGlyphs::new(fonts.face(Font::Regular));
        assert_eq!(used.add_text("a\u{1F600}"), [1, 0]);
        let subset = used.subset().expect("subset");
        assert!(!subset.to_unicode.contains_key(&0));
    }

    #[test]
    fn subset_program_contains_the_used_glyphs_with_their_widths() {
        let fonts = Fonts::embedded();
        let face = fonts.face(Font::Regular);
        let mut used = UsedGlyphs::new(face);
        let text = "Größe 1.234,56 €";
        let cids = used.add_text(text);
        let subset = used.subset().expect("subset");

        let parsed = Face::parse(&subset.font_program, 0).expect("subset parses");
        // Components of composite glyphs (ö = o + ¨) are appended after the CIDs.
        assert!(usize::from(parsed.number_of_glyphs()) >= used.len());
        assert_eq!(subset.widths.len(), used.len());
        for (c, cid) in text.chars().zip(cids) {
            let advance = parsed.glyph_hor_advance(GlyphId(cid)).expect("advance");
            assert_eq!(advance, face.advance(c), "{c:?}");
            let expected = f64::from(face.advance(c)) * 1000.0 / 2048.0;
            assert!((subset.widths[usize::from(cid)] - expected).abs() < 1e-9);
            assert_eq!(subset.to_unicode[&cid], c.to_string());
        }
        assert!(subset.font_program.len() < face.data().len() / 10);
    }

    #[test]
    fn base_font_has_a_stable_subset_tag() {
        let fonts = Fonts::embedded();
        let mut a = UsedGlyphs::new(fonts.face(Font::Bold));
        a.add_text("Rechnung");
        let mut b = UsedGlyphs::new(fonts.face(Font::Bold));
        b.add_text("Rechnung");
        let mut c = UsedGlyphs::new(fonts.face(Font::Bold));
        c.add_text("Betrag");
        let (a, b, c) = (
            a.subset().unwrap(),
            b.subset().unwrap(),
            c.subset().unwrap(),
        );

        let (tag, name) = a.base_font.split_once('+').expect("tag+name");
        assert_eq!(name, "LiberationSans-Bold");
        assert_eq!(tag.len(), 6);
        assert!(tag.chars().all(|c| c.is_ascii_uppercase()), "{tag}");
        assert_eq!(a.base_font, b.base_font);
        assert_ne!(a.base_font, c.base_font);
    }

    #[test]
    fn empty_subset_holds_only_notdef() {
        let fonts = Fonts::embedded();
        let subset = UsedGlyphs::new(fonts.face(Font::Regular))
            .subset()
            .expect("subset");
        assert_eq!(subset.widths.len(), 1);
        assert!(subset.to_unicode.is_empty());
    }

    #[test]
    fn metrics_are_read_from_the_font_in_glyph_space() {
        let fonts = Fonts::embedded();
        for font in [Font::Regular, Font::Bold] {
            let face = fonts.face(font);
            let raw = Face::parse(face.data(), 0).expect("font");
            let scale = |v: i16| f64::from(v) * 1000.0 / 2048.0;
            let m = face.metrics();
            assert_eq!(m.ascent, scale(raw.ascender()));
            assert_eq!(m.descent, scale(raw.descender()));
            assert_eq!(
                m.cap_height,
                scale(raw.capital_height().expect("cap height"))
            );
            let b = raw.global_bounding_box();
            assert_eq!(
                m.bbox,
                [
                    scale(b.x_min),
                    scale(b.y_min),
                    scale(b.x_max),
                    scale(b.y_max)
                ]
            );
            assert_eq!(m.italic_angle, 0.0);
            assert!(!m.is_fixed_pitch && !m.is_italic);
            assert_eq!(m.is_bold, font == Font::Bold);
            assert!(m.ascent > 0.0 && m.descent < 0.0 && m.cap_height > 0.0);
        }
    }
}
