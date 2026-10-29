# Common development tasks for dnd.
# Run `make help` to list the available targets.

CARGO      ?= cargo
RUSTC      ?= rustc
BIN        := dnd
PREFIX     ?= $(HOME)/.local
LINUX_TARGET ?= x86_64-unknown-linux-gnu
MACOS_TARGET ?= aarch64-apple-darwin
VERSION    := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
DIST       := dist

.DEFAULT_GOAL := help

.PHONY: help setup build release run test test-headless fmt fmt-check lint ra check check-linux ci dist-linux dist-macos checksums install uninstall clean

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"} /^[a-zA-Z_-]+:.*##/ {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

setup: ## Install the Rust toolchain and components via rustup
	rustup toolchain install stable --profile minimal
	rustup component add rustfmt clippy rust-analyzer rust-src
	rustup target add $(LINUX_TARGET) $(MACOS_TARGET)

build: ## Build a debug binary
	$(CARGO) build

release: ## Build an optimized binary
	$(CARGO) build --release

run: ## Run with ARGS, e.g. make run ARGS="-x Cargo.toml"
	$(CARGO) run -- $(ARGS)

test: ## Run unit, integration and GUI smoke tests
	$(CARGO) test

test-headless: ## Run tests that do not need a GUI session (e.g. over SSH)
	DND_SKIP_GUI_TESTS=1 $(CARGO) test

fmt: ## Format the code with rustfmt
	$(CARGO) fmt --all

fmt-check: ## Verify formatting without changing files
	$(CARGO) fmt --all -- --check

lint: ## Run clippy with warnings treated as errors
	$(CARGO) clippy --all-targets -- -D warnings

# rust-analyzer always reports `#[cfg]`-disabled code as WeakWarning hints;
# only real warnings and errors fail the target.
ra: ## Run rust-analyzer diagnostics over the project
	@out="$$(rust-analyzer diagnostics . 2>&1)"; \
	echo "$$out" | grep -v WeakWarning; \
	! echo "$$out" | grep -qE ': (Error|Warning) '

check: ## Type-check the crate for the host platform
	$(CARGO) check --all-targets

# `DOCS_RS` makes the gtk-rs `-sys` build scripts skip looking up the GTK
# libraries, so this also works on hosts without GTK installed. Nothing is
# linked, so the libraries are not needed.
check-linux: ## Type-check the crate for Linux (cross-target, no linking)
	DOCS_RS=1 $(CARGO) check --all-targets --target $(LINUX_TARGET)

ci: fmt-check lint ra test check-linux ## Run every verification step

# $(call package,TARGET): bundles the release binary for TARGET with the README
# into $(DIST)/dnd-vVERSION-TARGET.tar.gz.
define package
rm -rf $(DIST)/$(BIN)-v$(VERSION)-$(1)
install -d $(DIST)/$(BIN)-v$(VERSION)-$(1)
install -m 755 target/$(1)/release/$(BIN) $(DIST)/$(BIN)-v$(VERSION)-$(1)/
cp README.md $(DIST)/$(BIN)-v$(VERSION)-$(1)/
tar -C $(DIST) -czf $(DIST)/$(BIN)-v$(VERSION)-$(1).tar.gz $(BIN)-v$(VERSION)-$(1)
rm -rf $(DIST)/$(BIN)-v$(VERSION)-$(1)
endef

# Each archive is built on its own platform: the Linux one links against the
# system GTK and the macOS one against the macOS SDK. Collect both archives in
# $(DIST) and run `make checksums` before uploading them to a GitHub release.
dist-linux: ## Build the x86_64 Linux release archive (on Linux)
	$(CARGO) build --release --target $(LINUX_TARGET)
	$(call package,$(LINUX_TARGET))

# On an Intel Mac this cross-compiles; Apple's toolchain links either
# architecture without extra setup.
dist-macos: ## Build the Apple Silicon release archive (on macOS)
	@test -d "$$($(RUSTC) --print sysroot)/lib/rustlib/$(MACOS_TARGET)" || \
		{ echo "the Rust standard library for $(MACOS_TARGET) is missing: rustup target add $(MACOS_TARGET)"; exit 1; }
	$(CARGO) build --release --target $(MACOS_TARGET)
	$(call package,$(MACOS_TARGET))

checksums: ## Write SHA256SUMS for the release archives in dist/
	cd $(DIST) && shasum -a 256 *.tar.gz > SHA256SUMS
	@cat $(DIST)/SHA256SUMS

install: release ## Install the binary into $(PREFIX)/bin
	install -d $(PREFIX)/bin
	install -m 755 target/release/$(BIN) $(PREFIX)/bin/$(BIN)

uninstall: ## Remove the installed binary
	rm -f $(PREFIX)/bin/$(BIN)

clean: ## Remove build artifacts and release archives
	$(CARGO) clean
	rm -rf $(DIST)
