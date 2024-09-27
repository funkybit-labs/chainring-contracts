.PHONY: test

build:
	cd evm && make build && cd ../arch && make build && cd ..

lint:
	cd evm && make lint && cd ..

format:
	cd evm && make fmt && cd ..

test:
	cd evm && make test && cd ../arch && make test && cd ..

bitcoin_image:
	cd docker/bitcoin && make build && cd ../..

fulcrum_image:
	cd docker/fulcrum && make build && cd ../..

stop_containers:
	docker compose -p arch-bitcoin-network down --remove-orphans

start_containers: stop_containers
	./start_containers.sh

start_ci_containers: stop_containers
	BITCOIN_IMAGE=851725450525.dkr.ecr.us-east-2.amazonaws.com/bitcoin:latest FULCRUM_IMAGE=851725450525.dkr.ecr.us-east-2.amazonaws.com/fulcrum:latest ./start_containers.sh
