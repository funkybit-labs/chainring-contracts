#!/bin/sh

./Fulcrum regtest.conf -b $BITCOIND_HOST:$BITCOIND_PORT -D $DATADIR
