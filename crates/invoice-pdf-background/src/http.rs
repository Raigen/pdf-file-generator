//! Minimal blocking HTTP GET shared by the `https://` and `gs://` fetchers.

use std::time::Duration;

use crate::fetch::FetchError;

/// Largest Background accepted over the network.
pub(crate) const MAX_FETCH_BYTES: u64 = 32 * 1024 * 1024;

const TIMEOUT: Duration = Duration::from_secs(30);

/// An agent that never follows redirects (a redirect could lead off the
/// allow-list) and reports every status as a response, not an error.
pub(crate) fn agent(use_env_proxy: bool) -> ureq::Agent {
    let mut config = ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .user_agent(concat!(
            "invoice-pdf-background/",
            env!("CARGO_PKG_VERSION")
        ));
    if !use_env_proxy {
        config = config.proxy(None);
    }
    config.build().into()
}

/// GETs `url` and returns the body of a `200 OK` response; `404` is
/// [`FetchError::NotFound`], any other status (redirects included) a failure.
pub(crate) fn get(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<Vec<u8>, FetchError> {
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request
        .call()
        .map_err(|error| FetchError::Failed(error.to_string()))?;
    match response.status().as_u16() {
        200 => response
            .body_mut()
            .with_config()
            .limit(MAX_FETCH_BYTES)
            .read_to_vec()
            .map_err(|error| FetchError::Failed(error.to_string())),
        404 => Err(FetchError::NotFound),
        status => Err(FetchError::Failed(format!("HTTP status {status}"))),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    /// Serves one raw HTTP response on a local port; returns the base URL.
    fn serve_once(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
                line.clear();
            }
            reader.get_mut().write_all(response.as_bytes()).unwrap();
        });
        format!("http://{address}/acme.png")
    }

    #[test]
    fn returns_body_of_ok_response() {
        let url =
            serve_once("HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc");
        assert_eq!(get(&agent(false), &url, &[]), Ok(b"abc".to_vec()));
    }

    #[test]
    fn maps_404_to_not_found() {
        let url =
            serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        assert_eq!(get(&agent(false), &url, &[]), Err(FetchError::NotFound));
    }

    #[test]
    fn does_not_follow_redirects() {
        let url = serve_once(
            "HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        assert_eq!(
            get(&agent(false), &url, &[]),
            Err(FetchError::Failed("HTTP status 302".into()))
        );
    }
}
