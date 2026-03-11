# hitt

`hitt` is a fast terminal-native API client written in Rust.

It combines an interactive TUI with a scriptable CLI for HTTP workflows, collections, environments, request chaining, assertions, response diffing, load testing, and real-time protocols such as WebSocket and Server-Sent Events.

The goal is simple: bring a Postman-like workflow to the terminal without giving up speed, keyboard-first navigation, or developer-friendly plain-text storage.

## Why hitt?

- Terminal-first workflow with both `TUI` and `CLI` modes.
- Git-friendly collections and environments stored as readable JSON/TOML.
- Built-in imports from cURL, Postman, OpenAPI, HAR, dotenv, and chain YAML.
- Built-in assertions, response diffing, and load testing.
- Real-time protocol support for WebSocket and SSE.
- Written in Rust with a small release binary and a strong automated test suite.

## Current Status

`hitt` is already usable for day-to-day HTTP API work, especially if you want a keyboard-driven terminal tool with saved collections and workflow automation.

It is also an active work in progress. Some features are complete, some are partially implemented, and a few are present in the data model or UI but not fully wired into transport execution yet. This README explicitly calls out those gaps.

## Feature Highlights

### Core request workflow

- HTTP methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`, `TRACE`
- Headers, query params, cookies, auth, and request body editing
- Multi-tab TUI workflow
- Vim-style keyboard navigation plus mouse support
- Request timing and response metadata
- JSON-aware response rendering and pretty printing

### Collections, environments, and variables

- Saved collections and nested folders
- Request history
- Collection variables and environment variables
- Layered `.env` loading
- `{{variable}}` interpolation across URL, headers, body, and auth fields
- Dynamic variables such as `$timestamp`, `$guid`, `$randomInt`, and `$randomEmail`

### Workflow automation

- Request chains with step conditions and delays
- Response value extraction from JSONPath, headers, cookies, or status code
- Variable propagation between chain steps
- YAML import for chain definitions

### Validation and analysis

- Per-request assertions
- Status, header, body, JSONPath, size, and response time assertions
- JSON Schema validation
- Side-by-side response diffing
- Built-in load testing with latency percentiles (`p50`, `p90`, `p95`, `p99`)

### Real-time protocols

- WebSocket connect, send, receive, and disconnect
- SSE connect, stream, and disconnect
- gRPC proto parsing and service/method inspection

### Import/export ecosystem

- Import from cURL, Postman Collection v2.1, Postman Environment, OpenAPI 3.x, HAR, `.env`, and chain YAML
- Export to cURL, Markdown API docs, Postman Collection v2.1, and Postman Environment

### UX and developer ergonomics

- Built-in themes
- Fuzzy search across requests, collections, and history
- Help modal and command palette
- Clipboard integration for cURL and Markdown docs
- Plain-text config and storage paths that work well in Git

## Compatibility Matrix

### Protocols

| Protocol | Status | Notes |
| --- | --- | --- |
| HTTP | Implemented | Main transport path for saved and ad-hoc requests |
| WebSocket | Implemented | TUI + CLI support |
| SSE | Implemented | TUI + CLI support |
| gRPC | Partial | `.proto` parsing and inspection are implemented; RPC execution is not yet wired |

### Request body execution

| Body type | Status | Notes |
| --- | --- | --- |
| JSON | Implemented | Executed by the HTTP client |
| Raw text | Implemented | Custom content type supported |
| `application/x-www-form-urlencoded` | Implemented | Executed by the HTTP client |
| GraphQL | Implemented | Sent as JSON envelope with `query` and optional `variables` |
| Multipart form-data | Partial | Present in the model/UI, not yet sent by the HTTP client |
| Binary file body | Partial | Present in the model/UI, not yet sent by the HTTP client |
| Protobuf body | Partial | Present in the model/UI, not yet sent by the HTTP client |

### Auth

| Auth type | Status | Notes |
| --- | --- | --- |
| Bearer | Implemented | Executed by the HTTP client |
| Basic | Implemented | Executed by the HTTP client |
| API key | Implemented | Header/query placement supported |
| OAuth2 metadata | Partial | Import/export and stored config exist; runtime token acquisition flow is not complete |

### Import/export interoperability

| Format | Import | Export | Status |
| --- | --- | --- | --- |
| cURL | Yes | Yes | Implemented |
| Postman Collection v2.1 | Yes | Yes | Implemented |
| Postman Environment | Yes | Yes | Implemented |
| OpenAPI 3.x | Yes | No | Import implemented, export planned |
| HAR | Yes | No | Implemented |
| `.env` | Yes | No | Implemented |
| Chain YAML | Yes | No | Implemented |
| Markdown API docs | No | Yes | Implemented |

## What Is Implemented vs What Is Missing

### Implemented today

- Interactive TUI and scriptable CLI
- Collections, folders, request history, environments, and variables
- Postman/OpenAPI/HAR/cURL/dotenv import paths
- Postman/cURL/Markdown export paths
- Assertions, response diffing, and load testing
- WebSocket and SSE workflows

### Partial or in-progress

- gRPC execution
- Multipart, binary, and protobuf request body execution
- Full OAuth2 runtime token acquisition flow
- Dedicated UI panels for some advanced features like load test results and protocol-specific views

### Planned / not yet available

- Proxy inspector with real MITM capture
- OpenAPI export
- Collection runner / batch assertion reporting
- HTTP/2 and HTTP/3 support
- Plugin system

## Installation

### Prerequisites

- Rust `1.70+` is documented in the contributor guide
- `cargo`

### Build from source

```bash
git clone <your-fork-or-this-repo>
cd hitt
cargo build --release
```

### Install locally

```bash
cargo install --path .
```

Or use the provided Make target:

```bash
make install
```

## Quick Start

### Launch the TUI

```bash
cargo run
```

### Start with a URL

```bash
cargo run -- --url https://httpbin.org/get
```

### Run in CLI mode

```bash
cargo run -- send GET https://httpbin.org/get
```

### Open a collection or import a file

```bash
cargo run -- --collection path/to/collection.json
cargo run -- --import tests/fixtures/petstore_openapi.yaml
```

### Load environments

```bash
cargo run -- --environment path/to/environment.json
cargo run -- --env .env --env .env.local
```

## CLI Examples

### Send an ad-hoc request

```bash
hitt send GET https://httpbin.org/get
```

### Send a request with headers and auth

```bash
hitt send POST https://httpbin.org/post \
  -H "Content-Type: application/json" \
  -H "X-Trace-Id: demo-123" \
  --auth-bearer my-token \
  --body '{"hello":"world"}'
