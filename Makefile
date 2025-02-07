.PHONY: test

build:
	(cd evm && make build) && (cd coin-proxy && make build)

lint:
	(cd evm && make lint) && (cd coin-proxy && make lint)

format:
	(cd evm && make format) && (cd coin-proxy && make format)

test:
	(cd evm && make test) && (cd coin-proxy && make test)

bitcoin_image:
	cd docker/bitcoin && make build && cd ../..

stop_containers:
	docker compose -p arch-bitcoin-network down --remove-orphans

start_containers: stop_containers
	./start_containers.sh

start_ci_containers: stop_containers
	BITCOIN_IMAGE=851725450525.dkr.ecr.us-east-2.amazonaws.com/bitcoin:latest ORD_IMAGE=851725450525.dkr.ecr.us-east-2.amazonaws.com/ord:latest ./start_containers.sh
