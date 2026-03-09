# hitt — Roadmap & Progress

## Phase 1 — Core TUI & HTTP
- [x] Basic TUI layout with ratatui (sidebar + main panel + statusbar)
- [x] URL bar with method selector (GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS)
- [x] Send HTTP requests with reqwest
- [x] Display response body with JSON syntax highlighting
- [x] Status bar (status code, time, size, content-type)
- [x] Keyboard navigation between panels (vim-style)
- [x] Multi-tab support (t, w, Alt+1..9)

## Phase 2 — Request Building
- [x] Headers tab (add/edit/toggle key-value pairs)
- [x] Query params tab
- [x] Request body editor (JSON, form-data, urlencoded, raw, binary, GraphQL)
- [x] Auth tab (Bearer, Basic, API Key)
- [x] OAuth2 flow (client credentials, authorization code)
- [x] Content-Type auto-detection
- [x] Cookie jar (persistent cookies per collection)

## Phase 3 — Collections & Storage
- [x] Collection tree in sidebar (create, rename, delete)
- [x] Folder nesting with auth inheritance
- [x] Save/load collections from disk (git-friendly pretty JSON)
- [x] Request history (in-memory HistoryStore, searchable, timestamped)
- [x] Collection-level variables
- [x] Config file (~/.config/hitt/config.toml)
- [ ] Reorder / drag collections and requests

## Phase 4 — Environments & Variables
- [x] Environment manager (create, edit, switch)
- [x] {{variable}} interpolation in URL, headers, body, auth fields
- [x] Dynamic variables ($timestamp, $guid, $randomInt, $randomEmail)
- [ ] Variable quick-view (preview resolved values inline)
- [x] .env file support (--env flag, layered loading)
- [x] Duplicate environment (`:dupenv` command)

## Phase 5 — Postman Compatibility
- [x] Import Postman Collection v2.1 (.postman_collection.json)
- [x] Import Postman Environment (.postman_environment.json)
- [x] Export to Postman Collection v2.1
- [x] Export Postman Environment
- [ ] Handle v2.0 → v2.1 auto-upgrade on import
- [x] Preserve folder hierarchy, auth, variables, descriptions

## Phase 6 — Import/Export Ecosystem
- [x] cURL import (parse from clipboard or command mode)
- [x] cURL export (generate from any request)
- [x] OpenAPI/Swagger 3.x import → auto-generate collection
- [x] HAR file import
- [x] Markdown API documentation generator
- [ ] Export to OpenAPI spec from collection

## Phase 7 — Request Chaining & Workflows
- [ ] Chain editor UI (visual step flow)
- [x] Value extraction from responses (JSONPath, headers, cookies)
- [x] Variable propagation between chain steps
- [x] Conditional step execution (status checks, variable checks)
- [x] Step delays
- [x] Chain save/load in collections
- [x] Chain execution with live progress display (`render_chain_runner()`)
- [x] `:chain <name>` command (finds and executes chain)

## Phase 8 — Declarative Testing
- [x] Assertion editor per request (UI panel)
- [x] Status assertions (equals, range)
- [x] Body assertions (JSONPath exists, equals, type, contains)
- [x] Header assertions (exists, equals)
- [x] Performance assertions (response time, size)
- [x] JSON Schema validation
- [x] Assertion results display (pass/fail with actual vs expected)
- [ ] Run all assertions in collection (batch test report / collection runner)

## Phase 9 — Real-Time Protocols

### WebSocket
- [x] Connect/disconnect with custom headers
- [x] Send text/binary messages
- [x] Live message stream with timestamps + direction indicators
- [x] Auto-reconnect with backoff
- [x] Ping/pong support
- [x] Message history
- [ ] Dedicated `ws_panel.rs` interactive UI panel
- [ ] Message search within WebSocket session

### Server-Sent Events (SSE)
- [x] Connect to SSE endpoints
- [x] Display event stream with type, data, id
- [x] Auto-reconnect with Last-Event-ID
- [x] Accumulated text view (toggle with `a` key, for AI API streaming)
- [ ] Dedicated `sse_panel.rs` UI panel

### gRPC
- [x] Load .proto files (protox parser)
- [x] List services and methods
- [ ] Unary RPC calls
- [ ] Server streaming
- [ ] Client streaming
- [ ] Bidirectional streaming
- [ ] Auto-generate request body from message type
- [ ] Dedicated `grpc_panel.rs` UI panel

