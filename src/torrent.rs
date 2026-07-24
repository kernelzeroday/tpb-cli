use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Torrent {
    pub source: String,
    #[serde(default)]
    pub sources: Vec<String>,
    pub title: String,
    pub magnet: Option<String>,
    pub download: Option<String>,
    pub details: Option<String>,
    pub category: Option<String>,
    pub published: Option<String>,
    pub size_bytes: Option<u64>,
    pub seeders: Option<u64>,
    pub leechers: Option<u64>,
    pub grabs: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub averaged_swarm_stats: bool,
    #[serde(skip)]
    pub(crate) seeder_total: u64,
    #[serde(skip)]
    pub(crate) seeder_samples: u64,
    #[serde(skip)]
    pub(crate) leecher_total: u64,
    #[serde(skip)]
    pub(crate) leecher_samples: u64,
}

fn is_false(value: &bool) -> bool {
    !value
}

/// Merge results that share a BitTorrent info hash, averaging swarm counts
/// across the indexers that reported them.
pub fn condense_by_hash(torrents: Vec<Torrent>) -> Vec<Torrent> {
    let mut positions = HashMap::new();
    let mut condensed: Vec<Torrent> = Vec::new();

    for mut torrent in torrents {
        initialize_swarm_samples(&mut torrent);
        if torrent.sources.is_empty() && !torrent.source.is_empty() {
            torrent.sources.push(torrent.source.clone());
        }
        let Some(hash) = torrent_hash(&torrent) else {
            condensed.push(torrent);
            continue;
        };

        if let Some(&position) = positions.get(&hash) {
            merge_torrent(&mut condensed[position], torrent);
        } else {
            positions.insert(hash, condensed.len());
            condensed.push(torrent);
        }
    }
    for torrent in &mut condensed {
        torrent.averaged_swarm_stats = torrent.sources.len() > 1;
    }
    condensed
}

fn torrent_hash(torrent: &Torrent) -> Option<String> {
    let magnet = torrent.magnet.as_deref()?;
    Url::parse(magnet)
        .ok()?
        .query_pairs()
        .find_map(|(key, value)| {
            if key != "xt" {
                return None;
            }
            let value = value.to_ascii_lowercase();
            value
                .strip_prefix("urn:btih:")
                .or_else(|| value.strip_prefix("urn:btmh:"))
                .map(str::to_string)
        })
}

fn merge_torrent(target: &mut Torrent, incoming: Torrent) {
    target.seeder_total += incoming.seeder_total;
    target.seeder_samples += incoming.seeder_samples;
    target.seeders = average_option(target.seeder_total, target.seeder_samples);
    target.leecher_total += incoming.leecher_total;
    target.leecher_samples += incoming.leecher_samples;
    target.leechers = average_option(target.leecher_total, target.leecher_samples);
    target.grabs = max_option(target.grabs, incoming.grabs);
    if target.size_bytes.is_none() {
        target.size_bytes = incoming.size_bytes;
    }
    if target.category.is_none() {
        target.category = incoming.category;
    }
    if target.published.is_none() {
        target.published = incoming.published;
    }
    for source in incoming.sources {
        if !target.sources.contains(&source) {
            target.sources.push(source);
        }
    }
}

fn initialize_swarm_samples(torrent: &mut Torrent) {
    if torrent.seeder_samples == 0
        && let Some(seeders) = torrent.seeders
    {
        torrent.seeder_total = seeders;
        torrent.seeder_samples = 1;
    }
    if torrent.leecher_samples == 0
        && let Some(leechers) = torrent.leechers
    {
        torrent.leecher_total = leechers;
        torrent.leecher_samples = 1;
    }
}

fn average_option(total: u64, count: u64) -> Option<u64> {
    (count > 0).then(|| (total + count / 2) / count)
}

fn max_option(current: Option<u64>, candidate: Option<u64>) -> Option<u64> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.max(candidate)),
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (None, None) => None,
    }
}

pub fn source_summary(torrent: &Torrent) -> String {
    match torrent.sources.as_slice() {
        [] => torrent.source.clone(),
        [source] => source.clone(),
        [first, rest @ ..] => format!("{first} +{} more", rest.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condenses_identical_info_hashes_and_averages_swarm_counts() {
        let first = Torrent {
            source: "one.example".into(),
            sources: vec!["one.example".into()],
            title: "Example torrent".into(),
            magnet: Some("magnet:?xt=urn:btih:ABC123&dn=Example".into()),
            seeders: Some(3),
            leechers: Some(8),
            ..Torrent::default()
        };
        let second = Torrent {
            source: "two.example".into(),
            sources: vec!["two.example".into()],
            title: "Example torrent (alternate title)".into(),
            magnet: Some("magnet:?xt=urn:btih:abc123&dn=Another".into()),
            seeders: Some(11),
            leechers: Some(4),
            ..Torrent::default()
        };

        let condensed = condense_by_hash(vec![first, second]);
        assert_eq!(condensed.len(), 1);
        assert_eq!(condensed[0].seeders, Some(7));
        assert_eq!(condensed[0].leechers, Some(6));
        assert_eq!(condensed[0].sources, ["one.example", "two.example"]);
        assert!(condensed[0].averaged_swarm_stats);
    }

    #[test]
    fn leaves_distinct_hashes_uncondensed() {
        let first = Torrent {
            source: "one.example".into(),
            title: "First".into(),
            magnet: Some("magnet:?xt=urn:btih:AAA".into()),
            ..Torrent::default()
        };
        let second = Torrent {
            source: "one.example".into(),
            title: "Second".into(),
            magnet: Some("magnet:?xt=urn:btih:BBB".into()),
            ..Torrent::default()
        };
        let condensed = condense_by_hash(vec![first, second]);
        assert_eq!(condensed.len(), 2);
        assert!(!condensed[0].averaged_swarm_stats);
    }

    #[test]
    fn keeps_results_without_a_magnet_link_separate() {
        let first = Torrent {
            title: "No magnet A".into(),
            ..Torrent::default()
        };
        let second = Torrent {
            title: "No magnet B".into(),
            ..Torrent::default()
        };
        let condensed = condense_by_hash(vec![first, second]);
        assert_eq!(condensed.len(), 2);
    }
}
