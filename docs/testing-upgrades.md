# Testing Upgrades

Entrypoint for upgrade testing tooling is in scripts/upgrade_test.sh.

## Purpose
The upgrade tooling informs on any incompatibilities between old and new states of the node and runtime. It aims to provide as close of an emulation of a real environment upgrade as possible by using the node images themselves as opposed to just modelling the state transition.

The upgrade tooling should be used on every change to the node which:
1. Alters the node client
2. Alters the node's runtime

## Scenarios
The upgrade scripts should be used in the following scenarios
1. runtime upgrade
2. node upgrade
3. runtime upgrade followed by node upgrade
4. node upgrade followed by a runtime upgrade

What it does:
1. Fork important state(non-consensus/system) of a live environment
2. spin up a local network(docker)
3. Attempt your given upgrade type against the network
4. Leave you with an upgraded network which you can then then test/observe/break

## Dependencies
Subalfred - the tool attempts to install subalfred if you do not already have it.

## Usage: 
The tools require a number of environment variables in order to function:

`CURRENT_IMAGE`: The image tag(including container registry+repository) which the local network should be launched with.
`NEW_IMAGE`: The image tag(including container registry+repository) representing the image you want to upgrade the local network to.
`CHAIN_SPEC`: A raw JSON file containing the genesis of your environment. This will be merged with a new json chain spec containing forked state of the live environment.
`WASM_PATH`: The path to the WASM file of the runtime you would like to upgrade the local network to.
`NODE_URL`: The full ws/wss url to reach a node. This is used to request the full state of the node by the fork script.
`SUDO_KEY`: The privileged sudo key for the given environment.