## Phase 10 — Advanced Features
- [x] Response diffing — side-by-side comparison engine
- [x] Load testing — N requests at C concurrency, latency stats (p50/p90/p95/p99)
- [ ] Load test results UI panel (`loadtest_panel.rs`)
- [x] `:diff` command — opens diff selector modal
- [ ] Proxy inspector — local MITM proxy (hudsucker)
- [ ] Proxy UI panel (`proxy_panel.rs`)
- [ ] Proxy → Collection — save captured requests directly
- [ ] `:proxy [port]` command
- [ ] Auth templates (OAuth2, JWT refresh, API key rotation)
- [x] Theme system — bundled themes (Catppuccin, Dracula, Gruvbox, Tokyo Night)
- [ ] Theme loading from custom .toml files
- [x] Fuzzy search — across all requests, collections, history
- [ ] Response body search (regex search within response)
- [x] Copy as curl — one-key copy
- [x] Request timing waterfall (DNS, TCP, TLS, TTFB, download)
- [x] Resize-responsive layouts
- [x] Help modal — keybinding reference

## Commands (all implemented)
- [x] `:chain <name>` — Execute a named chain
- [x] `:newchain <name>` — Create a new chain
- [x] `:addstep <request>` — Add step to chain
- [x] `:importchain <path>` — Import chain from YAML
- [x] `:diff` — Open diff selector
- [x] `:set <key> <val>` — Change setting at runtime
- [x] `:env-file <path>` — Load .env file
- [x] `:save` — Save current request
- [x] `:delreq` — Delete selected request
- [x] `:delcol [name]` — Delete collection
- [x] `:newcol <name>` — Create collection
- [x] `:newenv <name>` — Create environment
- [x] `:dupenv [name]` — Duplicate environment
- [x] `:env <name>` — Switch environment
- [x] `:import [path]` — Import file
- [x] `:export [path]` — Export file
- [x] `:curl` — Copy as curl
- [x] `:paste-curl` — Import from clipboard
- [x] `:ws <url>` — WebSocket connect
- [x] `:sse <url>` — SSE connect
- [x] `:ws-disconnect` / `:sse-disconnect` — Disconnect
- [x] `:loadtest <n> <c>` — Run load test
- [x] `:docs` — Copy markdown docs
- [x] `:theme <name>` — Switch theme
- [x] `:rename <name>` — Rename request
- [x] `:addvar <key> <val>` — Add collection variable
- [x] `:clearhistory` — Clear history
- [x] `:help` — Show help
- [x] `:q` / `:quit` — Quit
- [ ] `:proxy [port]` — Start/stop proxy inspector

## Keybindings (all implemented)
- [x] `Ctrl+R` — Send request
- [x] `Ctrl+S` — Save current request
- [x] `Ctrl+P` — Fuzzy search requests
- [x] `Ctrl+E` — Switch environment
- [x] `Ctrl+N` — New tab
- [x] `Ctrl+I` — Import
- [x] `Ctrl+X` — Export
- [x] `Ctrl+C` — Quit

## Infrastructure
- [x] `/tests/` directory with integration tests (300+ tests across 11 files)
- [x] `tests/importers.rs` — cURL, dotenv, HAR, OpenAPI, Postman import tests
- [x] `tests/exporters.rs` — cURL, Markdown, Postman export tests
- [x] `tests/postman_roundtrip.rs` — Postman import/export roundtrip
- [x] `tests/assertions.rs` — All assertion types tested
- [x] `tests/variables.rs` — Variable interpolation tests
- [x] `tests/event_commands.rs` — 126 tests for commands and key handlers
- [x] `tests/cli_commands.rs` — CLI mode tests
- [x] `tests/core_models.rs` — Domain model tests
- [x] `tests/storage.rs` — Storage layer tests
- [x] `tests/diff.rs` — Diff engine tests
- [x] Test fixtures (`tests/fixtures/` — OpenAPI, Postman, HAR, .env, chain YAML)
- [x] Architecture documentation (`docs/` — 7 files with Mermaid diagrams)
- [ ] Keychain / secure credential storage (`keychain.rs`)
- [ ] Syntect-based syntax highlighting (replace inline pretty_print)

## Future / Community
- [ ] Plugin system (Lua or WASM)
- [ ] GraphQL explorer with schema introspection
- [ ] Mock server (serve saved responses)
- [ ] Collaborative collections via Git sync
- [ ] Certificate pinning / custom CA
- [ ] HTTP/2 and HTTP/3 support
- [ ] Protobuf binary body inspector
- [ ] Request replay from history
- [ ] Collection runner (run all requests sequentially with reporting)
