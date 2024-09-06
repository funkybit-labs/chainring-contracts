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
	cd arch/data && ./clear_state.sh && cd ../..
	docker compose -p arch-bitcoin-network up -d

stop_ci_containers:
	docker compose -p arch-bitcoin-network -f docker-compose-ci.yaml down --remove-orphans

start_ci_containers: stop_ci_containers
	cd arch/data && ./clear_state.sh && cd ../..
	docker compose -p arch-bitcoin-network -f docker-compose-ci.yaml up -d

start_ci_all_containers: stop_ci_containers
	docker compose -p arch-bitcoin-network -f docker-compose-ci-all.yaml up -d
