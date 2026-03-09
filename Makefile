.PHONY: build install uninstall test lint clean run help bench bench-core bench-http bench-report profile profile-memory flamegraph flamegraph-install compare

BINARY := hitt
INSTALL_DIR := $(HOME)/.cargo/bin

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

build: ## Build in release mode
	cargo build --release

install: build ## Install hitt globally (~/.cargo/bin)
	@cp target/release/$(BINARY) $(INSTALL_DIR)/$(BINARY)
	@chmod +x $(INSTALL_DIR)/$(BINARY)
	@echo "Installed $(BINARY) to $(INSTALL_DIR)/$(BINARY)"
	@echo "Version: $$($(INSTALL_DIR)/$(BINARY) --version)"

uninstall: ## Remove hitt from ~/.cargo/bin
	@rm -f $(INSTALL_DIR)/$(BINARY)
	@echo "Removed $(BINARY) from $(INSTALL_DIR)"

test: ## Run all tests
	cargo test

lint: ## Run clippy and check formatting
	cargo clippy -- -W clippy::all
	cargo fmt -- --check

clean: ## Clean build artifacts
	cargo clean

run: ## Run in dev mode (TUI)
	cargo run

bench: ## Run all benchmarks (core + HTTP)
	cargo bench

bench-core: ## Run core benchmarks (variables, curl, assertions, serialization)
	cargo bench --bench core_bench

bench-http: ## Run HTTP benchmarks (real network requests)
	cargo bench --bench http_bench

profile: build ## Run full performance profile (memory, CPU, hitt vs curl)
	@./scripts/profile.sh

bench-report: ## Open Criterion HTML report in browser
	@open target/criterion/report/index.html 2>/dev/null || echo "No report found. Run 'make bench' first."

profile-memory: ## Heap profile with dhat (generates dhat-heap.json)
	cargo run --example profile_memory --features dhat-heap --release
	@echo ""
	@echo "Open the viewer: https://nnethercote.github.io/dh_view/dh_view.html"
	@echo "Load file: dhat-heap.json"

flamegraph-install: ## Install cargo-flamegraph
	cargo install flamegraph

flamegraph: ## Generate CPU flamegraph (requires cargo-flamegraph)
	cargo flamegraph --profile release-with-debug --bin hitt -- send GET https://httpbin.org/get
	@echo "Flamegraph saved to flamegraph.svg"
	@open flamegraph.svg 2>/dev/null || echo "Open flamegraph.svg in your browser"

compare: build ## Compare hitt vs curl, xh, httpie, wget (speed, memory, load)
	@./scripts/compare.sh
