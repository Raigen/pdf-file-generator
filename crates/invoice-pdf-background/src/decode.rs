//! Decoding a Background image into the form it is embedded in the PDF.
//!
//! JPEGs are not decoded at all: their header is read for size, components
//! and bit depth, and the file is embedded as-is with `DCTDecode`. PNGs are
//! decoded to 8-bit samples (palettes expanded, 16-bit reduced), the alpha
//! channel is split off into an SMask unless it is fully opaque, and both are
//! recompressed with zlib for `FlateDecode`. Embedded ICC profiles are ignored:
//! Backgrounds are taken to be sRGB, matching the Invoice output intent.

use std::io::{Cursor, Write};

use flate2::Compression;
use flate2::write::ZlibEncoder;

use crate::error::BackgroundError;

/// Largest PNG accepted, in pixels (about A4 at 600 dpi). Larger images are
/// rejected rather than decoded into hundreds of megabytes.
const MAX_PNG_PIXELS: u64 = 40_000_000;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_SOI: &[u8] = &[0xFF, 0xD8];

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
        if bytes.starts_with(PNG_SIGNATURE) {
            decode_png(reference, bytes)
        } else if bytes.starts_with(JPEG_SOI) {
            decode_jpeg(reference, bytes)
        } else {
            Err(BackgroundError::UnsupportedFormat {
                reference: reference.to_owned(),
                reason: "not a PNG or JPEG image".into(),
            })
        }
    }

    /// Bytes held by this Background (image data plus SMask data); what the
    /// cache bound counts.
    pub fn byte_size(&self) -> usize {
        self.data.len() + self.s_mask.as_ref().map_or(0, |mask| mask.data.len())
    }
}

fn unsupported_format(reference: &str, reason: impl Into<String>) -> BackgroundError {
    BackgroundError::UnsupportedFormat {
        reference: reference.to_owned(),
        reason: reason.into(),
    }
}

fn unsupported_color_space(reference: &str, color_space: impl Into<String>) -> BackgroundError {
    BackgroundError::UnsupportedColorSpace {
        reference: reference.to_owned(),
        color_space: color_space.into(),
    }
}

fn corrupt(reference: &str, message: impl Into<String>) -> BackgroundError {
    BackgroundError::Decode {
        reference: reference.to_owned(),
        message: message.into(),
    }
}

/// Reads the JPEG header up to the first start-of-frame (SOF) marker.
fn decode_jpeg(reference: &str, bytes: &[u8]) -> Result<Background, BackgroundError> {
    let truncated = || corrupt(reference, "JPEG ends before its frame header");
    let mut pos = JPEG_SOI.len();
    loop {
        // Markers are 0xFF followed by a code; 0xFF fill bytes may precede it.
        if *bytes.get(pos).ok_or_else(truncated)? != 0xFF {
            return Err(corrupt(reference, "invalid JPEG marker"));
        }
        while bytes.get(pos) == Some(&0xFF) {
            pos += 1;
        }
        let marker = *bytes.get(pos).ok_or_else(truncated)?;
        pos += 1;
        match marker {
            // Stand-alone markers without a length.
            0x01 | 0xD0..=0xD7 => continue,
            0xD9 | 0xDA => return Err(corrupt(reference, "JPEG has no frame header")),
            _ => {}
        }
        let length = bytes.get(pos..pos + 2).ok_or_else(truncated)?;
        let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
        if length < 2 {
            return Err(corrupt(reference, "invalid JPEG segment length"));
        }
        let segment = bytes.get(pos + 2..pos + length).ok_or_else(truncated)?;
        pos += length;
        match marker {
            // Baseline, extended sequential and progressive Huffman-coded
            // frames: what PDF's DCTDecode supports.
            0xC0..=0xC2 => return jpeg_frame(reference, bytes, segment),
            // Other SOF markers: lossless, hierarchical or arithmetic-coded.
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err(unsupported_format(
                    reference,
                    format!(
                        "JPEG coding process SOF{} is not supported (only baseline, extended and progressive Huffman)",
                        marker - 0xC0
                    ),
                ));
            }
            _ => {}
        }
    }
}

