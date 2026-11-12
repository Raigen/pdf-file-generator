//! Line wrapping of text into a box of a given width.

use super::{FontFace, MM_PER_PT};

/// Text broken into lines that each fit the box width.
#[derive(Debug, Clone, PartialEq)]
pub struct WrappedText {
    /// The lines, top to bottom, without line breaks or trailing whitespace.
    pub lines: Vec<String>,
    /// Total height of all lines in mm: line count × size × line-height factor.
    pub height: f64,
}

impl FontFace {
    /// Wraps `text` set at `size` pt into a box `width` mm wide, with lines
    /// `line_height` × `size` apart.
    ///
    /// - Explicit line breaks (`\n`, `\r\n`) always start a new line; a single
    ///   trailing line break does not add an empty line (like [`str::lines`]).
    /// - Lines break at whitespace; the whitespace at a break is dropped,
    ///   whitespace inside a line (including runs) and at the start of a
    ///   paragraph is kept, trailing whitespace is removed.
    /// - A word wider than the box is broken between characters, filling each
    ///   line as far as it fits; every line holds at least one character, so
    ///   a box narrower than a character yields one character per line.
    /// - Empty text yields no lines and height 0.
    ///
    /// No-break spaces (U+00A0, U+2007, U+202F) are not break opportunities.
    pub fn wrap(&self, text: &str, size: f64, width: f64, line_height: f64) -> WrappedText {
        // Work in font design units, which add up exactly; only the box
        // width is converted.
        let max_units = width / (size * MM_PER_PT) * f64::from(self.units_per_em());
        let mut lines = Vec::new();
        for paragraph in text.lines() {
            self.wrap_paragraph(paragraph, max_units, &mut lines);
        }
        let height = lines.len() as f64 * self.line_height(size, line_height);
        WrappedText { lines, height }
    }

    fn wrap_paragraph(&self, paragraph: &str, max_units: f64, lines: &mut Vec<String>) {
        let fits = |units: u64| units as f64 <= max_units + FIT_TOLERANCE_UNITS;
        let first_line = lines.len();
        let mut line = Line::default();
        // Whitespace before the next word: kept if the word joins the line,
        // dropped if the line breaks there.
        let mut space = Line::default();
        let mut at_paragraph_start = true;

        for (is_space, run) in runs(paragraph) {
            if is_space {
                space = Line::measure(self, run);
                continue;
            }
            let word = Line::measure(self, run);
            if !line.is_empty() || at_paragraph_start {
                if fits(line.units + space.units + word.units) {
                    line.push_line(&space);
                    line.push_line(&word);
                    space = Line::default();
                    at_paragraph_start = false;
                    continue;
                }
                if !line.is_empty() {
                    lines.push(line.take());
                }
            }
            at_paragraph_start = false;
            space = Line::default();
            if fits(word.units) {
                line = word;
                continue;
            }
            // Overlong word: break between characters, at least one per line.
            for c in run.chars() {
                let units = u64::from(self.advance(c));
                if !line.is_empty() && !fits(line.units + units) {
                    lines.push(line.take());
                }
                line.text.push(c);
                line.units += units;
            }
        }
        if !line.is_empty() || lines.len() == first_line {
            lines.push(line.take());
        }
    }
}

/// Slack when comparing a width with the box width, in font design units,
/// so that text measured to exactly the box width still fits.
const FIT_TOLERANCE_UNITS: f64 = 1e-6;

/// Text with its width in font design units.
#[derive(Debug, Default)]
struct Line {
    text: String,
    units: u64,
}

impl Line {
    fn measure(face: &FontFace, text: &str) -> Self {
        Self {
            text: text.to_owned(),
            units: text.chars().map(|c| u64::from(face.advance(c))).sum(),
        }
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn push_line(&mut self, other: &Line) {
        self.text.push_str(&other.text);
        self.units += other.units;
    }

    fn take(&mut self) -> String {
        self.units = 0;
        std::mem::take(&mut self.text)
    }
}

/// Whether `c` separates words, i.e. is a break opportunity.
fn is_break_space(c: char) -> bool {
    c.is_whitespace() && !matches!(c, '\u{00A0}' | '\u{2007}' | '\u{202F}')
}

/// Splits `text` into maximal runs of break spaces and of other characters.
fn runs(text: &str) -> impl Iterator<Item = (bool, &str)> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let is_space = is_break_space(first);
        let end = rest
            .find(|c| is_break_space(c) != is_space)
            .unwrap_or(rest.len());
        let (run, tail) = rest.split_at(end);
        rest = tail;
        Some((is_space, run))
    })
}

#[cfg(test)]
mod tests {
    use super::super::Fonts;
    use super::*;
    use crate::layout::Font;

    const SIZE: f64 = 10.0;
    const LINE_HEIGHT: f64 = 1.2;

    fn face() -> FontFace {
        Fonts::embedded().face(Font::Regular).clone()
    }

    fn width_of(text: &str) -> f64 {
        face().text_width(text, SIZE)
    }

    fn lines(text: &str, width: f64) -> Vec<String> {
        face().wrap(text, SIZE, width, LINE_HEIGHT).lines
    }

    #[test]
    fn empty_text_has_no_lines_and_no_height() {
        let wrapped = face().wrap("", SIZE, 50.0, LINE_HEIGHT);
        assert!(wrapped.lines.is_empty());
        assert_eq!(wrapped.height, 0.0);
    }

    #[test]
    fn short_text_is_one_line() {
        let wrapped = face().wrap("Hallo Welt", SIZE, 50.0, LINE_HEIGHT);
        assert_eq!(wrapped.lines, ["Hallo Welt"]);
        assert!((wrapped.height - SIZE * LINE_HEIGHT * MM_PER_PT).abs() < 1e-12);
    }

