.DEFAULT_GOAL := help

.PHONY: help build clean
help:  ## show this help
	@cat $(MAKEFILE_LIST) | grep -E "^[a-zA-Z0-9_-]+:.*?## .*$$" | \
    awk 'BEGIN {FS = ":.*?# "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

build:
	cargo build --lib

clean:
	cargo clean

.PHONY: test
test:
	cargo test

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

.PHONY: publish
publish: build	## publish
	cargo login
	cargo publish
