//! Client for The Pirate Bay's public JSON search API (widely known as
//! "apibay"), plus the handful of mirrors that run the same software.
//!
//! There is no capability handshake for an arbitrary candidate the way
//! Torznab has `?t=caps`. Instead, a candidate is validated by issuing a
//! real search request and checking that the response actually deserializes
//! into this API's expected shape (see [`parse_response`]) before it is
//! ever trusted as a source.

use crate::torrent::Torrent;
use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

/// The API's sentinel row for "no results": a single entry with this id and
/// an all-zero info hash, rather than an empty array.
const NO_RESULTS_ID: &str = "0";

#[derive(Debug, Deserialize)]
struct ApiTorrent {
    id: String,
    name: String,
    info_hash: String,
    leechers: String,
    seeders: String,
    size: String,
    added: String,
    category: String,
}

pub fn build_search_url(base: &Url, query: &str) -> Url {
    let mut url = base.clone();
    url.set_path("/q.php");
    url.query_pairs_mut().clear().append_pair("q", query);
    url
}

pub fn build_probe_url(base: &Url) -> Url {
    build_search_url(base, "test")
}

/// Parses a search response. A response that doesn't even deserialize into
/// this API's row shape is a hard error; one that does, but only contains
/// the no-results sentinel, is a legitimate empty result set.
pub fn parse_response(body: &str) -> Result<Vec<Torrent>> {
    let rows: Vec<ApiTorrent> =
        serde_json::from_str(body).context("response is not a recognized Pirate Bay API result")?;
    let torrents = rows
        .into_iter()
        .filter(|row| row.id != NO_RESULTS_ID)
        .filter_map(|row| {
            if row.info_hash.is_empty() || row.name.is_empty() {
                return None;
            }
            Some(Torrent {
                title: row.name.clone(),
                magnet: Some(format!(
                    "magnet:?xt=urn:btih:{}&dn={}",
                    row.info_hash,
                    urlencoding_light(&row.name)
                )),
                category: (row.category != "0").then_some(row.category),
                size_bytes: row.size.parse().ok(),
                seeders: row.seeders.parse().ok(),
                leechers: row.leechers.parse().ok(),
                published: row.added.parse::<i64>().ok().map(format_unix_timestamp),
                ..Torrent::default()
            })
        })
        .collect();
    Ok(torrents)
}

/// Response validity check used to accept or reject a discovered candidate:
/// does the body deserialize into this API's expected shape at all?
pub fn is_valid_response(body: &str) -> bool {
    serde_json::from_str::<Vec<ApiTorrent>>(body).is_ok()
}

/// A minimal, dependency-free percent-encoder for the magnet `dn` parameter.
/// Space and reserved URL characters are the only ones a torrent title is
/// likely to contain that need escaping here.
fn urlencoding_light(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn format_unix_timestamp(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let secs_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// Howard Hinnant's `civil_from_days`: converts a day count since the Unix
/// epoch into a proleptic-Gregorian (year, month, day). Avoids pulling in a
/// full date/time crate for one field.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_query_url() {
        let base = Url::parse("https://apibay.example").unwrap();
        let url = build_search_url(&base, "ubuntu 24.04");
        assert_eq!(url.as_str(), "https://apibay.example/q.php?q=ubuntu+24.04");
    }

    #[test]
    fn parses_a_results_response() {
        let body = include_str!("../tests/fixtures/results.json");
        let results = parse_response(body).expect("recognized response");
        assert_eq!(results.len(), 2);

        let first = &results[0];
        assert_eq!(first.title, "Ubuntu 24.04");
        assert_eq!(
            first.magnet.as_deref(),
            Some("magnet:?xt=urn:btih:ABC123DEF456&dn=Ubuntu%2024.04")
        );
        assert_eq!(first.seeders, Some(27));
        assert_eq!(first.leechers, Some(4));
        assert_eq!(first.size_bytes, Some(1_503_238_553));
        assert_eq!(first.category.as_deref(), Some("500"));
        assert_eq!(first.published.as_deref(), Some("2026-01-01 00:00 UTC"));
    }

    #[test]
    fn treats_the_no_results_sentinel_as_zero_results() {
        let body = include_str!("../tests/fixtures/empty.json");
        let results = parse_response(body).expect("recognized response");
        assert!(results.is_empty());
    }

    #[test]
    fn rejects_a_response_that_does_not_match_the_api_shape() {
        let body = include_str!("../tests/fixtures/malformed.json");
        assert!(parse_response(body).is_err());
        assert!(!is_valid_response(body));
    }

    #[test]
    fn converts_epoch_seconds_to_a_readable_utc_timestamp() {
        assert_eq!(format_unix_timestamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_unix_timestamp(1_704_067_200), "2024-01-01 00:00 UTC");
        assert_eq!(format_unix_timestamp(1_767_225_600), "2026-01-01 00:00 UTC");
    }
}
