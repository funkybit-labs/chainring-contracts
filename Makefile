.PHONY: test

lint:
	forge fmt --check

format:
	forge fmt

test:
	forge test -vvvv

bitcoin_image:
	cd docker/bitcoin && make build && cd ../..

stop_containers:
	docker compose -p arch-bitcoin-network down --remove-orphans

start_containers: stop_containers
	docker compose -p arch-bitcoin-network up -d

stop_ci_containers:
	docker compose -p arch-bitcoin-network -f docker-compose-ci.yaml down --remove-orphans

start_ci_containers: stop_ci_containers
	docker compose -p arch-bitcoin-network -f docker-compose-ci.yaml up -d
