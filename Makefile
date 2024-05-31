.DEFAULT_GOAL := help

.PHONY: help
help:  ## show this help
	@cat $(MAKEFILE_LIST) | grep -E "^[a-zA-Z0-9_-]+:.*?## .*$$" | \
    awk 'BEGIN {FS = ":.*?# "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

build:
	cargo build --lib

clean:
	cargo clean

example-calc:	## example calc_basic
	cargo run --bin calc_basic

example-calc_fn:	## example calc_fn
	cargo run --bin calc_fn_basic

example-calc_curr:	## example calc_concurrent
	cargo run --bin calc_concurrent

example: example-calc example-calc_fn example-calc_curr	## example all

publish: build	## publish
	cargo login
	cargo publish
