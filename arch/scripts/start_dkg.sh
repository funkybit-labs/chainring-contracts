#!/bin/bash

echo -e "\nRunning start_dkg"

sleep 3

curl -v -sLX POST \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":"id","method":"start_dkg","params":[]}' \
  http://localhost:9002/

echo -e "\nDone!"