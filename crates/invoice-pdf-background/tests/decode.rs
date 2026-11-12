//! Decoding Backgrounds into their ready-to-embed form.

mod support;

use invoice_pdf_background::{Background, BackgroundError, ColorSpace, Filter};
use support::{cmyk_jpeg, inflate, jpeg, png, rgb_jpeg, rgb_png, rgba_png};

const REFERENCE: &str = "gs://tenant-assets/acme";

fn reject(bytes: &[u8]) -> BackgroundError {
    let error = Background::decode(REFERENCE, bytes).expect_err("image should be rejected");
    assert_eq!(error.reference(), REFERENCE);
    assert!(error.to_string().contains(REFERENCE), "{error}");
    assert!(error.is_rejected_reference());
    error
}

#[test]
fn passes_rgb_jpeg_through_as_dct() {
    let bytes = rgb_jpeg();
    let background = Background::decode(REFERENCE, &bytes).expect("rgb jpeg");
    assert_eq!(
        background,
        Background {
            width: 640,
            height: 480,
            color_space: ColorSpace::DeviceRgb,
            bits_per_component: 8,
            filter: Filter::DctDecode,
            data: bytes,
            s_mask: None,
        }
    );
}

#[test]
fn passes_grayscale_jpeg_through_as_dct() {
    let background = Background::decode(REFERENCE, &jpeg(0xC0, 8, 10, 20, 1)).expect("gray");
    assert_eq!(background.color_space, ColorSpace::DeviceGray);
    assert_eq!((background.width, background.height), (10, 20));
}

#[test]
fn accepts_progressive_and_extended_jpeg() {
    for marker in [0xC1, 0xC2] {
        let background = Background::decode(REFERENCE, &jpeg(marker, 8, 3, 4, 3))
            .unwrap_or_else(|error| panic!("SOF {marker:#X}: {error}"));
        assert_eq!(background.filter, Filter::DctDecode);
    }
}

#[test]
fn rejects_cmyk_jpeg_naming_the_reference() {
    let error = reject(&cmyk_jpeg());
    assert!(
        matches!(&error, BackgroundError::UnsupportedColorSpace { color_space, .. } if color_space.contains("CMYK")),
        "{error:?}"
    );
    assert!(error.to_string().contains("CMYK"), "{error}");
}

#[test]
fn rejects_jpeg_with_two_components() {
    let error = reject(&jpeg(0xC0, 8, 3, 4, 2));
    assert!(
        matches!(error, BackgroundError::UnsupportedColorSpace { .. }),
        "{error:?}"
    );
}

#[test]
fn rejects_12_bit_and_arithmetic_coded_jpeg() {
    for bytes in [jpeg(0xC1, 12, 3, 4, 3), jpeg(0xC9, 8, 3, 4, 3)] {
        let error = reject(&bytes);
        assert!(
            matches!(error, BackgroundError::UnsupportedFormat { .. }),
            "{error:?}"
        );
    }
}

#[test]
fn rejects_truncated_jpeg() {
    let bytes = rgb_jpeg();
    let error = reject(&bytes[..24]);
    assert!(matches!(error, BackgroundError::Decode { .. }), "{error:?}");
}

#[test]
fn recompresses_rgb_png_as_flate_without_smask() {
    let background = Background::decode(REFERENCE, &rgb_png()).expect("rgb png");
    assert_eq!(background.width, 2);
    assert_eq!(background.height, 2);
    assert_eq!(background.color_space, ColorSpace::DeviceRgb);
    assert_eq!(background.bits_per_component, 8);
    assert_eq!(background.filter, Filter::FlateDecode);
    assert_eq!(
        inflate(&background.data),
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
    );
    assert_eq!(background.s_mask, None);
}

#[test]
fn splits_png_alpha_into_smask() {
    let background = Background::decode(REFERENCE, &rgba_png()).expect("rgba png");
    assert_eq!(background.color_space, ColorSpace::DeviceRgb);
    assert_eq!(background.filter, Filter::FlateDecode);
    assert_eq!(inflate(&background.data), [255, 0, 0, 0, 0, 255]);
    let mask = background.s_mask.as_ref().expect("SMask for alpha");
    assert_eq!(
        (mask.width, mask.height, mask.bits_per_component),
        (2, 1, 8)
    );
    assert_eq!(inflate(&mask.data), [255, 128]);
    assert_eq!(
        background.byte_size(),
        background.data.len() + mask.data.len()
    );
}

#[test]
fn drops_alpha_of_fully_opaque_png() {
    let bytes = png(1, 1, png::ColorType::Rgba, &[9, 8, 7, 255]);
    let background = Background::decode(REFERENCE, &bytes).expect("opaque rgba");
    assert_eq!(inflate(&background.data), [9, 8, 7]);
    assert_eq!(background.s_mask, None);
}

#[test]
fn decodes_grayscale_png_with_alpha() {
    let bytes = png(2, 1, png::ColorType::GrayscaleAlpha, &[10, 0, 20, 255]);
    let background = Background::decode(REFERENCE, &bytes).expect("gray alpha");
    assert_eq!(background.color_space, ColorSpace::DeviceGray);
    assert_eq!(inflate(&background.data), [10, 20]);
    assert_eq!(inflate(&background.s_mask.expect("SMask").data), [0, 255]);
}

#[test]
fn expands_palette_png_to_rgb() {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, 2, 1);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(vec![1, 2, 3, 4, 5, 6]);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[1, 0]).unwrap();
    }
    let background = Background::decode(REFERENCE, &out).expect("palette png");
    assert_eq!(background.color_space, ColorSpace::DeviceRgb);
    assert_eq!(inflate(&background.data), [4, 5, 6, 1, 2, 3]);
}

#[test]
fn reduces_16_bit_png_to_8_bit() {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, 1, 1);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Sixteen);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0xAB, 0xCD]).unwrap();
    }
    let background = Background::decode(REFERENCE, &out).expect("16-bit png");
    assert_eq!(background.bits_per_component, 8);
    assert_eq!(inflate(&background.data), [0xAB]);
}

#[test]
fn rejects_corrupt_png() {
    let bytes = rgb_png();
    let error = reject(&bytes[..bytes.len() - 20]);
    assert!(matches!(error, BackgroundError::Decode { .. }), "{error:?}");
}

#[test]
fn rejects_other_formats() {
    for bytes in [&b"GIF89a......"[..], b"", b"%PDF-1.7"] {
        let error = reject(bytes);
        assert!(
            matches!(error, BackgroundError::UnsupportedFormat { .. }),
            "{error:?}"
        );
    }
}
