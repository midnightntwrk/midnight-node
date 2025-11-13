---
title: Substrate Chain Specifications
---

## Intro
In Substrate, a chain specification is the collection of information that describes a Substrate-based blockchain network. For example, the chain specification identifies the network that a blockchain node connects to, the other nodes that it initially communicates with, and the initial state that nodes must agree on to produce blocks.

Chain specs can be provided to a node either through a hard-coded means(see chain_spec.rs), or a generated version(see ./node/res). It is common to write different functions or generate chain spec files to represent different network deployments. E.g. a chain spec by the name of "local.json" might represent a chain spec by the name of local("local" in the case of our network, represents the public devnet, due to timing of release).

## Examples
Build "local" chain specification:
```shell
./target/release/midnight-node build-spec --disable-default-bootnode > chain-spec.json
```

Get "raw" version of "local chain specification:
```shell
./target/release/midnight-node build-spec --chain chain-spec.json --disable-default-bootnode > chain-spec-raw.json
```

Start a node based on a chain spec file
```shell
./target/release/midnight-node --chain chain-spec-raw.json
```
