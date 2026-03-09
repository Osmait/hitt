# ReqTUI — Roadmap & Progress

## Phase 1 — Core TUI & HTTP (Weeks 1-3)
- [x] Basic TUI layout with ratatui (sidebar + main panel + statusbar)
- [x] URL bar with method selector (GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS)
- [x] Send HTTP requests with reqwest
- [x] Display response body with JSON syntax highlighting
- [x] Status bar (status code, time, size, content-type)
- [x] Keyboard navigation between panels (vim-style)
- [x] Multi-tab support (t, w, Alt+1..9)

## Phase 2 — Request Building (Weeks 4-5)
- [x] Headers tab (add/edit/toggle key-value pairs)
- [x] Query params tab
- [x] Request body editor (JSON, form-data, urlencoded, raw, binary, GraphQL)
- [x] Auth tab (Bearer, Basic, API Key)
- [x] OAuth2 flow (client credentials, authorization code)
- [x] Content-Type auto-detection
- [x] Cookie jar (persistent cookies per collection)

## Phase 3 — Collections & Storage (Weeks 6-7)
- [x] Collection tree in sidebar (create, rename, delete)
- [x] Folder nesting with auth inheritance
- [x] Save/load collections from disk (git-friendly pretty JSON)
- [x] Request history with sled (searchable, timestamped)
- [x] Collection-level variables
- [x] Config file (~/.config/reqtui/config.toml)
- [ ] Reorder / drag collections and requests

## Phase 4 — Environments & Variables (Week 8)
- [x] Environment manager (create, edit, switch)
- [x] {{variable}} interpolation in URL, headers, body, auth fields
- [x] Dynamic variables ($timestamp, $guid, $randomInt, $randomEmail)
- [ ] Variable quick-view (preview resolved values inline)
- [x] .env file support (--env flag, layered loading)
- [ ] Duplicate environment

## Phase 5 — Postman Compatibility (Weeks 9-10)
- [x] Import Postman Collection v2.1 (.postman_collection.json)
- [x] Import Postman Environment (.postman_environment.json)
- [x] Export to Postman Collection v2.1
- [x] Export Postman Environment
- [ ] Handle v2.0 → v2.1 auto-upgrade on import
- [x] Preserve folder hierarchy, auth, variables, descriptions

## Phase 6 — Import/Export Ecosystem (Weeks 11-12)
- [x] cURL import (parse from clipboard or command mode)
- [x] cURL export (generate from any request)
- [x] OpenAPI/Swagger 3.x import → auto-generate collection
- [x] HAR file import
- [x] Markdown API documentation generator
- [ ] Export to OpenAPI spec from collection

## Phase 7 — Request Chaining & Workflows (Weeks 13-14)
- [ ] Chain editor UI (visual step flow)
- [x] Value extraction from responses (JSONPath, headers, cookies)
- [x] Variable propagation between chain steps
- [x] Conditional step execution (status checks, variable checks)
- [x] Step delays
- [x] Chain save/load in collections
- [ ] Chain execution with live progress display
- [ ] `:chain` / `:chain run` commands

## Phase 8 — Declarative Testing (Weeks 15-16)
- [x] Assertion editor per request (UI panel)
- [x] Status assertions (equals, range)
- [x] Body assertions (JSONPath exists, equals, type, contains)
- [x] Header assertions (exists, equals)
- [x] Performance assertions (response time, size)
- [x] JSON Schema validation
- [x] Assertion results display (pass/fail with actual vs expected)
- [ ] Run all assertions in collection (batch test report / collection runner)

## Phase 9 — Real-Time Protocols (Weeks 17-20)

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
- [ ] Dedicated `sse_panel.rs` UI panel
- [ ] Accumulated text view (for AI API streaming)

### gRPC
- [x] Load .proto files (protox parser)
- [x] List services and methods
- [ ] Unary RPC calls
- [ ] Server streaming
- [ ] Client streaming
- [ ] Bidirectional streaming
- [ ] Auto-generate request body from message type
- [ ] Dedicated `grpc_panel.rs` UI panel

## Phase 10 — Advanced Features (Weeks 21-24)
- [x] Response diffing — side-by-side comparison engine
- [x] Load testing — N requests at C concurrency, latency stats (p50/p90/p95/p99)
- [ ] Load test results UI panel (`loadtest_panel.rs`)
- [ ] `:diff` command to diff two responses
- [ ] Proxy inspector — local MITM proxy (hudsucker)
- [ ] Proxy UI panel (`proxy_panel.rs`)
- [ ] Proxy → Collection — save captured requests directly
- [ ] `:proxy [port]` command
- [ ] Auth templates (OAuth2, JWT refresh, API key rotation)
- [x] Theme system — bundled themes (Catppuccin, Dracula, Gruvbox, Tokyo Night)
- [ ] Theme loading from .toml files (only catppuccin.toml exists)
- [x] Fuzzy search — across all requests, collections, history
- [ ] Response body search (regex search within response)
- [x] Copy as curl — one-key copy
- [x] Request timing waterfall (DNS, TCP, TLS, TTFB, download)
- [x] Resize-responsive layouts
- [x] Help modal — keybinding reference

## Missing Commands
- [ ] `:chain <name>` — Open chain editor
- [ ] `:chain run` — Execute current chain
- [ ] `:proxy [port]` — Start/stop proxy inspector
- [ ] `:diff <id1> <id2>` — Diff two responses
- [ ] `:set <key> <val>` — Change setting at runtime
- [ ] `:env-file <path>` — Load .env file from command

## Missing Keybindings (Ctrl+ combos from plan)
- [ ] `Ctrl+R` — Send request (alternative)
- [ ] `Ctrl+S` — Save current request
- [ ] `Ctrl+P` — Fuzzy search requests
- [ ] `Ctrl+E` — Switch environment
- [ ] `Ctrl+N` — New request
- [ ] `Ctrl+I` — Import
- [ ] `Ctrl+X` — Export

## Infrastructure
- [ ] `/tests/` directory with integration tests
- [ ] `postman_import_test.rs`
- [ ] `postman_export_test.rs`
- [ ] `curl_parser_test.rs`
- [ ] `openapi_import_test.rs`
- [ ] `chain_execution_test.rs`
- [ ] `assertion_engine_test.rs`
- [ ] `variable_interpolation_test.rs`
- [ ] Test fixtures (sample collection, environment, OpenAPI spec, HAR)
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
