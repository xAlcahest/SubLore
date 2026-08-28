//! The only code in Sublore that opens a socket. See BACKLOG.md M3.2.
//!
//! It holds no address of its own: the URL is built from the catalog and passed in, which is what
//! makes "no network unless the user asks" checkable by grep as well as by test. Compression is
//! off, so `Content-Length` counts the same bytes the file has and a resume offset means what it
//! says.

use std::time::Duration;

use crate::error::{AsrError, AsrErrorKind};
use crate::model::download::{Fetched, RangeFetcher};

/// Long enough for a slow link to answer, short enough that a dead server is not forever.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// The whole exchange up to the first byte of the body. The body itself is not timed: a 3 GB
/// download over a slow line is legitimate.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct HttpFetcher {
    agent: ureq::Agent,
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(RESPONSE_TIMEOUT))
            // Statuses are read, not thrown: a 200 where a 206 was asked for is a restart, not a
            // failure, and the caller has to be able to tell the difference.
            .http_status_as_error(false)
            .user_agent(concat!("Sublore/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl RangeFetcher for HttpFetcher {
    fn get(&self, url: &str, from: u64) -> Result<Fetched, AsrError> {
        let mut request = self.agent.get(url);
        if from > 0 {
            request = request.header("Range", &format!("bytes={from}-"));
        }
        let response = request.call().map_err(|error| {
            AsrError::new(AsrErrorKind::NetworkFailed, format!("{url}: {error}"))
        })?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(AsrError::new(
                AsrErrorKind::NetworkFailed,
                format!("{url} answered {status}"),
            ));
        }
        let content_range = header(&response, "content-range");
        let content_length =
            header(&response, "content-length").and_then(|value| value.parse().ok());

        // 206 means the server honoured the range; anything else means it sent the whole file,
        // whatever we asked for.
        let (start, total) = match (status, content_range.as_deref()) {
            (206, Some(range)) => parse_content_range(range).ok_or_else(|| {
                AsrError::new(
                    AsrErrorKind::NetworkFailed,
                    format!("{url} sent an unreadable Content-Range: {range}"),
                )
            })?,
            (206, None) => {
                return Err(AsrError::new(
                    AsrErrorKind::NetworkFailed,
                    format!("{url} sent a partial body with no Content-Range"),
                ))
            }
            _ => (0, content_length),
        };

        Ok(Fetched {
            start,
            total,
            body: Box::new(response.into_body().into_reader()),
        })
    }
}

fn header(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)?
        .to_str()
        .ok()
        .map(str::to_owned)
}

/// `bytes 100-199/200` -> (100, Some(200)). `bytes 100-199/*` -> (100, None).
fn parse_content_range(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes ")?;
    let (range, total) = rest.split_once('/')?;
    let (start, _) = range.split_once('-')?;
    let start = start.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        digits => Some(digits.parse().ok()?),
    };
    Some((start, total))
}

#[cfg(test)]
mod tests {
    use super::parse_content_range;

    #[test]
    fn a_content_range_gives_the_offset_and_the_whole_size() {
        assert_eq!(
            parse_content_range("bytes 100-199/200"),
            Some((100, Some(200)))
        );
        assert_eq!(parse_content_range("bytes 0-0/1"), Some((0, Some(1))));
        assert_eq!(parse_content_range("bytes 5-9/*"), Some((5, None)));
    }

    #[test]
    fn anything_else_is_not_a_content_range() {
        for value in [
            "",
            "items 1-2/3",
            "bytes 100-199",
            "bytes x-y/200",
            "bytes /200",
        ] {
            assert_eq!(parse_content_range(value), None, "{value:?}");
        }
    }
}
