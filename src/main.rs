//! `tpb`: a terminal-first, decentralized search client for The Pirate
//! Bay's public API and its mirrors. See `PROJECT_BRIEF.md` for the design
//! history, including why this targets that API rather than a generic
//! Torznab indexer: no single mirror is queried by default, and Shodan
//! discovery plus concurrent multi-mirror search means one mirror going
//! down does not stop a search.

mod cache;
mod cli;
mod discover;
mod proxy;
mod render;
mod search;
mod source;
mod torrent;

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
    if !(1..=100).contains(&args.fanout) {
        bail!("--fanout must be between 1 and 100");
    }

    let client = search::build_client(discovery.timeout_secs)?;
    let mut sources = configured_sources(&args.proxy)?;

    // No --proxy, no TPB_PROXIES, and nothing saved from a previous
    // `discover` yet: bootstrap via Shodan automatically so a bare `tpb
    // <query>` works with zero configuration, the same way `--shodan` does
    // explicitly. Once this succeeds, the discovered mirrors are cached, so
    // later bare searches use them without repeating a Shodan query.
    let auto_discovering = sources.is_empty() && !args.shodan;
    if args.shodan || auto_discovering {
        if auto_discovering {
            eprintln!(
                "no mirrors configured; discovering some via Shodan (pass --proxy or set TPB_PROXIES to skip this)"
            );
        }
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
            "provide --proxy <mirror base URL>, set TPB_PROXIES, or add --shodan to discover mirrors"
        );
    }

    let query = args.query.join(" ");

    let (torrents, failures) = search::search_all(
        &client,
        &sources,
        &query,
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
                "warning: live mirrors are unavailable; showing a cached result from the last {} minutes",
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
        bail!("no mirror returned results for: {query} (use --verbose for request failures)");
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
        CacheCommand::Clear { include_proxies } => {
            let removed = cache::clear(include_proxies)?;
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
