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
