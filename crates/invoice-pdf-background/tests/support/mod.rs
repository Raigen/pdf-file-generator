//! Shared test helpers: tiny generated images and a fake [`Fetch`].

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

use invoice_pdf_background::{Fetch, FetchError, Source};

/// Encodes an 8-bit PNG of the given colour type from raw samples.
pub fn png(width: u32, height: u32, color: png::ColorType, samples: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(samples).expect("png data");
    }
    out
}

/// A 2×1 RGBA PNG: one opaque red pixel, one half-transparent blue pixel.
pub fn rgba_png() -> Vec<u8> {
    png(
        2,
        1,
        png::ColorType::Rgba,
        &[255, 0, 0, 255, 0, 0, 255, 128],
    )
}

/// A 2×2 RGB PNG.
pub fn rgb_png() -> Vec<u8> {
    png(
        2,
        2,
        png::ColorType::Rgb,
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
    )
}

/// A JPEG made of SOI, an APP0 segment, a start-of-frame segment and EOI.
/// Only the header is meaningful, which is all the decoder reads.
pub fn jpeg(sof_marker: u8, precision: u8, width: u16, height: u16, components: u8) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    // APP0 "JFIF\0" 1.01, no thumbnail.
    out.extend_from_slice(&[
        0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 1, 1, 0, 0, 1, 0, 1, 0, 0,
    ]);
    let length = 8 + 3 * u16::from(components);
    out.extend_from_slice(&[0xFF, sof_marker]);
    out.extend_from_slice(&length.to_be_bytes());
    out.push(precision);
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.push(components);
    for id in 1..=components {
        out.extend_from_slice(&[id, 0x11, 0]);
    }
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}

/// A baseline 8-bit RGB (YCbCr) JPEG header of 640×480.
pub fn rgb_jpeg() -> Vec<u8> {
    jpeg(0xC0, 8, 640, 480, 3)
}

/// A baseline 8-bit CMYK JPEG header.
pub fn cmyk_jpeg() -> Vec<u8> {
    jpeg(0xC0, 8, 640, 480, 4)
}

/// Inflates zlib data (a `FlateDecode` stream).
pub fn inflate(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .expect("valid zlib data");
    out
}

type Response = Result<Vec<u8>, FetchError>;

/// A [`Fetch`] serving fixed bytes per source and counting calls. With a gate,
/// every fetch of the gated source blocks until [`FakeFetcher::open_gate`].
#[derive(Default)]
pub struct FakeFetcher {
    responses: Mutex<HashMap<Source, Vec<Response>>>,
    calls: AtomicUsize,
    fetched: Mutex<Vec<Source>>,
    gate: Option<(Source, Mutex<bool>, Condvar)>,
}

impl FakeFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn gated_on(source: Source) -> Self {
        Self {
            gate: Some((source, Mutex::new(false), Condvar::new())),
            ..Self::default()
        }
    }

    /// Serves `bytes` for `source` on every fetch.
    pub fn serve(self, source: Source, bytes: Vec<u8>) -> Self {
        self.respond(source, vec![Ok(bytes)])
    }

    /// Serves `responses` for `source` in order; the last one repeats.
    pub fn respond(self, source: Source, responses: Vec<Response>) -> Self {
        self.responses.lock().unwrap().insert(source, responses);
        self
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn fetched(&self) -> Vec<Source> {
        self.fetched.lock().unwrap().clone()
    }

    pub fn open_gate(&self) {
        let (_, open, condvar) = self.gate.as_ref().expect("gated fetcher");
        *open.lock().unwrap() = true;
        condvar.notify_all();
    }
}

impl Fetch for FakeFetcher {
    fn fetch(&self, source: &Source) -> Result<Vec<u8>, FetchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.fetched.lock().unwrap().push(source.clone());
        if let Some((_, open, condvar)) = self.gate.as_ref().filter(|gate| gate.0 == *source) {
            let mut open = open.lock().unwrap();
            while !*open {
                open = condvar.wait(open).unwrap();
            }
        }
        let mut responses = self.responses.lock().unwrap();
        match responses.get_mut(source) {
            Some(queue) if queue.len() > 1 => queue.remove(0),
            Some(queue) => queue[0].clone(),
            None => Err(FetchError::NotFound),
        }
    }
}