```

### List saved collections

```bash
hitt collections
```

### Run a saved request

```bash
hitt run "Get Users" --collection "Demo API"
```

### Work with chains

```bash
hitt chain list --collection "Demo API"
hitt chain run "User Signup Flow" --collection "Demo API"
```

### Connect to WebSocket or SSE endpoints

```bash
hitt ws wss://echo.websocket.events --message "hello"
hitt sse https://example.com/events --duration 10 --max-events 5
```

## TUI Commands

The command palette exposes a large part of the workflow directly inside the TUI. Some useful examples:

- `:import` / `:export`
- `:newcol`, `:delcol`, `:save`
- `:newenv`, `:env`, `:dupenv`, `:env-file`
- `:chain`, `:newchain`, `:addstep`, `:importchain`
- `:diff`, `:loadtest`, `:docs`, `:theme`
- `:ws`, `:sse`, `:ws-disconnect`, `:sse-disconnect`

For a larger command list and keybindings, see `ROADMAP.md`, `CONTRIBUTING.md`, and the in-app help modal.

## Storage and Project Layout

`hitt` stores config and data in OS-appropriate directories.

| Platform | Config/data path |
| --- | --- |
| Linux | `~/.config/hitt/` and `~/.local/share/hitt/` |
| macOS | `~/Library/Application Support/hitt/` |
| Windows | `%APPDATA%\hitt\` |

Collections and environments are stored in readable JSON files, and config is stored in TOML.

Useful docs in this repository:

- `docs/ARCHITECTURE.md`
- `docs/IMPORTERS_EXPORTERS.md`
- `docs/TESTING.md`
- `docs/PROTOCOLOS.md`
- `docs/ALMACENAMIENTO.md`
- `CONTRIBUTING.md`

## Benchmarks

This repository includes reproducible Criterion benchmarks, profiling helpers, and comparison scripts.

### Benchmark methodology

The benchmark suite intentionally separates microbenchmarks from real network benchmarks:

- `benches/core_bench.rs` measures local code paths such as variable resolution, cURL parsing/export, assertion execution, serialization, and OpenAPI import.
- `benches/http_bench.rs` measures real HTTP requests against `https://httpbin.org`, which means results include network variability.
- `scripts/profile.sh` focuses on binary size, single-request timing, and simple sequential throughput.
- `scripts/compare.sh` compares `hitt` against tools such as `curl`, `xh`, `httpie`, and `wget` when they are available locally.

