.PHONY: test

lint:
	forge fmt --check

format:
	forge fmt

test:
	forge test -vvvv
