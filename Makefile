.DEFAULT_GOAL := help

.PHONY: help
help:  ## show this help
	@cat $(MAKEFILE_LIST) | grep -E "^[a-zA-Z0-9_-]+:.*?## .*$$" | \
    awk 'BEGIN {FS = ":.*?# "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

example:
	cargo run --bin example