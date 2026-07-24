use crate::proxy;
use crate::source::Source;
use crate::torrent::Torrent;
use anyhow::{Context, Result};
use futures_util::{StreamExt, stream};
use reqwest::Client;
use std::time::Duration;

pub const USER_AGENT: &str = concat!("tpb/", env!("CARGO_PKG_VERSION"));

/// A single retry follows a transient network failure (timeout, connection
/// reset). It does not retry HTTP error statuses or malformed responses,
/// since those will not resolve on an immediate second attempt.
const TRANSIENT_RETRY_DELAY: Duration = Duration::from_millis(250);

pub fn build_client(timeout_seconds: u64) -> Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(timeout_seconds))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .context("could not build HTTP client")
}

pub async fn search_source(client: &Client, source: Source, query: &str) -> Result<Vec<Torrent>> {
    match search_source_once(client, &source, query).await {
        Ok(results) => Ok(results),
        Err(error) if is_transient(&error) => {
            tokio::time::sleep(TRANSIENT_RETRY_DELAY).await;
            search_source_once(client, &source, query).await
        }
        Err(error) => Err(error),
    }
}

/// Timeouts and connection failures are worth one immediate retry; HTTP
/// error statuses and malformed responses are not, since they will not
/// resolve on a second attempt a quarter-second later.
fn is_transient(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<reqwest::Error>()
        .is_some_and(|error| error.is_timeout() || error.is_connect())
}

async fn search_source_once(client: &Client, source: &Source, query: &str) -> Result<Vec<Torrent>> {
    let endpoint = proxy::build_search_url(&source.endpoint, query);
    let response = client
        .get(endpoint)
        .send()
        .await
        .with_context(|| format!("{} did not respond", source.endpoint))?
        .error_for_status()
        .with_context(|| format!("{} returned an HTTP error", source.endpoint))?;
    let body = response
        .text()
        .await
        .with_context(|| format!("could not read response from {}", source.endpoint))?;

    let mut results =
        proxy::parse_response(&body).with_context(|| format!("{}", source.endpoint))?;
    for result in &mut results {
        result.source = source.label();
        result.sources = vec![result.source.clone()];
    }
    Ok(results)
}

/// Searches `sources` in fanout-sized batches, trying the next batch only if
/// the previous one produced no results. Returns the combined results
/// alongside any per-source failures.
pub async fn search_all(
    client: &Client,
    sources: &[Source],
    query: &str,
    fanout: usize,
    concurrency: usize,
) -> (Vec<Torrent>, Vec<anyhow::Error>) {
    let mut torrents = Vec::new();
    let mut failures = Vec::new();

    for batch in sources.chunks(fanout) {
        let searches = stream::iter(batch.iter().cloned().map(|source| {
            let client = client.clone();
            let query = query.to_string();
            async move { search_source(&client, source, &query).await }
        }))
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;

        for result in searches {
            match result {
                Ok(mut response) => torrents.append(&mut response),
                Err(error) => failures.push(error),
            }
        }
        if !torrents.is_empty() {
            break;
        }
    }

    (torrents, failures)
}
