//! `tpb`: a terminal-first search client for Torznab-compatible torrent
//! indexers. See `PROJECT_BRIEF.md` for the design rationale, in particular
//! why Torznab (a documented, self-describing protocol) was chosen over
//! scraping an arbitrary proxy-served web interface.

mod cache;
mod cli;
mod discover;
mod render;
mod search;
mod source;
mod torrent;
mod torznab;

use anyhow::{Result, bail};
use cli::{CacheCommand, Cli, CommandKind, DiscoverArgs, SearchArgs};
use discover::{DiscoveryOptions, discover};
use source::{Source, configured_sources, deduplicate_sources};
use std::process::ExitCode;
use torrent::condense_by_hash;
use url::Url;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = cli::parse_cli();
    let color = render::color_enabled(cli.no_color);
    match run(cli, color).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli, color: bool) -> Result<()> {
    match cli.command {
        CommandKind::Search(args) => run_search(args, color).await,
        CommandKind::Discover(args) => run_discover(args, color).await,
        CommandKind::Cache(args) => run_cache(args),
    }
}

fn discovery_options(options: cli::DiscoveryOptions) -> DiscoveryOptions {
    DiscoveryOptions {
        shodan_queries: options.shodan_queries,
        shodan_limit: options.shodan_limit,
        concurrency: options.concurrency,
        timeout_secs: options.timeout,
        verbose: options.verbose,
    }
}

async fn run_discover(args: DiscoverArgs, color: bool) -> Result<()> {
    let discovery = discovery_options(args.discovery);
    let client = search::build_client(discovery.timeout_secs)?;
    let results = discover(&client, &discovery).await?;
    announce_saved_endpoints(&results)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        render::print_discovery(&results, color);
    }
    Ok(())
}

async fn run_search(args: SearchArgs, color: bool) -> Result<()> {
    let discovery = discovery_options(args.discovery);
    discover::validate_options(&discovery)?;
    if !(1..=100).contains(&args.limit) {
        bail!("--limit must be between 1 and 100");
    }
    if !(1..=100).contains(&args.per_source_limit) {
        bail!("--per-source-limit must be between 1 and 100");
    }
    if !(1..=100).contains(&args.fanout) {
        bail!("--fanout must be between 1 and 100");
    }

    let client = search::build_client(discovery.timeout_secs)?;
    let mut sources = configured_sources(&args.indexer)?;

    if args.shodan {
        let discovered = discover(&client, &discovery).await?;
        announce_saved_endpoints(&discovered)?;
        sources.extend(discovered.into_iter().filter_map(|result| {
            Url::parse(&result.endpoint).ok().map(|endpoint| Source {
                endpoint,
                origin: result.origin,
            })
        }));
    }

    sources = deduplicate_sources(sources);
    if sources.is_empty() {
        bail!(
            "provide --indexer <full Torznab URL>, set TPB_INDEXERS, or add --shodan with --shodan-query"
        );
    }

    let query = args.query.join(" ");
    let api_key = args.api_key.as_deref();

    let (torrents, failures) = search::search_all(
        &client,
        &sources,
        &query,
        args.per_source_limit,
        api_key,
        args.fanout,
        discovery.concurrency,
    )
    .await;

    if discovery.verbose {
        for failure in &failures {
            eprintln!("warning: {failure:#}");
        }
    }

    let mut torrents = condense_by_hash(torrents);
    torrents.sort_by(|a, b| {
        b.seeders
            .cmp(&a.seeders)
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.title.cmp(&b.title))
    });
    torrents.truncate(args.limit);

    if torrents.is_empty() {
        if !args.no_cache
            && let Some(cached) = cache::load_cached_search(&query)
        {
            eprintln!(
                "warning: live indexers are unavailable; showing a cached result from the last {} minutes",
                cache::SEARCH_CACHE_TTL.as_secs() / 60
            );
            if args.json {
                println!("{}", serde_json::to_string_pretty(&cached)?);
            } else {
                render::print_torrents(&cached, color);
            }
            return Ok(());
        }
        if failures.is_empty() {
            bail!("no results found for: {query}");
        }
        bail!("no indexer returned results for: {query} (use --verbose for request failures)");
    }

    if !args.no_cache
        && let Err(error) = cache::save_cached_search(&query, &torrents)
        && discovery.verbose
    {
        eprintln!("warning: could not save search cache: {error:#}");
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&torrents)?);
    } else {
        render::print_torrents(&torrents, color);
    }
    Ok(())
}

fn run_cache(args: cli::CacheArgs) -> Result<()> {
    match args.command {
        CacheCommand::Clear { include_indexers } => {
            let removed = cache::clear(include_indexers)?;
            if removed.is_empty() {
                println!("Nothing to clear.");
            } else {
                for path in removed {
                    println!("Removed {}", path.display());
                }
            }
            Ok(())
        }
    }
}

fn announce_saved_endpoints(results: &[discover::DiscoveryResult]) -> Result<()> {
    let endpoints: Vec<String> = results
        .iter()
        .map(|result| result.endpoint.clone())
        .collect();
    if let Some(path) = cache::save_discovered_sources(&endpoints)? {
        eprintln!(
            "Saved {} verified endpoint(s) to {}",
            endpoints.len(),
            path.display()
        );
    }
    Ok(())
}
