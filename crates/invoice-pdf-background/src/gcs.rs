//! Fetching `gs://` Backgrounds through the GCS JSON API, authenticated with
//! an access token of the instance's service account from the metadata
//! server (Cloud Run, GKE, GCE). No other credential sources are supported.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::fetch::FetchError;
use crate::http;

const TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
const STORAGE_URL: &str = "https://storage.googleapis.com/storage/v1/b";

/// Tokens are refreshed this long before they expire.
const TOKEN_MARGIN: Duration = Duration::from_secs(60);

pub(crate) struct GcsClient {
    storage: ureq::Agent,
    metadata: ureq::Agent,
    token: Mutex<Option<(String, Instant)>>,
}

impl Default for GcsClient {
    fn default() -> Self {
        Self {
            storage: http::agent(true),
            // The metadata server is link-local; never route it via a proxy.
            metadata: http::agent(false),
            token: Mutex::new(None),
        }
    }
}

impl std::fmt::Debug for GcsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GcsClient").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct Token {
    access_token: String,
    expires_in: u64,
}

impl GcsClient {
    pub(crate) fn fetch(&self, bucket: &str, object: &str) -> Result<Vec<u8>, FetchError> {
        let url = object_url(bucket, object);
        let authorization = format!("Bearer {}", self.token()?);
        http::get(&self.storage, &url, &[("Authorization", &authorization)])
    }

    fn token(&self) -> Result<String, FetchError> {
        let mut cached = self.token.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((token, refresh_at)) = cached.as_ref()
            && Instant::now() < *refresh_at
        {
            return Ok(token.clone());
        }
        let body = http::get(&self.metadata, TOKEN_URL, &[("Metadata-Flavor", "Google")]).map_err(
            |error| {
                FetchError::Failed(format!(
                    "no GCS access token from the metadata server: {error}"
                ))
            },
        )?;
        let token: Token = serde_json::from_slice(&body).map_err(|error| {
            FetchError::Failed(format!("invalid metadata server token response: {error}"))
        })?;
        let lifetime = Duration::from_secs(token.expires_in).saturating_sub(TOKEN_MARGIN);
        *cached = Some((token.access_token.clone(), Instant::now() + lifetime));
        Ok(token.access_token)
    }
}

/// The JSON API media download URL of an object.
fn object_url(bucket: &str, object: &str) -> String {
    format!(
        "{STORAGE_URL}/{bucket}/o/{}?alt=media",
        percent_encode(object)
    )
}

/// Percent-encodes everything but RFC 3986 unreserved characters (so `/` in
/// object names becomes `%2F`, as the JSON API requires).
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_object_name_into_media_url() {
        assert_eq!(
            object_url("tenant-assets", "backgrounds/acme 1ü.png"),
            "https://storage.googleapis.com/storage/v1/b/tenant-assets/o/backgrounds%2Facme%201%C3%BC.png?alt=media"
        );
    }

    #[test]
    fn keeps_unreserved_characters() {
        assert_eq!(percent_encode("aZ09-._~"), "aZ09-._~");
        assert_eq!(percent_encode("?#%"), "%3F%23%25");
    }
}
