#!/bin/sh

ord_tar_file=/ord-0.21.3-x86_64-unknown-linux-gnu.tar.gz

tar xf $ord_tar_file

ord-0.21.3/ord --chain=regtest --bitcoin-rpc-url=http://$CORE_RPC_HOST:$CORE_RPC_PORT --bitcoin-rpc-username=$CORE_RPC_USERNAME --bitcoin-rpc-password=$CORE_RPC_PASSWORD --index-sats --index-runes --index-addresses server --http-port=$PORT

