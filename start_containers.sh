#!/bin/bash

echo -e "\nRunning start containers"

for loop in {1..8}
do
  echo "Attempt $loop"
  sleep 2
  docker compose -p arch-bitcoin-network down --remove-orphans
  rm -rf arch/.arch-data
  sleep 1
  if docker compose -p arch-bitcoin-network up -d; then
      echo "Docker success!"
  else
      echo "docker failed retrying .."
      continue
  fi

  # wait till we see Ready for DKG
  ready="0"
  echo "Waiting for leader to be ready for DKG"
  for countdown in {30..0}
    do
      sleep 1
      ready=$(docker logs arch-bitcoin-network-leader-1 | grep -c "Ready to start DKG")
      if [ "$ready" == "0" ]
        then
           echo "Not ready $countdown"
        else
           echo "DKG is ready"
           break
        fi
    done

  if [ "$ready" == "0" ]
    then
      continue
    fi

  result=$(curl -sLX POST -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":"id","method":"start_dkg","params":[]}' http://localhost:9002/ | jq .error)

  if [ "$result" == "null" ]
  then
    echo "start DKG passed"
  else
    echo "Failed with $result, Retrying .."
    continue
  fi

  sleep 3
  echo "Verifying DKG"
  result=$(curl -sLX POST -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":"id","method":"get_account_address","params":[253,202,185,92,100,57,129,202,241,10,232,30,20,105,68,186,183,157,236,0,154,126,186,31,35,100,165,246,138,250,58,219]}' http://localhost:9002/ | jq .error)

  if [ "$result" == "null" ]
  then
    echo "Verified DKG"
  else
    echo "Failed to verify DKG with $result, Retrying .."
    continue
  fi

  # wait till we see ready for network
  ready="0"
  echo "Waiting for network to reach ready state"
  for countdown in {25..0}
    do
       sleep 1
       ready=$(docker logs arch-bitcoin-network-leader-1 | grep -c "Network is ready")
       if [ "$ready" == "0" ]
        then
           echo "Not ready $countdown"
        else
           echo "Network is ready"
           break
        fi
    done
  if [ "$ready" == "0" ]
    then
      continue
    fi

  num_blocks="0"
  echo "Waiting for multiple blocks to be processed"
  for countdown in {30..0}
    do
      sleep 1
      num_blocks=$(docker logs arch-bitcoin-network-leader-1 | grep -c "Starting block")
      if [ "$num_blocks" -gt "3" ]
        then
           echo "$num_blocks blocks processed"
           break
        else
           echo "Not ready $countdown"
        fi
    done

  if [ "$num_blocks" -gt "3" ]
    then
      break
    fi

done
echo "Done!"