fn jpeg_frame(reference: &str, bytes: &[u8], frame: &[u8]) -> Result<Background, BackgroundError> {
    let [precision, h1, h0, w1, w0, components, ..] = *frame else {
        return Err(corrupt(reference, "JPEG frame header is too short"));
    };
    if precision != 8 {
        return Err(unsupported_format(
            reference,
            format!("{precision}-bit JPEG (only 8-bit is supported)"),
        ));
    }
    let height = u16::from_be_bytes([h1, h0]);
    let width = u16::from_be_bytes([w1, w0]);
    if width == 0 || height == 0 {
        return Err(unsupported_format(
            reference,
            "JPEG without width or height in its frame header",
        ));
    }
    let color_space = match components {
        1 => ColorSpace::DeviceGray,
        3 => ColorSpace::DeviceRgb,
        4 => {
            return Err(unsupported_color_space(
                reference,
                "CMYK (4-component JPEG)",
            ));
        }
        n => {
            return Err(unsupported_color_space(
                reference,
                format!("{n}-component JPEG"),
            ));
        }
    };
    Ok(Background {
        width: u32::from(width),
        height: u32::from(height),
        color_space,
        bits_per_component: 8,
        filter: Filter::DctDecode,
        data: bytes.to_vec(),
        s_mask: None,
    })
}

fn decode_png(reference: &str, bytes: &[u8]) -> Result<Background, BackgroundError> {
    let decode_error = |error: png::DecodingError| corrupt(reference, error.to_string());
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let info = decoder.read_header_info().map_err(decode_error)?;
    if u64::from(info.width) * u64::from(info.height) > MAX_PNG_PIXELS {
        return Err(unsupported_format(
            reference,
            format!(
                "PNG of {}x{} pixels is larger than {MAX_PNG_PIXELS} pixels",
                info.width, info.height
            ),
        ));
    }
    let mut reader = decoder.read_info().map_err(decode_error)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| corrupt(reference, "PNG is too large"))?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(decode_error)?;
    buffer.truncate(frame.buffer_size());
    let (width, height) = (frame.width, frame.height);

    let (color_space, has_alpha) = match frame.color_type {
        png::ColorType::Grayscale => (ColorSpace::DeviceGray, false),
        png::ColorType::GrayscaleAlpha => (ColorSpace::DeviceGray, true),
        png::ColorType::Rgb => (ColorSpace::DeviceRgb, false),
        png::ColorType::Rgba => (ColorSpace::DeviceRgb, true),
        // Palettes are expanded by `normalize_to_color8`.
        png::ColorType::Indexed => {
            return Err(unsupported_color_space(reference, "unexpanded PNG palette"));
        }
    };
    if frame.bit_depth != png::BitDepth::Eight {
        return Err(corrupt(reference, "PNG was not reduced to 8 bits"));
    }

    let (color, s_mask) = if has_alpha {
        let channels = usize::from(color_space.components()) + 1;
        let pixels = buffer.len() / channels;
        let mut color = Vec::with_capacity(pixels * (channels - 1));
        let mut alpha = Vec::with_capacity(pixels);
        for pixel in buffer.chunks_exact(channels) {
            let (samples, a) = pixel.split_at(channels - 1);
            color.extend_from_slice(samples);
            alpha.push(a[0]);
        }
        let s_mask = if alpha.iter().all(|&a| a == u8::MAX) {
            None
        } else {
            Some(SMask {
                width,
                height,
                bits_per_component: 8,
                data: zlib(&alpha),
            })
        };
        (color, s_mask)
    } else {
        (buffer, None)
    };

    Ok(Background {
        width,
        height,
        color_space,
        bits_per_component: 8,
        filter: Filter::FlateDecode,
        data: zlib(&color),
        s_mask,
    })
}

/// zlib-compresses `samples` (the format `FlateDecode` expects).
fn zlib(samples: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(samples)
        .expect("writing to a Vec cannot fail");
    encoder.finish().expect("writing to a Vec cannot fail")
}
