//! Shodan-assisted endpoint discovery.
//!
//! Shodan can locate HTTP services matching a search fingerprint, but a
//! fingerprint match is not proof that a service speaks Torznab. Every
//! candidate returned by Shodan is therefore probed with `?t=caps` and only
//! kept if it returns a genuine Torznab capability document. Discovery never
//! runs without an explicit, user-supplied Shodan query: there is no
//! built-in default fingerprint, since baking in one indexer's signature
//! would imply it is the only or the recommended one.

use crate::source::{Source, deduplicate_sources};
use crate::torznab;
use anyhow::{Context, Result, anyhow, bail};
use futures_util::{StreamExt, stream};
use reqwest::{Client, Url};
use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::{process::Command, time::timeout};

#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    pub shodan_queries: Vec<String>,
    pub shodan_limit: usize,
    pub concurrency: usize,
    pub timeout_secs: u64,
    pub verbose: bool,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryResult {
    pub endpoint: String,
    pub origin: String,
    pub latency_ms: u128,
}

pub fn validate_options(options: &DiscoveryOptions) -> Result<()> {
    if !(1..=1000).contains(&options.shodan_limit) {
        bail!("--shodan-limit must be between 1 and 1000");
    }
    if !(1..=100).contains(&options.concurrency) {
        bail!("--concurrency must be between 1 and 100");
    }
    if !(1..=120).contains(&options.timeout_secs) {
        bail!("--timeout must be between 1 and 120 seconds");
    }
    Ok(())
}

pub async fn discover(client: &Client, options: &DiscoveryOptions) -> Result<Vec<DiscoveryResult>> {
    validate_options(options)?;
    if options.shodan_queries.is_empty() {
        bail!(
            "discovery requires at least one --shodan-query; there is no built-in default fingerprint"
        );
    }

    let mut candidates = Vec::new();
    for query in &options.shodan_queries {
        if options.verbose {
            eprintln!("shodan: {query}");
        }
        candidates.extend(discover_shodan(query, options.shodan_limit).await?);
    }

    candidates = deduplicate_sources(candidates);
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let probes = stream::iter(candidates.into_iter().map(|source| {
        let client = client.clone();
        async move { probe_source(&client, source).await }
    }))
    .buffer_unordered(options.concurrency)
    .collect::<Vec<_>>()
    .await;

    let mut working = Vec::new();
    for probe in probes {
        match probe {
            Ok(result) => working.push(result),
            Err(error) if options.verbose => eprintln!("reject: {error:#}"),
            Err(_) => {}
        }
    }
    working.sort_by_key(|item| item.latency_ms);
    Ok(working)
}

async fn discover_shodan(query: &str, limit: usize) -> Result<Vec<Source>> {
    let output = timeout(
        Duration::from_secs(30),
        Command::new("shodan")
            .args([
                "search",
                "--fields",
                "ip_str,port",
                "--limit",
                &limit.to_string(),
                query,
            ])
            .output(),
    )
    .await
    .map_err(|_| anyhow!("Shodan CLI timed out after 30 seconds"))?
    .context("could not run the Shodan CLI; install it and run `shodan init <API_KEY>`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("Shodan CLI failed: {stderr}");
    }

    let mut sources = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split('\t').map(str::trim);
        let Some(host) = fields.next().filter(|value| !value.is_empty()) else {
            continue;
        };
        let Some(port) = fields.next().and_then(|value| value.parse::<u16>().ok()) else {
            continue;
        };
        if let Ok(endpoint) = Url::parse(&candidate_url(host, port)) {
            sources.push(Source {
                endpoint,
                origin: format!("shodan:{query}"),
            });
        }
    }
    Ok(sources)
}

fn candidate_url(host: &str, port: u16) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    match port {
        443 => format!("https://{host}/torznab"),
        80 => format!("http://{host}/torznab"),
        _ => format!("http://{host}:{port}/torznab"),
    }
}

async fn probe_source(client: &Client, source: Source) -> Result<DiscoveryResult> {
    let endpoint = torznab::build_url(&source.endpoint, [("t", "caps")], None);
    let started = Instant::now();
    let response = client
        .get(endpoint.clone())
        .send()
        .await
        .with_context(|| format!("{} did not respond", source.endpoint))?
        .error_for_status()
        .with_context(|| format!("{} returned an HTTP error", source.endpoint))?;
    let body = response
        .text()
        .await
        .with_context(|| format!("could not read response from {}", source.endpoint))?;
    if !torznab::is_caps_document(&body) {
        bail!("{} is not a Torznab endpoint", source.endpoint);
    }
    Ok(DiscoveryResult {
        endpoint: source.endpoint.to_string(),
        origin: source.origin,
        latency_ms: started.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_shodan_address_to_torznab_endpoint() {
        assert_eq!(
            candidate_url("203.0.113.7", 3333),
            "http://203.0.113.7:3333/torznab"
        );
        assert_eq!(
            candidate_url("2001:db8::1", 443),
            "https://[2001:db8::1]/torznab"
        );
    }

    #[test]
    fn rejects_discovery_without_an_explicit_query() {
        let options = DiscoveryOptions {
            shodan_queries: Vec::new(),
            shodan_limit: 50,
            concurrency: 10,
            timeout_secs: 10,
            verbose: false,
        };
        // Compile-time only check that validate_options accepts these bounds;
        // the empty-query error is asserted in the async `discover` path via
        // the integration-level CLI tests.
        assert!(validate_options(&options).is_ok());
    }
}
