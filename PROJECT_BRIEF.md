# Project brief

## Requested goal

Create a Rust command-line application with the fast, terminal-first search
experience of the local `nyaa-cli` and `magnets-cli` projects. The requested
data-source direction is proxy-served public torrent metadata, with Shodan
considered as a way to locate candidate data services.

The intended user experience is:

- accept a search query as a simple positional command (for example,
  `tool "query"`);
- query one or more configured data sources;
- return normalized result metadata in a readable terminal format;
- offer structured JSON output for scripting;
- rank and de-duplicate equivalent results;
- deal gracefully with timeouts, unreachable sources, malformed responses, and
  empty results.

## Current workspace status

`tpb-cli` has not been initialized yet:

- no Git repository is present;
- no `Cargo.toml`, source files, tests, or README are present;
- this document is the first project artifact.

No source code has been changed in either reference project during this review.

## Reference project review

### `../nyaa-cli`

- Branch: `master`, tracking `origin/master`.
- Working tree: an untracked `target/` directory only.
- Shape: one synchronous Rust binary (`src/main.rs`) using `reqwest` blocking
  HTTP, `scraper`, `urlencoding`, and `colored`.
- Behaviour: fetches a single HTML search page, extracts table rows, sorts by
  seed count, and prints metadata plus a magnet or download link.
- Validation: `cargo test` passed (there are currently zero tests);
  `cargo clippy --all-targets -- -D warnings` passed.
- Formatting: `cargo fmt --check` reports formatting changes in `src/main.rs`.

What it contributes is the concise CLI interaction and result presentation.
Its direct HTML parsing is tightly coupled to one site's markup and has no
parser fixtures, CLI framework, structured output, caching, or concurrent
source handling.

### `../magnets-cli`

- Branch: `main`, tracking `origin/main`.
- Working tree: user changes in `README.md` and `src/main.rs`.
- Shape: an asynchronous Rust binary based on `clap`, `tokio`, `reqwest`,
  `quick-xml`, `serde`, and `url`.
- Behaviour: accepts explicitly configured Torznab endpoints, can discover and
  validate a narrowly defined class of public endpoints, executes concurrent
  searches, caches successful results, supports JSON, and prints color-aware
  terminal output.
- Current uncommitted change: raises the default combined result limit from 20
  to 40 and condenses results that share a BitTorrent info hash. For merged
  results, swarm statistics are averaged and the display reports the number of
  contributing sources.
- Validation: `cargo test` passed (5 tests); `cargo fmt --check` passed; and
  `cargo clippy --all-targets -- -D warnings` passed.

`magnets-cli` is the better engineering baseline: it has a normalized result
model, robust CLI parsing, bounded concurrency, timeouts, a source abstraction,
and tests around protocol parsing and result de-duplication.

## Local Shodan readiness

The locally installed `shodan` command is available. `shodan info` reports:

- query credits available: 199,993;
- scan credits available: 65,536.

This confirms that the CLI is authenticated and capable of making Shodan API
queries. It does not establish that any particular external data source is
stable, compatible, or suitable for the requested client.

## Feasibility assessment

The CLI itself is straightforward in Rust and can reuse the general patterns
already present in `magnets-cli`:

1. Parse the command line with `clap` and preserve the convenient bare-search
   syntax.
2. Model source-specific responses behind a common `Torrent`/search-result
   type.
3. Execute requests with bounded asynchronous concurrency, explicit timeouts,
   and safe redirect handling.
4. Normalize and de-duplicate results by a validated content identifier.
5. Support human-readable and JSON output, deterministic ranking, and explicit
   failure reporting.
6. Cover each response parser with local fixtures and test the public CLI
   contract.

The uncertain part is not the Rust implementation; it is the data-source
contract. Torznab supplies a documented capability check and a stable XML
response format, so it can be discovered and validated consistently. An
ordinary proxy-served web interface has no comparable common contract. In that
case, each independent layout or response format requires its own parser,
fixtures, health checks, and ongoing maintenance. Candidate services can also
disappear, redirect, rate-limit, or change markup without notice.

For a reliable first release, select a documented, authorized endpoint contract
and make all sources explicit configuration rather than treating discovery as a
guarantee of compatibility. Add an adapter only after its response format and
test fixtures are available.

## Proposed delivery milestones

1. Initialize the Rust binary, repository metadata, README, formatter, clippy,
   and a small test suite.
2. Implement the common result model, terminal renderer, JSON serialization,
   ranking, and de-duplication tests.
3. Implement one documented source adapter with fixture-driven parsing and
   endpoint configuration.
4. Add concurrent multi-source search, timeouts, retries where appropriate,
   cache policy, and source-health reporting.
5. Add discovery only where a source type has an explicit, stable,
   permissioned fingerprint and a capability validation step.

## Decisions needed before implementation

- Which authorized, documented endpoint type is the initial source contract?
- Is the first release read-only metadata search, or should it also integrate
  with a locally configured downloader?
- Which platforms and installation method should be supported?
- What retention, privacy, and cache-clearing behaviour should the CLI offer?

## 2026-07-23 update: direction changed to a TPB-specific decentralized client

After the first implementation (Torznab, per the recommendation above) shipped
and was reviewed, the requested direction changed explicitly: target The
Pirate Bay specifically rather than a generic Torznab indexer, use Shodan
discovery with a built-in default fingerprint (no explicit query required, for
parity with `magnets-cli`'s out-of-the-box `discover`), and treat "search
several independent mirrors concurrently with no hardcoded single host" as the
decentralization requirement (rather than a Mainline DHT/BEP5 client, which
was considered and explicitly ruled out as disproportionate scope).

The implementation was rebuilt around The Pirate Bay's public JSON search API
("apibay") and mirrors running the same software: it has a documented,
consistent response shape, so a discovered candidate can still be validated
before use the same way a Torznab `?t=caps` probe was used previously, without
depending on parsing an arbitrary proxy site's HTML (which was rejected for
the same fragility reasons the original feasibility assessment raised).
