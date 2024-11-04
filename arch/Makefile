.PHONY: test

build:
	cd contracts/exchange/program && cargo build-sbf && cd -

test: build
	cd contracts/exchange && RUST_BACKTRACE=1 RUST_LOG=debug,reqwest=info cargo test --  --test-threads 1 --nocapture && cd -
