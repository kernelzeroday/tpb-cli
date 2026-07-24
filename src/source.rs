use anyhow::{Context, Result, bail};
use reqwest::Url;
use std::collections::HashSet;
use std::env;

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

/// Resolves the sources to search, in priority order: explicit `--indexer`
/// flags, then `TPB_INDEXERS`, then any endpoints saved by a previous
/// `tpb discover`.
pub fn configured_sources(indexers: &[String]) -> Result<Vec<Source>> {
    if !indexers.is_empty() {
        return parse_sources(indexers.to_vec(), "cli");
    }

    if let Some(value) = env::var_os("TPB_INDEXERS") {
        let sources = value
            .to_string_lossy()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        return parse_sources(sources, "environment");
    }

    parse_sources(crate::cache::load_discovered_sources()?, "discovery cache")
}

pub fn parse_sources(raw: Vec<String>, origin: &str) -> Result<Vec<Source>> {
    raw.into_iter()
        .map(|input| {
            let endpoint = Url::parse(&input).with_context(|| {
                format!("invalid indexer URL `{input}`; pass the full Torznab endpoint URL")
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
        let result = parse_sources(vec!["ftp://example.test/torznab".to_string()], "cli");
        assert!(result.is_err());
    }

    #[test]
    fn deduplicates_by_normalized_endpoint() {
        let sources = parse_sources(
            vec![
                "http://example.test/torznab/".to_string(),
                "HTTP://EXAMPLE.TEST/torznab".to_string(),
            ],
            "cli",
        )
        .unwrap();
        assert_eq!(deduplicate_sources(sources).len(), 1);
    }
}
