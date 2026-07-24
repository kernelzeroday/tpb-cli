use clap::{Args, Parser, Subcommand};
use std::env;
use std::ffi::OsString;

#[derive(Parser)]
#[command(
    name = "tpb",
    version,
    about = "Search Torznab-compatible torrent indexers",
    long_about = "Search one or more Torznab endpoints (for example a self-hosted Bitmagnet, \
Jackett, or another compatible indexer) and print normalized, de-duplicated results.\n\
\n\
Pass one or more full Torznab endpoint URLs with --indexer, set TPB_INDEXERS to a \
comma-separated list, or use --shodan with an explicit --shodan-query to discover and \
verify candidate endpoints before search."
)]
pub struct Cli {
    /// Disable ANSI styling
    #[arg(long, global = true)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: CommandKind,
}

#[derive(Subcommand)]
pub enum CommandKind {
    /// Search configured Torznab endpoints concurrently
    Search(SearchArgs),
    /// Find and verify candidate Torznab endpoints using the local Shodan CLI
    Discover(DiscoverArgs),
    /// Manage locally cached search results and discovered endpoints
    Cache(CacheArgs),
}

#[derive(Args, Clone)]
pub struct DiscoveryOptions {
    /// Shodan query identifying a candidate service (repeatable); required for discovery
    #[arg(long = "shodan-query")]
    pub shodan_queries: Vec<String>,

    /// Maximum Shodan matches per query to inspect
    #[arg(long, default_value_t = 50)]
    pub shodan_limit: usize,

    /// Maximum simultaneous capability probes or search requests
    #[arg(long, default_value_t = 12)]
    pub concurrency: usize,

    /// HTTP timeout in seconds
    #[arg(long, default_value_t = 10)]
    pub timeout: u64,

    /// Show rejected candidates and request failures
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Args)]
pub struct DiscoverArgs {
    #[command(flatten)]
    pub discovery: DiscoveryOptions,

    /// Emit machine-readable JSON
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Args)]
pub struct SearchArgs {
    /// Search terms
    #[arg(required = true)]
    pub query: Vec<String>,

    /// Full Torznab endpoint URL; may be supplied more than once
    #[arg(short, long, visible_alias = "source")]
    pub indexer: Vec<String>,

    /// Discover candidate endpoints with Shodan before searching (requires --shodan-query)
    #[arg(long)]
    pub shodan: bool,

    #[command(flatten)]
    pub discovery: DiscoveryOptions,

    /// Torznab API key added as the `apikey` parameter
    #[arg(long, env = "TPB_API_KEY")]
    pub api_key: Option<String>,

    /// Maximum combined results to print
    #[arg(short = 'n', long, default_value_t = 40)]
    pub limit: usize,

    /// Concurrent endpoint count per fallback batch
    #[arg(long, default_value_t = 10)]
    pub fanout: usize,

    /// Number of results requested from each endpoint before combining them
    #[arg(long, default_value_t = 20)]
    pub per_source_limit: usize,

    /// Do not read or write the local search-result cache
    #[arg(long)]
    pub no_cache: bool,

    /// Emit machine-readable JSON
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Args)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub command: CacheCommand,
}

#[derive(Subcommand)]
pub enum CacheCommand {
    /// Remove cached search results
    Clear {
        /// Also remove endpoints saved by a previous `discover`
        #[arg(long)]
        include_indexers: bool,
    },
}

/// Treats an unrecognised first positional argument as a search query. This
/// keeps `tpb fedora` as convenient as the older one-site CLIs while
/// retaining explicit `search`, `discover`, and `cache` subcommands.
pub fn parse_cli() -> Cli {
    let mut arguments: Vec<OsString> = env::args_os().collect();
    let first = arguments.get(1).and_then(|value| value.to_str());
    let is_command = matches!(first, Some("search" | "discover" | "cache" | "help"));
    let is_global_option = first.is_some_and(|value| value.starts_with('-'));
    if first.is_some() && !is_command && !is_global_option {
        arguments.insert(1, OsString::from("search"));
    }
    Cli::parse_from(arguments)
}
