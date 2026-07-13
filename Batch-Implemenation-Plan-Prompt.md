❯ Let's create an implementation plan for the batch verification work

  read the current notes

  My thinking:

  - let's create a large LRU cache in our ledger/ crate - this cache will live on the native side of the node, and will be large (1000?) - mapping <tx-hashes> -> <proof-verification-result:bool>
    - we don't want this cache to miss - a cache miss should result in an error log (since it represents potentially pretty slow block production/verification)

  - Let's leave the batch verification implementation open for now - just know that this is the API we'll use for the ledger:
  ```
  // Pass defer_proofs() so Transaction::well_formed skips inline verification.
  let deferred = WellFormedStrictness::default().defer_proofs();
  let mut all_evidence = vec![];
  for tx in &block_transactions {
      tx.well_formed(ref_state, deferred, tblock)?;
      all_evidence.extend(tx.collect_proof_evidence(ref_state)?);
  }
  // One batch verify covers contract + dust + zswap proofs for the entire block.
  P::batch_proof_verify(&all_evidence, deferred.proof_verification_mode)?;
  ```

  We should compute this on mempool entry (let's abstract into a func with a todo!() implementation for now, but focus instead on the data flow) and on block import

  On mempool entry, we should use the algorithm in the notes, and create a custom implementation of ChainApi to feed this queue. The VerifiedTransaction returned from well_formed() should be added to the tx validation cache (soft and strict) once the proofs have been verified. We should add a `todo!()` function for the fallback if batch proof verification fails

  For block import, let's create a BlockImport wrapper as indicated by the notes. Block import will first do all the wellformed checks with deffered proofs, then attempt to batch-verify the proofs. There is no fallback for block import - if block import fails, that's it.

  Whenever a transaction is being processed, it should check the proof-verified cache. If there is a cache miss, this is an error log (the cache should always be filled, either when a block is imported, or a tx is entering the mempool)

  The exception to this is the ingress points, in the mempool and in block import. These points should not poll the proof-verification or tx-validation caches.

  Create an implementation plan based on this
