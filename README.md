# tpb

`tpb` is an asynchronous CLI client for the [Torznab](https://torznab.github.io/spec-1.3-draft/index.html) search API. Point it at a Torznab endpoint you have configured or otherwise trust, and it searches, normalizes, ranks, and de-duplicates the results.

It does not download torrent content. It prints the metadata and magnet or download link returned by the indexer.

See [`PROJECT_BRIEF.md`](PROJECT_BRIEF.md) for the design rationale, including why an ordinary proxy-served torrent-site HTML interface was rejected in favor of Torznab as the first supported source contract.

## Install

```bash
cargo install --path .
```

## Search configured indexers

Pass the complete Torznab endpoint, not a web UI URL:

```bash
tpb search "Ubuntu 24.04" --indexer http://localhost:3333/torznab
```

`search` is optional for a simple query:

```bash
tpb "Ubuntu 24.04" --indexer http://localhost:3333/torznab
```

Use several endpoints concurrently:

```bash
tpb search "Fedora Workstation" \
  -i http://localhost:3333/torznab \
  -i http://localhost:9117/api/v2.0/indexers/all/results/torznab/api \
  --api-key "$JACKETT_API_KEY"
```

`TPB_INDEXERS` may hold a comma-separated list of endpoint URLs, and `TPB_API_KEY` supplies a default `--api-key`, so neither has to be repeated per invocation.

Results that share a BitTorrent info hash are condensed into one entry; seed and leech counts from multiple indexers are averaged and labelled `avg`, and the entry lists how many indexers returned that hash. Combined output defaults to 40 results (`-n`/`--limit`); `--per-source-limit` bounds the request size sent to each endpoint, and `--fanout` bounds how many endpoints are queried concurrently in each fallback batch.

Successful searches are cached for 15 minutes under `~/.cache/tpb/searches` (or `$XDG_CACHE_HOME/tpb/searches`). If every configured indexer is unreachable or times out, the most recent matching cached result is shown instead, with a warning on stderr. Pass `--no-cache` to skip both reading and writing this cache. Clear it at any time:

```bash
tpb cache clear
```

## Discover candidate endpoints with Shodan

`discover` shells out to a locally configured [`shodan` CLI](https://cli.shodan.io/) to search for services matching a fingerprint you supply, then probes every candidate concurrently with `?t=caps`. Only responses that are genuine Torznab capability documents are kept; nothing else from a Shodan result is ever trusted as an indexer. There is no built-in default fingerprint — you must supply one with `--shodan-query`, since baking in a single piece of software's signature would misrepresent it as the only or the recommended option:

```bash
tpb discover --shodan-query 'http.title:"some-torznab-frontend"'
```

Verified endpoints are written to `~/.config/tpb/indexers` (or `$XDG_CONFIG_HOME/tpb/indexers`) and are used automatically by a bare `search` once `--indexer` and `TPB_INDEXERS` are both unset. Add `--shodan` to a search to refresh discovery first:

```bash
tpb search "ubuntu 24.04" --shodan --shodan-query 'http.title:"some-torznab-frontend"' -n 10
```

Use `--verbose` to see rejected candidates and per-source request failures, `--shodan-limit` to change the candidate count per query, `--concurrency` to bound parallel HTTP requests, and `--json` for machine-readable output.

The Shodan scan is opt-in (`--shodan` for search, or the dedicated `discover` command). For controlled or authenticated deployments, prefer an explicit `--indexer` endpoint instead.

## JSON output

Every command accepts `--json` for scripting:

```bash
tpb search "debian 12" --indexer http://localhost:3333/torznab --json | jq '.[0].magnet'
```

## Development

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Torznab response parsing is covered by fixture files under `tests/fixtures/`, covering a normal result feed, an empty feed, a Torznab `<error>` document, malformed XML, and a capability document.
