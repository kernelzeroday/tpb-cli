use crate::discover::DiscoveryResult;
use crate::torrent::{Torrent, source_summary};
use std::env;
use std::io::IsTerminal;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";

pub fn color_enabled(no_color: bool) -> bool {
    !no_color && env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

fn paint(code: &str, text: &str, enabled: bool) -> String {
    if enabled {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

pub fn print_torrents(torrents: &[Torrent], color: bool) {
    for (position, torrent) in torrents.iter().enumerate() {
        println!(
            "{}. {}  {}",
            paint(DIM, &(position + 1).to_string(), color),
            paint(&format!("{BOLD}{CYAN}"), &torrent.title, color),
            paint(DIM, &format!("[{}]", source_summary(torrent)), color)
        );
        let mut metadata = Vec::new();
        if let Some(size) = torrent.size_bytes {
            metadata.push(paint(YELLOW, &format_bytes(size), color));
        }
        if let Some(seeders) = torrent.seeders {
            let label = if torrent.averaged_swarm_stats {
                format!("{seeders} avg seeds")
            } else {
                format!("{seeders} seeds")
            };
            metadata.push(paint(GREEN, &label, color));
        }
        if let Some(leechers) = torrent.leechers {
            let label = if torrent.averaged_swarm_stats {
                format!("{leechers} avg leech")
            } else {
                format!("{leechers} leech")
            };
            metadata.push(paint(RED, &label, color));
        }
        if let Some(grabs) = torrent.grabs {
            metadata.push(paint(DIM, &format!("{grabs} grabs"), color));
        }
        if let Some(category) = &torrent.category {
            metadata.push(paint(MAGENTA, category, color));
        }
        if let Some(published) = &torrent.published {
            metadata.push(paint(DIM, published, color));
        }
        if torrent.sources.len() > 1 {
            metadata.push(paint(
                DIM,
                &format!("{} sources", torrent.sources.len()),
                color,
            ));
        }
        if !metadata.is_empty() {
            println!("   {}", metadata.join(&paint(DIM, " · ", color)));
        }
        if let Some(magnet) = &torrent.magnet {
            println!("{}", paint(DIM, magnet, color));
        } else if let Some(download) = &torrent.download {
            println!("{}", paint(DIM, download, color));
        } else if let Some(details) = &torrent.details {
            println!("{}", paint(DIM, details, color));
        }
        println!();
    }
}

pub fn print_discovery(results: &[DiscoveryResult], color: bool) {
    if results.is_empty() {
        println!(
            "{}",
            paint(YELLOW, "No reachable Torznab endpoints found.", color)
        );
        return;
    }
    println!(
        "{:<58} {:>8}  {}",
        paint(BOLD, "ENDPOINT", color),
        paint(BOLD, "LATENCY", color),
        paint(BOLD, "SOURCE", color)
    );
    println!("{}", paint(DIM, &"─".repeat(86), color));
    for result in results {
        println!(
            "{:<58} {:>7}{}  {}",
            paint(CYAN, &result.endpoint, color),
            paint(DIM, &result.latency_ms.to_string(), color),
            paint(DIM, "ms", color),
            paint(DIM, &result.origin, color)
        );
    }
    eprintln!(
        "\n{} verified endpoint(s)",
        paint(GREEN, &results.len().to_string(), color)
    );
}

pub fn format_bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{size:.2} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_byte_sizes_with_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.50 KiB");
        assert_eq!(format_bytes(1_073_741_824), "1.00 GiB");
    }
}
