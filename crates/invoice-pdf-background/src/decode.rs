//! Decoding a Background image into the form it is embedded in the PDF.

use crate::error::BackgroundError;

/// A decoded Background, ready to be written as a PDF image XObject.
///
/// `data` is the stream content, already encoded with `filter`; the
/// dictionary entries are `Width`, `Height`, `ColorSpace`,
/// `BitsPerComponent`, `Filter` and, if present, `SMask`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Background {
    pub width: u32,
    pub height: u32,
    pub color_space: ColorSpace,
    pub bits_per_component: u8,
    pub filter: Filter,
    pub data: Vec<u8>,
    /// Soft mask with the alpha channel of a PNG; `None` if fully opaque.
    pub s_mask: Option<SMask>,
}

/// The alpha channel of a Background as a separate image: `DeviceGray`,
/// 8 bits per component, same size as the Background, `FlateDecode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SMask {
    pub width: u32,
    pub height: u32,
    pub bits_per_component: u8,
    pub data: Vec<u8>,
}

/// PDF colour space of a Background (`/DeviceRGB` or `/DeviceGray`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    DeviceRgb,
    DeviceGray,
}

impl ColorSpace {
    /// Number of colour components per pixel.
    pub fn components(self) -> u8 {
        match self {
            Self::DeviceRgb => 3,
            Self::DeviceGray => 1,
        }
    }
}

/// PDF stream filter of the image data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// JPEG data, passed through unchanged.
    DctDecode,
    /// zlib-compressed samples.
    FlateDecode,
}

impl Background {
    /// Decodes a PNG or JPEG (detected by its signature). `reference` is only
    /// used in error messages.
    pub fn decode(reference: &str, bytes: &[u8]) -> Result<Self, BackgroundError> {
        let _ = (reference, bytes);
        unimplemented!("issue #5")
    }

    /// Bytes held by this Background (image data plus SMask data); what the
    /// cache bound counts.
    pub fn byte_size(&self) -> usize {
        self.data.len() + self.s_mask.as_ref().map_or(0, |mask| mask.data.len())
    }
}
