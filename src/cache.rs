//! Local persistence: verified indexer endpoints (configuration, kept until
//! explicitly replaced) and search results (a cache, expired by TTL and
//! clearable on demand).

use crate::torrent::Torrent;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SEARCH_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Deserialize, Serialize)]
struct CachedSearch {
    saved_at_secs: u64,
    torrents: Vec<Torrent>,
}

fn config_dir() -> Option<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(path).join("tpb"));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/tpb"))
}

fn cache_dir() -> Option<PathBuf> {
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(path).join("tpb"));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache/tpb"))
}

fn discovered_sources_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("indexers"))
}

pub fn load_discovered_sources() -> Result<Vec<String>> {
    let Some(path) = discovered_sources_path() else {
        return Ok(Vec::new());
    };
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("could not read saved endpoints from {}", path.display()))?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

pub fn save_discovered_sources(endpoints: &[String]) -> Result<Option<PathBuf>> {
    if endpoints.is_empty() {
        return Ok(None);
    }
    let Some(path) = discovered_sources_path() else {
        return Ok(None);
    };
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("discovery cache path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "could not create discovery cache directory {}",
            parent.display()
        )
    })?;
    let content = endpoints.join("\n");
    fs::write(&path, format!("{content}\n"))
        .with_context(|| format!("could not save endpoints to {}", path.display()))?;
    Ok(Some(path))
}

fn search_cache_path(query: &str) -> Option<PathBuf> {
    Some(
        cache_dir()?
            .join("searches")
            .join(format!("{:016x}.json", query_hash(query))),
    )
}

fn query_hash(query: &str) -> u64 {
    // A stable cache filename, not a security boundary.
    query
        .to_lowercase()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

pub fn load_cached_search(query: &str) -> Option<Vec<Torrent>> {
    let path = search_cache_path(query)?;
    let content = fs::read_to_string(path).ok()?;
    let cached: CachedSearch = serde_json::from_str(&content).ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    if now.saturating_sub(cached.saved_at_secs) <= SEARCH_CACHE_TTL.as_secs() {
        Some(cached.torrents)
    } else {
        None
    }
}

pub fn save_cached_search(query: &str, torrents: &[Torrent]) -> Result<()> {
    let Some(path) = search_cache_path(query) else {
        return Ok(());
    };
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("search cache path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "could not create search cache directory {}",
            parent.display()
        )
    })?;
    let saved_at_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    let content = serde_json::to_string(&CachedSearch {
        saved_at_secs,
        torrents: torrents.to_vec(),
    })?;
    fs::write(&path, content)
        .with_context(|| format!("could not write search cache {}", path.display()))
}

/// Removes cached search results. Discovered indexer endpoints are treated
/// as configuration rather than a cache and are left untouched; pass
/// `include_indexers` to remove them as well.
pub fn clear(include_indexers: bool) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    if let Some(dir) = cache_dir().map(|dir| dir.join("searches"))
        && dir.exists()
    {
        fs::remove_dir_all(&dir).with_context(|| format!("could not remove {}", dir.display()))?;
        removed.push(dir);
    }
    if include_indexers
        && let Some(path) = discovered_sources_path()
        && path.exists()
    {
        fs::remove_file(&path).with_context(|| format!("could not remove {}", path.display()))?;
        removed.push(path);
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_hash_is_case_insensitive_and_stable() {
        assert_eq!(query_hash("Ubuntu 24.04"), query_hash("ubuntu 24.04"));
        assert_eq!(query_hash("same"), query_hash("same"));
    }
}
