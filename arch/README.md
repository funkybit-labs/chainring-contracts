# arch

This contains the arch exchange contract and associated tests

## Requirements:
- [Docker](https://www.docker.com/)
- [Rust](https://www.rust-lang.org/)
- A C++ Compiler (gcc/clang)
- [RISC0 Toolchain](https://www.risczero.com/) (instructions below)

## Getting Started

In order to test we must start a local arch network and bitcoin node. The arch network consists of 3 nodes - a boot node, a validator node and a zkvm node.
An init node is started which sends some commands to configure the arch network but the exists when done.


### 1 - Start the containers
- From the root of this project you will need to run the following command once
```bash
make bitcoin_image
```
- To start the arch network and bitcoin node:
```bash
make start_containers
```
- The nodes store data in `./arch/data` off the root of the project. You might need to wipe these directories sometimes. There is a clean_state.sh script

### 2 - Compile and Test

### 2.1 - Install RISC0-Toolchain

To compile , the solana must be installed. Execute the following commands to install the toolchain to your local system.

```bash
sh -c "$(curl -sSfL https://release.solana.com/v1.18.18/install)"
```

### 2.2 - Compile and run the exchange program
- From the `arch` folder: run the following command:
```bash
make build
```
- This will compile the exchange program into an eBPF file. You'll find the generated file at `arch/contracts/exchange/program/target/sbf-solana-solana/release/exchangeprogram.so`
- To run the unit tests:
```bash
make test
```
- to run individual tests you can go to  `arch/contracts/exchange` folder and run: `RUST_BACKTRACE=1 RUST_LOG=debug cargo test <test_name> -- --test-threads 1 --nocapture`

## General approach

- To deploy the program we must first create a keypair, and then use that to create an account. Th
- All state is stored in accounts. There is a process for onboarding the account onto the arch network
- The process involves sending a bitcoin transaction to the arch network with the uxto to be used to hold state. The account must be create byu calling the system program and then ownership assigned to our program.
- In our case the authority would be our submitter. This means any arch transactions to change state on these utxos must be signed by our submitter key
- The initial implementation is using one utxo to hold exchange state like fee account, settlement batch hash, last settlement or withdrawal hash
- We then have a state utxo per asset type that holds each wallets balance for that asset type. We will have to see how this scales and can be optimized.
- The `handler()` method is the entry point for the contract. When invoking the arch RPC to send a transaction you must send the list of utxos needed and instructions
- the instructions are things like deposit, withdraw, prepare batch etc.
- currently both the instructions and state use borsch serialization.
- the arch framework will lookup and attach the current state and authorities to the list of UTXOs before invoking the handler.
- the handler using the instructions will perform the necessary operations and will update the state on the UTXOS
- this updated state gets persisted into new state utxos which are sent to the block chain.
- From the client perspective that means the identifiers for the state utxos are changing every transaction if the state on that utxo changes, 
so we have to query the arch network for the processed tx to find the new utxo identifiers
