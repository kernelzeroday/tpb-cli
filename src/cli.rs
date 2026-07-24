use clap::{Args, Parser, Subcommand};
use std::env;
use std::ffi::OsString;

#[derive(Parser)]
#[command(
    name = "tpb",
    version,
    about = "Search The Pirate Bay proxy mirrors, decentralized across several at once",
    long_about = "Search one or more Pirate Bay API mirrors concurrently and print normalized, \
de-duplicated results. No single mirror is a point of failure: pass several with --proxy, set \
TPB_PROXIES to a comma-separated list, or use --shodan to discover and verify mirrors first.\n\
\n\
This never queries a single hardcoded host; you configure or discover the mirrors it uses."
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
    /// Search configured Pirate Bay API mirrors concurrently
    Search(SearchArgs),
    /// Find and verify candidate mirrors using the local Shodan CLI
    Discover(DiscoverArgs),
    /// Manage locally cached search results and discovered endpoints
    Cache(CacheArgs),
}

#[derive(Args, Clone)]
pub struct DiscoveryOptions {
    /// Shodan query identifying a candidate mirror (repeatable); defaults to a broad fingerprint
    #[arg(long = "shodan-query")]
    pub shodan_queries: Vec<String>,

    /// Maximum Shodan matches per query to inspect
    #[arg(long, default_value_t = 50)]
    pub shodan_limit: usize,

    /// Maximum simultaneous validation probes or search requests
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

    /// Base URL of a Pirate Bay API mirror; may be supplied more than once
    #[arg(short, long, visible_alias = "source")]
    pub proxy: Vec<String>,

    /// Discover candidate mirrors with Shodan before searching
    #[arg(long)]
    pub shodan: bool,

    #[command(flatten)]
    pub discovery: DiscoveryOptions,

    /// Maximum combined results to print
    #[arg(short = 'n', long, default_value_t = 40)]
    pub limit: usize,

    /// Concurrent mirror count per fallback batch
    #[arg(long, default_value_t = 10)]
    pub fanout: usize,

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
        include_proxies: bool,
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
