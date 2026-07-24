# tpb

`tpb` is an asynchronous, decentralized CLI client for The Pirate Bay's public JSON search API (widely known as "apibay") and the mirrors that run the same software. It searches several independent mirrors concurrently, normalizes, ranks, and de-duplicates the results, so no single mirror going down, rate-limiting, or disappearing stops a search.

It does not download torrent content. It prints the metadata and magnet link returned by each mirror.

See [`PROJECT_BRIEF.md`](PROJECT_BRIEF.md) for the design history, including why an earlier direction (Torznab) was replaced with this one.

## Install

```bash
cargo install --path .
```

## Search configured mirrors

Pass the base URL of a mirror (not a search results page URL):

```bash
tpb search "Ubuntu 24.04" --proxy https://apibay.org
```

`search` is optional for a simple query, and works with zero configuration:

```bash
tpb "Ubuntu 24.04"
```

With no `--proxy`, no `TPB_PROXIES`, and nothing yet saved by `discover`, a
bare search falls back to a small built-in list of known-working mirrors
(currently just `apibay.org`, the canonical host) before ever needing to
touch Shodan. `--proxy`/`TPB_PROXIES` still take priority when set.

Use several mirrors concurrently, so no one of them is a single point of failure:

```bash
tpb search "Fedora Workstation" \
  -p https://apibay.org \
  -p https://mirror.example/
```

`TPB_PROXIES` may hold a comma-separated list of mirror base URLs, so it doesn't have to be repeated per invocation.

Results that share a BitTorrent info hash are condensed into one entry; seed and leech counts from multiple mirrors are averaged and labelled `avg`, and the entry lists how many mirrors returned that hash. Combined output defaults to 40 results (`-n`/`--limit`); `--fanout` bounds how many mirrors are queried concurrently in each fallback batch — later batches are only tried if an earlier one returns nothing.

Successful searches are cached for 15 minutes under `~/.cache/tpb/searches` (or `$XDG_CACHE_HOME/tpb/searches`). If every configured mirror is unreachable or times out, the most recent matching cached result is shown instead, with a warning on stderr. Pass `--no-cache` to skip both reading and writing this cache. Clear it at any time:

```bash
tpb cache clear
```

## Discover mirrors with Shodan

`discover` shells out to a locally configured [`shodan` CLI](https://cli.shodan.io/) to search for services matching a fingerprint, then probes every candidate concurrently with a real search request. Only responses that actually deserialize into this API's expected shape are kept; nothing else from a Shodan result is ever trusted as a mirror. A broad default query (`apibay`) is built in so discovery works with zero flags, but you can narrow or replace it:

```bash
tpb discover
tpb discover --shodan-query 'http.html:"apibay"'
```

Verified endpoints are written to `~/.config/tpb/proxies` (or `$XDG_CONFIG_HOME/tpb/proxies`) and are used automatically by a bare `search` once `--proxy` and `TPB_PROXIES` are both unset. Add `--shodan` to a search to refresh discovery first:

```bash
tpb search "ubuntu 24.04" --shodan -n 10
```

`discover` writing multiple mirrors to that cache, and `search` fanning out across all of them concurrently, is what makes this decentralized: it doesn't depend on one hardcoded host. In practice, independent mirrors of this exact API are not common or reliably fingerprintable on Shodan; the built-in known-mirror list above is the main reason zero-config search works, and `discover`/`--shodan` exist to find more if and when they're out there.

Use `--verbose` to see rejected candidates and per-mirror request failures, `--shodan-limit` to change the candidate count per query, `--concurrency` to bound parallel HTTP requests, and `--json` for machine-readable output.

The Shodan scan is opt-in (`--shodan` for search, or the dedicated `discover` command). For controlled deployments, prefer explicit `--proxy` endpoints instead.

## JSON output

Every command accepts `--json` for scripting:

```bash
tpb search "debian 12" --proxy https://apibay.org --json | jq '.[0].magnet'
```

## Development

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

API response parsing is covered by fixture files under `tests/fixtures/`: a normal result set, the API's no-results sentinel row, and a response that doesn't match the expected shape at all.