    #[test]
    fn text_exactly_as_wide_as_the_box_fits() {
        assert_eq!(lines("Hallo Welt", width_of("Hallo Welt")), ["Hallo Welt"]);
    }

    #[test]
    fn breaks_at_whitespace() {
        let width = width_of("Hallo Welt") - 0.01;
        assert_eq!(lines("Hallo Welt", width), ["Hallo", "Welt"]);
    }

    #[test]
    fn fills_lines_greedily() {
        let width = width_of("eins zwei");
        assert_eq!(
            lines("eins zwei drei vier fünf", width),
            ["eins zwei", "drei vier", "fünf"]
        );
    }

    #[test]
    fn height_counts_all_lines() {
        let wrapped = face().wrap("eins\nzwei\ndrei", SIZE, 50.0, 1.5);
        assert_eq!(wrapped.lines.len(), 3);
        assert!((wrapped.height - 3.0 * SIZE * 1.5 * MM_PER_PT).abs() < 1e-12);
    }

    #[test]
    fn single_long_word_breaks_by_character() {
        let word = "Donaudampfschifffahrtsgesellschaft";
        let width = width_of("Donau");
        let wrapped = lines(word, width);
        assert!(wrapped.len() > 1, "{wrapped:?}");
        assert_eq!(wrapped.concat(), word);
        let mut rest = word;
        for line in &wrapped {
            assert!(width_of(line) <= width + 1e-9, "{line:?} is too wide");
            rest = &rest[line.len()..];
            // Greedy: the next character would not have fitted.
            if let Some(next) = rest.chars().next() {
                assert!(width_of(&format!("{line}{next}")) > width, "{line:?}");
            }
        }
    }

    #[test]
    fn long_word_after_short_word_starts_on_its_own_line() {
        let width = width_of("Donau");
        let wrapped = lines("Nr. Donaudampf", width);
        assert_eq!(wrapped[0], "Nr.");
        assert_eq!(wrapped[1..].concat(), "Donaudampf");
    }

    #[test]
    fn box_narrower_than_a_character_yields_one_character_per_line() {
        assert_eq!(lines("aßc", 0.1), ["a", "ß", "c"]);
        assert_eq!(lines("a b", 0.0), ["a", "b"]);
    }

    #[test]
    fn honours_explicit_newlines() {
        assert_eq!(lines("A\nB", 50.0), ["A", "B"]);
        assert_eq!(lines("A\n\nB", 50.0), ["A", "", "B"]);
        assert_eq!(lines("A\r\nB", 50.0), ["A", "B"]);
        assert_eq!(lines("\nA", 50.0), ["", "A"]);
    }

    #[test]
    fn single_trailing_newline_adds_no_line() {
        assert_eq!(lines("A\n", 50.0), ["A"]);
        assert_eq!(lines("A\n\n", 50.0), ["A", ""]);
        assert_eq!(lines("\n", 50.0), [""]);
    }

    #[test]
    fn newlines_and_wrapping_combine() {
        let width = width_of("Hauptstraße") + 0.01;
        assert_eq!(
            lines("Frau Groß\nHauptstraße 17", width),
            ["Frau Groß", "Hauptstraße", "17"]
        );
    }

    #[test]
    fn keeps_whitespace_runs_inside_a_line() {
        assert_eq!(lines("a   b", 50.0), ["a   b"]);
        assert_eq!(lines("  eingerückt", 50.0), ["  eingerückt"]);
    }

    #[test]
    fn drops_whitespace_runs_at_breaks_and_line_ends() {
        let width = width_of("aaa  ");
        assert_eq!(lines("aaa   bbb", width), ["aaa", "bbb"]);
        assert_eq!(lines("a b   ", 50.0), ["a b"]);
        assert_eq!(lines("a b  \nc", 50.0), ["a b", "c"]);
    }

    #[test]
    fn whitespace_only_text_is_one_empty_line() {
        assert_eq!(lines("   ", 50.0), [""]);
        assert_eq!(lines("\t", 50.0), [""]);
    }

    #[test]
    fn no_break_spaces_do_not_break() {
        let text = "1.046,90\u{00A0}€";
        let width = width_of(text) - 0.01;
        let wrapped = lines(text, width);
        assert_eq!(wrapped.concat(), text, "broken by character only");
        assert!(wrapped[0].len() > "1.046,90".len(), "{wrapped:?}");
    }

    #[test]
    fn tabs_are_break_opportunities() {
        let width = width_of("Menge") + 0.01;
        assert_eq!(lines("Menge\tEinheit", width), ["Menge", "Einheit"]);
    }

    #[test]
    fn every_line_fits_a_long_paragraph() {
        let text = "Lizenz „Planer Pro“, 12 Monate – Reisekosten Hamburg–München, \
                    Bahnticket 2. Klasse, Beratung Projektplanung 1.046,90 €";
        // Wider than every word, so only whitespace breaks are needed.
        let width = 45.0;
        let wrapped = lines(text, width);
        assert!(wrapped.len() > 2);
        for line in &wrapped {
            assert!(width_of(line) <= width + 1e-9, "{line:?}");
            assert_eq!(line.trim_end(), line);
        }
        let words: Vec<&str> = text.split_whitespace().collect();
        let rejoined: Vec<String> = wrapped
            .iter()
            .flat_map(|l| l.split_whitespace().map(str::to_owned))
            .collect();
        assert_eq!(rejoined, words);
    }
}