### Commands to reproduce

```bash
# Core Criterion benchmarks
cargo bench --bench core_bench

# Real-network HTTP Criterion benchmarks
cargo bench --bench http_bench

# Run all benchmarks
cargo bench

# Open the Criterion HTML report after a benchmark run
make bench-report

# Heap profiling
make profile-memory

# CPU flamegraph
make flamegraph

# End-to-end comparison scripts
make profile
make compare
```

### Local benchmark snapshot

The following numbers were measured locally on the repository state documented by this README using:

- `cargo 1.94.0`
- `rustc 1.94.0`
- macOS (`darwin`)

Core Criterion results:

| Benchmark | Result |
| --- | --- |
| Variable resolution, no variables | `35.494 ns - 39.402 ns` |
| Variable resolution, single variable | `266.86 ns - 269.89 ns` |
| Variable resolution, 50 vars / 3 used | `478.11 ns - 484.60 ns` |
| Variable resolution, deep lookup | `231.52 ns - 233.05 ns` |
| Resolve 10 headers | `654.93 ns - 660.69 ns` |
| cURL parse, simple GET | `1.1848 us - 1.2372 us` |
| cURL parse, complex POST | `2.6581 us - 2.6906 us` |
| Export request to cURL | `1.1395 us - 1.1481 us` |
| Single status assertion | `84.924 ns - 102.62 ns` |
| Mixed assertion suite (8 checks) | `7.7889 us - 9.1280 us` |
| Serialize collection with 50 requests | `37.023 us - 37.745 us` |
| Deserialize collection with 50 requests | `62.221 us - 62.736 us` |
| OpenAPI import (`petstore`) | `42.148 us - 42.654 us` |

HTTP Criterion results against `httpbin.org`:

| Benchmark | Result |
| --- | --- |
| GET request | `92.818 ms - 121.56 ms` |
| POST JSON request | `92.281 ms - 123.12 ms` |
| GET request with variable resolution | `84.664 ms - 112.91 ms` |

Additional local notes:

- Release binary size observed locally: `7.5M` (`target/release/hitt`)
- Criterion warned that the HTTP benchmarks could not complete 20 samples within the default measurement window, which is expected for real-network tests
- For stable comparisons, prefer `core_bench` when tracking regressions and treat `http_bench` as an end-to-end network snapshot

## Quality Signals

- MIT license declared in `Cargo.toml`
- `300` tests currently passing locally (`cargo test`)
- Dedicated architecture, protocol, storage, import/export, and testing docs
- Criterion benchmarks and profiling scripts included in the repo

## Contributing

Contributions are welcome.

Start with:

- `CONTRIBUTING.md`
- `docs/ARCHITECTURE.md`

Common commands:

```bash
cargo test
cargo clippy -- -W clippy::all
cargo fmt -- --check
```

## License

MIT
