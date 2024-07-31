#!/bin/sh

bitcoin_dir=/bitcoin
bitcoin_core_tar_file=$bitcoin_dir/bitcoin-27.1-x86_64-linux-gnu.tar.gz

# untar bitcoin 
tar xf $bitcoin_core_tar_file -C $bitcoin_dir
rm $bitcoin_core_tar_file

# move bitcoind conf file to ~/.bitcoin
mkdir $HOME/.bitcoin
mv /bitcoin/bitcoin.conf $HOME/.bitcoin

# create softlink to conf file for convenience
ln -s $HOME/.bitcoin/bitcoin.conf $bitcoin_dir/bitcoin.conf

# start bitcoind in foreground and wait for it to init
bitcoind -regtest -datadir=$HOME/.bitcoin -fallbackfee=0.0002 -txindex -daemon
sleep 1

# add known test wallet and add some blocks to fill it with some BTC
bitcoin-cli restorewallet testwallet /bitcoin/wallet.dat
bitcoin-cli --regtest generatetoaddress 101 $FAUCET_ADDRESS >/dev/null 2>&1

# stop bitcoind running in background now that wallet has been initialized
bitcoin-cli --regtest -datadir=$data_dir stop

# wait for the daemon to exit before restarting bitcoind in foreground
while [ -n "$(ps x | grep bitcoind | grep daemon)" ]; do
    sleep 1
done

# start bitcoind in foreground with REST enabled for health check
bitcoind -regtest -datadir=$HOME/.bitcoin -fallbackfee=0.0002 -txindex -rest -wallet=testwallet
