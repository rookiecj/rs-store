.DEFAULT_GOAL := help

.PHONY: help build clean
help:  ## show this help
	@cat $(MAKEFILE_LIST) | grep -E "^[a-zA-Z0-9_-]+:.*?## .*$$" | \
    awk 'BEGIN {FS = ":.*?# "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

build:	## build
	cargo build --lib

clean:	## clean
	cargo clean

.PHONY: test
test:	## test
	cargo test

.PHONY: doc
doc:	## doc
	cargo doc --lib --no-deps -p rs-store


example-calc:	## example calc_basic
	cargo run --bin calc_basic

example-calc_fn:	## example calc_fn
	cargo run --bin calc_fn_basic

example-calc_curr:	## example calc_concurrent
	cargo run --bin calc_concurrent

example-calc_unsubcribe:	## example calc_unsubcribe
	cargo run --bin calc_unsubscribe

example-calc_clear_subscriber:	## example calc_unsubcribe
	cargo run --bin calc_clear_subscriber

example-calc_thunk:	## example calc_thunk
	cargo run --bin calc_thunk


example: example-calc example-calc_fn example-calc_curr	example-calc_unsubcribe example-calc_clear_subscriber example-calc_thunk	## example all

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

.PHONY: publish_cargo
publish_cargo:	## publish to cargo
	cargo login
	cargo publish

