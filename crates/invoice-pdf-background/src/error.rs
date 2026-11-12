//! Errors of Background loading. Every error names the Background reference.

use std::fmt;

/// Why a Background could not be loaded.
///
/// All variants except [`BackgroundError::Fetch`] are problems with the
/// reference or the image itself and map to HTTP 422; see
/// [`BackgroundError::is_rejected_reference`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundError {
    /// The reference uses none of the schemes `gs://`, `file://`, `https://`.
    UnsupportedScheme { reference: String },
    /// The scheme is valid but disabled by the configuration (no base
    /// directory for `file://`, no allowed hosts for `https://`).
    SchemeNotConfigured {
        reference: String,
        scheme: &'static str,
    },
    /// The reference has a supported scheme but is malformed.
    InvalidReference { reference: String, reason: String },
    /// A `file://` reference leaves the base directory: `..`, an absolute
    /// path, or a symlink pointing outside.
    OutsideBaseDir { reference: String },
    /// The host of an `https://` reference is not on the allow-list.
    HostNotAllowed { reference: String, host: String },
    /// Nothing exists at the referenced location.
    NotFound { reference: String },
    /// Fetching failed for another reason (I/O, network, upstream error).
    Fetch { reference: String, message: String },
    /// The image is neither a PNG nor a baseline/progressive 8-bit JPEG.
    UnsupportedFormat { reference: String, reason: String },
    /// The image is not RGB or grayscale, e.g. CMYK (ADR 0002).
    UnsupportedColorSpace {
        reference: String,
        color_space: String,
    },
    /// The image is corrupt.
    Decode { reference: String, message: String },
}

impl BackgroundError {
    /// The Background reference this error is about.
    pub fn reference(&self) -> &str {
        match self {
            Self::UnsupportedScheme { reference }
            | Self::SchemeNotConfigured { reference, .. }
            | Self::InvalidReference { reference, .. }
            | Self::OutsideBaseDir { reference }
            | Self::HostNotAllowed { reference, .. }
            | Self::NotFound { reference }
            | Self::Fetch { reference, .. }
            | Self::UnsupportedFormat { reference, .. }
            | Self::UnsupportedColorSpace { reference, .. }
            | Self::Decode { reference, .. } => reference,
        }
    }

    /// Whether the reference or the image it names is unusable (HTTP 422),
    /// as opposed to a failure to fetch it ([`BackgroundError::Fetch`]).
    pub fn is_rejected_reference(&self) -> bool {
        !matches!(self, Self::Fetch { .. })
    }
}

impl fmt::Display for BackgroundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScheme { reference } => write!(
                f,
                "Background `{reference}`: unsupported reference, expected gs://, file:// or https://"
            ),
            Self::SchemeNotConfigured { reference, scheme } => write!(
                f,
                "Background `{reference}`: {scheme} Backgrounds are not enabled on this service"
            ),
            Self::InvalidReference { reference, reason } => {
                write!(f, "Background `{reference}`: invalid reference: {reason}")
            }
            Self::OutsideBaseDir { reference } => write!(
                f,
                "Background `{reference}`: path leaves the Background base directory"
            ),
            Self::HostNotAllowed { reference, host } => write!(
                f,
                "Background `{reference}`: host `{host}` is not on the allow-list"
            ),
            Self::NotFound { reference } => write!(f, "Background `{reference}`: not found"),
            Self::Fetch { reference, message } => {
                write!(f, "Background `{reference}`: fetch failed: {message}")
            }
            Self::UnsupportedFormat { reference, reason } => {
                write!(f, "Background `{reference}`: unsupported image: {reason}")
            }
            Self::UnsupportedColorSpace {
                reference,
                color_space,
            } => write!(
                f,
                "Background `{reference}`: colour space {color_space} is not supported, only RGB and grayscale"
            ),
            Self::Decode { reference, message } => {
                write!(f, "Background `{reference}`: corrupt image: {message}")
            }
        }
    }
}

impl std::error::Error for BackgroundError {}
