# Midnight Relayer

1. Start node with the following args:
```
        --state-pruning archive
        --blocks-pruning archive
        --enable-offchain-indexing true
```

2. Ensure that BEEFY begins. The node must have relevant BEEFY keys inserted, and the first session must have passed.

### Run with local node
You may need to insert the BEEFY key manually, after the node starts.  

Note: Omit `--unsafe-rpc-external` when running the midnight node. This is to allow unsafe rpc calls, like `author_insertKey`.  

#### How to insert
Make sure to have ready the following details:
* keyType: beef
* suri: < secret seed >
* publicKey: < `ECDSA` public key of the secret seed, in 0x.. format > 

1. Via polkadot js:
   1. Go to Developer -> RPC Calls
   2. Select `author` endpoint and `insertKey` method
   3. Input your corresponding data
2. Via curl:
   ```      
    curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d \
        '{
        "jsonrpc":"2.0",
        "id":1,
        "method":"author_insertKey",
        "params":["beef","<suri>","<publicKey>"]
        }'
   ```
3. Via this relayer:  
   1. Prepare json file of all the beefy keys and their corresponding urls
      * ```json
         [
          {
           "suri": "//Alice",
           "pub_key": "0x020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1",
           "node_url": "ws://localhost:9937"
          }
         ]
        ```
      * see example [beefy-keys-mock.json](../res/mock-bridge-data/beefy-keys-mock.json)
   2. Execute:  
      ```
       cargo run --bin midnight-beefy-relay -- --keys-path=<file_path>
      ```
In the logs it will display: 
```
Added beefy key: 0x020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1 to ws://localhost:9933
Added beefy key: ...
```

#### Testing with Local Environment
Prerequisite: The [local environment](../local-environment/) should already be running. 
Connect to any of the available ports of nodes 1 to 4.

Run: 
```
cargo run --bin midnight-beefy-relay -- --keys-path res/mock-bridge-data/beefy-keys-mock.json
```

