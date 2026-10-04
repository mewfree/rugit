.DEFAULT_GOAL := help
.PHONY: help install run test lint

help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "} /^[a-z-]+:.*## / {printf "  %-8s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

install: ## Install rugit into ~/.cargo/bin
	cargo install --path .

run: ## Run rugit in the current repo
	cargo run

test: ## Run the test suite
	cargo test

lint: ## Clippy, warnings as errors
	cargo clippy --all-targets -- -D warnings
