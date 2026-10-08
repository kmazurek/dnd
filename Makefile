# Common development tasks for dnd.
# Run `make help` to list the available targets.

CARGO      ?= cargo
BIN        := dnd
PREFIX     ?= $(HOME)/.local
LINUX_TARGET ?= x86_64-unknown-linux-gnu

.DEFAULT_GOAL := help

.PHONY: help setup build release run test fmt fmt-check lint ra check check-linux ci install uninstall clean

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"} /^[a-zA-Z_-]+:.*##/ {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

setup: ## Install the Rust toolchain and components via rustup
	rustup toolchain install stable --profile minimal
	rustup component add rustfmt clippy rust-analyzer rust-src
	rustup target add $(LINUX_TARGET)

build: ## Build a debug binary
	$(CARGO) build

release: ## Build an optimized binary
	$(CARGO) build --release

run: ## Run with ARGS, e.g. make run ARGS="-x Cargo.toml"
	$(CARGO) run -- $(ARGS)

test: ## Run unit and integration tests
	$(CARGO) test

fmt: ## Format the code with rustfmt
	$(CARGO) fmt --all

fmt-check: ## Verify formatting without changing files
	$(CARGO) fmt --all -- --check

lint: ## Run clippy with warnings treated as errors
	$(CARGO) clippy --all-targets -- -D warnings

ra: ## Run rust-analyzer diagnostics over the project
	rust-analyzer diagnostics .

check: ## Type-check the crate for the host platform
	$(CARGO) check --all-targets

check-linux: ## Type-check the crate for Linux (cross-target, no linking)
	$(CARGO) check --target $(LINUX_TARGET)

ci: fmt-check lint ra test check-linux ## Run every verification step

install: release ## Install the binary into $(PREFIX)/bin
	install -d $(PREFIX)/bin
	install -m 755 target/release/$(BIN) $(PREFIX)/bin/$(BIN)

uninstall: ## Remove the installed binary
	rm -f $(PREFIX)/bin/$(BIN)

clean: ## Remove build artifacts
	$(CARGO) clean