#### Log:
A successful connection will show: 
```
Starting relay...
Connecting to ws://localhost:9933

Querying from the best block number: 857
BeefyConsensusState {
    latest_height: 857,
    activation_block: 0,
    current_authority_set: AuthoritySetCommitment {
        id: 0,
        len: 4,
        root: "a33c1baaa379963ee43c3a7983a3157080c32a462a9774f1fe6d2f0480428e5c",
    },
    next_authority_set: AuthoritySetCommitment {
        id: 1,
        len: 4,
        root: "a33c1baaa379963ee43c3a7983a3157080c32a462a9774f1fe6d2f0480428e5c",
    },
}
RelayChainProof {
    signed_commitment: SignedCommitment {
        commitment: Commitment {
            payloads: [
                Payload {
                    id: "6d68",
                    data: "7993b591f341670214276fe35f2338ad8338aaf0663e9353f5c27524be1bd975",
                },
            ],
            block_number: 857,
            validator_set_id: 0,
        },
        votes: [
            Vote {
                signature: "ba9b21bb15c1fa57d26f5aeb1b45e9e2167ee3f7c38be703716d51737dcafe6a71e9d9c4b3a691b2670d0d54399d0d90b8e81a553e18882533f53b1834e9763c",
                authority_index: 0,
                public_key: "020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1",
            },
            Vote {
                signature: "c25e719a58d7246caa211fa6ca501ddd619fb2b8668cf19f8a9e23e678b6d18720775efe10da81616020284796f11e3964add804ce915f4c04eda94cc193b344",
                authority_index: 1,
                public_key: "0390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f27",
            },
            Vote {
                signature: "42d6631d3c1ea19359a0b4990434a5cb745bf993dbfb218731c3f32ee40aacd90d4f7c083676063129f08a6abfa0771006d05f633e82eaab4b6d4adbab7f6a99",
                authority_index: 2,
                public_key: "0389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb",
            },
        ],
    },
    latest_mmr_leaf: BeefyMmrLeaf {
        version: 0,
        parent_number: 856,
        parent_hash: "e88ef15e3aac470b0bdafb9bf32ab69fb790a2d5a4edce115ef2279ac4e0d323",
        next_authority_set: AuthoritySetCommitment {
            id: 1,
            len: 4,
            root: "a33c1baaa379963ee43c3a7983a3157080c32a462a9774f1fe6d2f0480428e5c",
        },
        extra: [],
        k_index: 0,
        leaf_index: 856,
    },
    mmr_proof: [
        "009825f3b665bca192f7fbd2c1a0d616f6259f08814533b3014e4010c8e18367",
        "7fb47a293b182d09187b5914ac41efaebd5f8485ddcae83b85112a29d864ff6d",
        "714f568ca046e3a092cbd31c2eb39f69e0d6ac82305554bfa4e1ecfe69acefd9",
        "8d42c447926f941f92472a134ea4c1ebf7f70dd020d2ee303ee84096263abe50",
        "b2a3e55008461ee077065f944904c02cf130f6809c84950023ca541237f3c0af",
    ],
    proof: AuthoritiesProof {
        root: 0xa33c1baaa379963ee43c3a7983a3157080c32a462a9774f1fe6d2f0480428e5c,
        total_leaves: 4,
        proof: [
            [
                ea5e28e6e07cc0d6ea6978c5c161f0da9f05ad6d5c259bd98a38d5ed63c6d66d,
                2434439b3f6496cdfc9295f52379b6dd06c6d3f72bb3fd7f367acf4cde15a5c4,
            ],
            [
                69ccb87a5d16f07350e6181de08bf71dc70c3289ebe67751b7eda1f0b2da965c,
                54e7776947cbea688edb0eafffef41c9bf1d91bf02b51b0debb8e9234679200a,
            ],
        ],
    },
}
Plutus Beefy Consensus State: "d8799f19035900d8799f00045820a33c1baaa379963ee43c3a7983a3157080c32a462a9774f1fe6d2f0480428e5cffd8799f01045820a33c1baaa379963ee43c3a7983a3157080c32a462a9774f1fe6d2f0480428e5cffff"
Plutus Relay Chain Proof: "d8799fd8799fd8799f9fd8799f426d6858207993b591f341670214276fe35f2338ad8338aaf0663e9353f5c27524be1bd975ffff19035900ff9fd8799f5840ba9b21bb15c1fa57d26f5aeb1b45e9e2167ee3f7c38be703716d51737dcafe6a71e9d9c4b3a691b2670d0d54399d0d90b8e81a553e18882533f53b1834e9763c005821020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1ffd8799f5840c25e719a58d7246caa211fa6ca501ddd619fb2b8668cf19f8a9e23e678b6d18720775efe10da81616020284796f11e3964add804ce915f4c04eda94cc193b3440158210390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f27ffd8799f584042d6631d3c1ea19359a0b4990434a5cb745bf993dbfb218731c3f32ee40aacd90d4f7c083676063129f08a6abfa0771006d05f633e82eaab4b6d4adbab7f6a990258210389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afbffffffd8799f001903585820e88ef15e3aac470b0bdafb9bf32ab69fb790a2d5a4edce115ef2279ac4e0d323d8799f01045820a33c1baaa379963ee43c3a7983a3157080c32a462a9774f1fe6d2f0480428e5cff4000190358ff9f5820009825f3b665bca192f7fbd2c1a0d616f6259f08814533b3014e4010c8e1836758207fb47a293b182d09187b5914ac41efaebd5f8485ddcae83b85112a29d864ff6d5820714f568ca046e3a092cbd31c2eb39f69e0d6ac82305554bfa4e1ecfe69acefd958208d42c447926f941f92472a134ea4c1ebf7f70dd020d2ee303ee84096263abe505820b2a3e55008461ee077065f944904c02cf130f6809c84950023ca541237f3c0afffd8799f5820a33c1baaa379963ee43c3a7983a3157080c32a462a9774f1fe6d2f0480428e5c049f9f5820ea5e28e6e07cc0d6ea6978c5c161f0da9f05ad6d5c259bd98a38d5ed63c6d66d58202434439b3f6496cdfc9295f52379b6dd06c6d3f72bb3fd7f367acf4cde15a5c4ff9f582069ccb87a5d16f07350e6181de08bf71dc70c3289ebe67751b7eda1f0b2da965c582054e7776947cbea688edb0eafffef41c9bf1d91bf02b51b0debb8e9234679200affffffff"
```