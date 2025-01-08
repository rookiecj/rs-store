.DEFAULT_GOAL := help

.PHONY: help build clean
help:  ## show this help
	@cat $(MAKEFILE_LIST) | grep -E "^[a-zA-Z0-9_-]+:.*?## .*$$" | \
    awk 'BEGIN {FS = ":.*?# "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

setup:	## setup
	cargo install cargo-tarpaulin

build:	## build
	cargo build --lib

build-dev: ## build for development
	cargo build --lib --features dev

clean:	## clean
	cargo clean

.PHONY: test
test:	## test
	cargo test

.PHONY: test-dev
test-dev:	## test for development
	RUST_TEST_THREADS=1 RUST_LOG=debug cargo test --features dev  -- --nocapture

.PHONY: test-cov
test-cov:	## Run test coverage using tarpaulin
	cargo tarpaulin --verbose --all-features --workspace --timeout 120 \
		--out Html --output-dir coverage \
		--exclude-files "examples/*"


.PHONY: test-cov-open
test-cov-open: test-cov	## Run test coverage and open the report
	@if [ "$(shell uname)" = "Darwin" ]; then \
		open coverage/tarpaulin-report.html; \
	elif [ "$(shell uname)" = "Linux" ]; then \
		xdg-open coverage/tarpaulin-report.html; \
	fi

.PHONY: lint
lint:	## lint
	cargo clippy

.PHONY: fmt
fmt: ## format
	cargo fmt

.PHONY: doc
doc:	## doc
	cargo doc --lib --no-deps -p rs-store

example-calc:	## example calc_basic
	cargo run --example calc_basic

example-calc_fn:	## example calc_fn
	cargo run --example calc_fn_basic

example-calc_curr:	## example calc_concurrent
	cargo run --example calc_concurrent

example-calc_unsubcribe:	## example calc_unsubcribe
	cargo run --example calc_unsubscribe

example-calc_clear_subscribers:	## example calc_clear_subscribers
	cargo run --example calc_clear_subscribers

example-calc_thunk:	## example calc_thunk
	cargo run --example calc_thunk

example-calc_basic_builder:	## example calc_basic_builder
	cargo run --example calc_basic_builder

examples: example-calc \
	example-calc_fn \
	example-calc_curr \
	example-calc_unsubcribe \
	example-calc_clear_subscribers \
	example-calc_thunk \
	example-calc_basic_builder	## example all

VERSION := $(shell cargo pkgid -p "rs-store" | cut -d\# -f2 | cut -d@ -f2)

.PHONY: version
version:
#	cargo metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name == "rs-store") | .version'
	@echo $(VERSION)

.PHONY: check_version_tag
check_version_tag: TAG_EXISTS := $(shell git tag | grep -q "^v$(VERSION)" && echo "true" || echo "false")
check_version_tag:	## check version tag
	# version tag should not be found
	[ "$(TAG_EXISTS)" = "false" ]

.PHONY: add_tag
add_tag:	## add tag
	git tag -a "v$(VERSION)" -m "v$(VERSION)"

.PHONY: publish
publish: check_version_tag build publish_cargo add_tag	## publish
	@echo "published v$(VERSION)"
	@echo push tag v$(VERSION) to origin
	@git push origin v$(VERSION)
	@git push origin main

.PHONY: publish_cargo
publish_cargo:	## publish to cargo
	cargo login
	cargo publish
