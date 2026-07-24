use anyhow::{Context, Result, bail};
use reqwest::Url;
use std::collections::HashSet;
use std::env;

/// Public mirrors known to run this exact API, tried when nothing else is
/// configured or cached. This is intentionally short and only includes
/// mirrors actually verified to respond correctly; Shodan discovery (see
/// `crate::discover`) is the mechanism for finding others, since there is
/// no reliable way to fingerprint this API's mirrors at scale.
pub const KNOWN_MIRRORS: &[&str] = &["https://apibay.org"];

#[derive(Debug, Clone)]
pub struct Source {
    pub endpoint: Url,
    pub origin: String,
}

impl Source {
    /// A short, stable label for terminal output and per-result attribution.
    pub fn label(&self) -> String {
        let host = self
            .endpoint
            .host_str()
            .map(str::to_string)
            .unwrap_or_else(|| self.endpoint.to_string());
        match self.endpoint.port() {
            Some(port) => format!("{host}:{port}"),
            None => host,
        }
    }
}

/// Resolves the sources to search, in priority order: explicit `--proxy`
/// flags, then `TPB_PROXIES`, then any endpoints saved by a previous
/// `tpb discover`, then the small built-in [`KNOWN_MIRRORS`] fallback.
/// Supporting several independent sources at once, rather than one
/// hardcoded default host, is what makes search decentralized: no single
/// mirror going down or rate-limiting stops a search.
pub fn configured_sources(proxies: &[String]) -> Result<Vec<Source>> {
    if !proxies.is_empty() {
        return parse_sources(proxies.to_vec(), "cli");
    }

    if let Some(value) = env::var_os("TPB_PROXIES") {
        let sources = value
            .to_string_lossy()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        return parse_sources(sources, "environment");
    }

    let discovered = crate::cache::load_discovered_sources()?;
    if !discovered.is_empty() {
        return parse_sources(discovered, "discovery cache");
    }

    parse_sources(
        KNOWN_MIRRORS.iter().map(|url| url.to_string()).collect(),
        "known mirror",
    )
}

pub fn parse_sources(raw: Vec<String>, origin: &str) -> Result<Vec<Source>> {
    raw.into_iter()
        .map(|input| {
            let endpoint = Url::parse(&input).with_context(|| {
                format!(
                    "invalid proxy URL `{input}`; pass the full base URL of a Pirate Bay API mirror"
                )
            })?;
            match endpoint.scheme() {
                "http" | "https" => Ok(Source {
                    endpoint,
                    origin: origin.to_string(),
                }),
                scheme => bail!("unsupported URL scheme `{scheme}` for `{input}`"),
            }
        })
        .collect()
}

pub fn deduplicate_sources(sources: Vec<Source>) -> Vec<Source> {
    let mut seen = HashSet::new();
    sources
        .into_iter()
        .filter(|source| {
            seen.insert(
                source
                    .endpoint
                    .as_str()
                    .trim_end_matches('/')
                    .to_lowercase(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_schemes() {
        let result = parse_sources(vec!["ftp://example.test".to_string()], "cli");
        assert!(result.is_err());
    }

    #[test]
    fn deduplicates_by_normalized_endpoint() {
        let sources = parse_sources(
            vec![
                "http://example.test/".to_string(),
                "HTTP://EXAMPLE.TEST".to_string(),
            ],
            "cli",
        )
        .unwrap();
        assert_eq!(deduplicate_sources(sources).len(), 1);
    }

    #[test]
    fn known_mirrors_are_non_empty_and_parse_as_valid_sources() {
        assert!(!KNOWN_MIRRORS.is_empty());
        let sources = parse_sources(
            KNOWN_MIRRORS.iter().map(|url| url.to_string()).collect(),
            "known mirror",
        )
        .expect("every built-in known mirror must be a valid http(s) URL");
        assert_eq!(sources.len(), KNOWN_MIRRORS.len());
    }
}
