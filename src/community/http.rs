//! Bounded HTTP reads with explicit transport failures for the two-channel policy.
use crate::common::error::{Error, Result};
use reqwest::blocking::Client;
use std::{io::Read, time::Duration};

/// Only missing content and transient network errors permit switching channels.
#[derive(Debug)]
pub struct FetchError {
    pub unavailable: bool,
    pub missing: bool,
    pub message: String,
}
impl FetchError {
    pub fn into_error(self) -> Error {
        Error::Other(self.message)
    }
}
/// A native HTTP client, with fixed deadlines and bounded redirects and bodies.
#[derive(Clone)]
pub struct Http {
    client: Client,
}
impl Http {
    /// Build an HTTPS transport with bounded redirects and request deadlines.
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("aidget/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                    attempt.error("invalid download redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(Self { client })
    }
    /// Read a public resource, refusing oversized bodies even without Content-Length.
    pub fn get(&self, url: &str, max: usize) -> std::result::Result<Vec<u8>, FetchError> {
        let response = self.client.get(url).send().map_err(|e| FetchError {
            unavailable: !e.is_redirect() && (e.is_timeout() || e.is_connect() || e.is_request()),
            missing: false,
            message: format!("GET {url}: {e}"),
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(FetchError {
                unavailable: status.as_u16() == 404
                    || status.as_u16() == 429
                    || status.is_server_error(),
                missing: status.as_u16() == 404,
                message: format!("GET {url}: HTTP {status}"),
            });
        }
        if response.content_length().is_some_and(|n| n > max as u64) {
            return Err(FetchError {
                unavailable: false,
                missing: false,
                message: format!("GET {url}: body exceeds {max} bytes"),
            });
        }
        let mut bytes = vec![];
        response
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| FetchError {
                unavailable: true,
                missing: false,
                message: format!("GET {url}: {e}"),
            })?;
        if bytes.len() > max {
            return Err(FetchError {
                unavailable: false,
                missing: false,
                message: format!("GET {url}: body exceeds {max} bytes"),
            });
        }
        Ok(bytes)
    }
}
