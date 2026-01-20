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

Note: Use `--unsafe-rpc-external` when running the midnight node. This is to allow unsafe rpc calls, like `author_insertKey`.

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

### Payload
The Payload is inside the `Justification` -> `BeefySignedCommitment` -> `Payload`.
Example encoded Payload:
```
0x146362b00000000000000000040000007f0c9b27381104febfb4a6be51e8fc0f08ba70060531fc5fcf60dcbed1f4e5f96373950210020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a100000000000000000390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f2701000000000000000389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb010000000000000003bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c00000000000000006d6880cf8f937e2fbfc0b9281ff6b7c125e56553eef5afecdece1ce0edb5f05e9b4c3d6e62b0010000000000000004000000bc6ac935d96c7f67b6c697417300282da5f56302e2b8b1c3e73dadbb9c4b987b6e73950210020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a101000000000000000390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f2701000000000000000389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb010000000000000003bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c0000000000000000
```
The Payload contains (at most) 5 keyed data:
* MMR Root
* Current Beefy Stakes
  * A list of tuple (Beefy Id, Stake)
* Current Beefy Authority Set
  * The merkle root of the Current Beefy Stakes
* Next Beefy Stakes
  * The expected list of tuple (Beefy Id, Stake) on the next session
* Next Beefy Authority Set
  * The merkle root of the Next Beefy Stakes

From the example payload, the current Beefy Stakes:  
```
 [
   (
      Public(
            020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1 (KW39r9CJ...),
      ),
      0,
   ),
   (
      Public(
            0390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f27 (KWByAN7W...),
      ),
      1,
   ),
   (
      Public(
            0389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb (KWBpGtyJ...),
      ),
      1,
   ),
   (
      Public(
            03bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c (KWCycezx...),
      ),
      0,
   ),
]
```  

The current Authority Set:
```
BeefyAuthoritySet {
   id: 0,
   len: 4,
   keyset_commitment: 0x7f0c9b27381104febfb4a6be51e8fc0f08ba70060531fc5fcf60dcbed1f4e5f9,
}
```    

### Authority Proof
Based on the _current_ Beefy Stakes and the signatures.

With this example of Signatures:
```
Signatures: [
    Some(
        Signature(
            df0a4327e04d8f987c6b0e70c8de42c09bc28e55c564de2439d008fe3734ec4f11e1dad3468a4c90f6fd0520235dab4262c6821e81505fbd20c258465d55362e00,
        ),
    ),
    Some(
        Signature(
            0b9a840d99989290b6931428d3a168b40c55bc1ed5911467a598b8041df8106f168eecb51b763fea3f982089ea7a070ac2d94672ea0acc25c6ce3a1fe66a23fe00,
        ),
    ),
    Some(
        Signature(
            a450c9cbebc56d50d5b8191cfaa7c2aba3fa119f2daf0be01687f52a24e8532e287cf2bdd6e6a50c11868e80b8a0a914742304d8144d2cd9ddfe5cdd8f9dd1c101,
        ),
    ),
    None,
]
```
It shows index 0, 1, 2 have signed the commitment.   
The authorites proof will look like this:
```
AuthoritiesProof {
    root: 0x7f0c9b27381104febfb4a6be51e8fc0f08ba70060531fc5fcf60dcbed1f4e5f9,
    total_leaves: 4,
    proof: [
        [
            842bf70c249dcf599308bd26cdfa5daa3267e430d0f1bfd8d54d8f87ecc344ce,
            616d2b6ddf7e12af84115334c3f52f1cfc18b10fe10d66c525bb9461e9365c31,
        ],
        [
            9b0fb5c442fabe52618fa06c856cc5e222baf04d8f70a29fcf151ca50e192d9f,
            cf29b3b2a189561f70b84fa20478a6d89d2ffae3167b3a2ef19256af48d26cca,
        ],
    ],
}
```

### RelayChain Proof
Contains the signed commitment file and the authorities proof:
```
RelayChainProof {
   signed_commitment: SignedCommitment {
      commitment: Commitment {
            payloads: [
               Payload {
                  id: "0x6d68",
                  data: "0xcf8f937e2fbfc0b9281ff6b7c125e56553eef5afecdece1ce0edb5f05e9b4c3d",
               },
            ],
            block_number: 89,
            validator_set_id: 0,
      },
      votes: [
            Vote {
               signature: "0xdf0a4327e04d8f987c6b0e70c8de42c09bc28e55c564de2439d008fe3734ec4f11e1dad3468a4c90f6fd0520235dab4262c6821e81505fbd20c258465d55362e",
               authority_index: 0,
               public_key: "0x020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1",
            },
            Vote {
               signature: "0x0b9a840d99989290b6931428d3a168b40c55bc1ed5911467a598b8041df8106f168eecb51b763fea3f982089ea7a070ac2d94672ea0acc25c6ce3a1fe66a23fe",
               authority_index: 1,
               public_key: "0x0390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f27",
            },
            Vote {
               signature: "0xa450c9cbebc56d50d5b8191cfaa7c2aba3fa119f2daf0be01687f52a24e8532e287cf2bdd6e6a50c11868e80b8a0a914742304d8144d2cd9ddfe5cdd8f9dd1c1",
               authority_index: 2,
               public_key: "0x0389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb",
            },
      ],
   },
   proof: AuthoritiesProof {
      root: 0x7f0c9b27381104febfb4a6be51e8fc0f08ba70060531fc5fcf60dcbed1f4e5f9,
      total_leaves: 4,
      proof: [
            [
               842bf70c249dcf599308bd26cdfa5daa3267e430d0f1bfd8d54d8f87ecc344ce,
               616d2b6ddf7e12af84115334c3f52f1cfc18b10fe10d66c525bb9461e9365c31,
            ],
            [
               9b0fb5c442fabe52618fa06c856cc5e222baf04d8f70a29fcf151ca50e192d9f,
               cf29b3b2a189561f70b84fa20478a6d89d2ffae3167b3a2ef19256af48d26cca,
            ],
      ],
   },
}
```

Plutus data of the example RelayChain Proof:
```
0xd8799fd8799fd8799f9fd8799f426d685820cf8f937e2fbfc0b9281ff6b7c125e56553eef5afecdece1ce0edb5f05e9b4c3dffff185900ff9fd8799f5840df0a4327e04d8f987c6b0e70c8de42c09bc28e55c564de2439d008fe3734ec4f11e1dad3468a4c90f6fd0520235dab4262c6821e81505fbd20c258465d55362e005821020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1ffd8799f58400b9a840d99989290b6931428d3a168b40c55bc1ed5911467a598b8041df8106f168eecb51b763fea3f982089ea7a070ac2d94672ea0acc25c6ce3a1fe66a23fe0158210390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f27ffd8799f5840a450c9cbebc56d50d5b8191cfaa7c2aba3fa119f2daf0be01687f52a24e8532e287cf2bdd6e6a50c11868e80b8a0a914742304d8144d2cd9ddfe5cdd8f9dd1c10258210389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afbffffffd8799f58207f0c9b27381104febfb4a6be51e8fc0f08ba70060531fc5fcf60dcbed1f4e5f9049f9f5820842bf70c249dcf599308bd26cdfa5daa3267e430d0f1bfd8d54d8f87ecc344ce5820616d2b6ddf7e12af84115334c3f52f1cfc18b10fe10d66c525bb9461e9365c31ff9f58209b0fb5c442fabe52618fa06c856cc5e222baf04d8f70a29fcf151ca50e192d9f5820cf29b3b2a189561f70b84fa20478a6d89d2ffae3167b3a2ef19256af48d26ccaffffffff
```
