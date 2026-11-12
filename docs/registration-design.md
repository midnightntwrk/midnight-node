# Registered Candidates Observability

Status: **Draft / proposal** Scope: how a Midnight observes permissionless candidates that
are backed by ADA stake, and how the keys they register reach `pallet_session`.

---

## 1. Overview and assumptions

Midnight is meant to select its committee with **Ariadne**, which draws from two pools:
**permissioned candidates**, a governance-curated list, and **registered candidates**, Cardano
SPOs whose stake earns them a chance at a seat. Only the permissioned pool is in use. This
document designs the registered (permissionless) one.

There is no deployed registration format, no operator to migrate, and no compatibility constraint
— `num_registered_candidates` is 0 on every network and no registration UTxO has ever been posted.

There will be Cardano Miniting Policy and Validator working together with Midnight Observability
and Pallet to achieve the goal. This document will refer to it as something that is given.

There will be offchain code to help working with Registration Miniting Policy and Validator.
Out of scope of this document.

This document focuses on Midnight side.

High level: observability is processing UTXOs deposited at the Registration Validator address.
Lower layer, currently implemented by queries to db-sync postgres, filters out
UTXOs with no Registration Minting Policy token attach (Registration Token).
Observed UTXOs have datum in a specified format (otherwise they would not have token minted).

Assumptions about Cardano (off-chain an on-chain):

- A1 valid registration UTXO with have Registration Token attached
- A2 Registration Token can be minted only by real SPO
- A3 Registration Policy checks datum format
- A4 Registration Policy checks SPO pool_id/public key hash field of datum
- A5 Registration Datum has Active/Inactive flag
- A6 Registration UTXO can be spent only when depositing new registration UTXO (possibly with 'inactive' flag)
- A7 Registration UTXO with Inactive flag can not be spent
- A8 One Transaction can deposit at most one Registration UTXO
- A9 Registration UTXO is the first output of a transaction
- A10 cross-chain key cannot be re-used by SPO other than put it on Cardano in the first place.

General observability rule: each UTXO deposit is processed once. Midnight runtime will keep cursor
pointing to already processed Cardano blocks/transactions. Observability looks only at stable
part of Cardano. This means our assumption is that nothing ever gets rolled back.
This high bound of observation never decreses: even if indexer we use drops some blocks from tip
of the chain, we know that stable never changes its status to unstable.

---

## 2. Context

### 2.0 Keys relationship

Cardano Stake key identifies registration.

Registration owns cross-chain key. One cross-chain key can't have by used by more than one registration. To be enforced on Cardano.
Registration can change cross-chain key it uses. Cross-chain key is Midnight side identity of registration.
One SPO can change Midnight identity it backs.

Cross-chain key owns each session key from the registration.
No session key can change cross-chain key that owns it nor can be own by more than one.

### 2.1 From registration to committee

The runtime recomputes selection from state every node agrees on, so validation runs in the
runtime and every node reaches the same answer. Selection randomness is seeded from the Cardano
epoch nonce and the Midnight epoch.

Registered candidates begin participating in selection the moment the D-parameter is updated and R
becomes non-zero. It is a governance action, not a release, and it is deliberately the last thing
to move: everything else in this document can ship while R is 0, with no candidate selected from a
registration and so no activation risk.

A selected candidate's `AccountId` is derived from their cross-chain ECDSA public key, which is
part of the Cardano registration.

### 2.2 What Cardano gives us, and what it does not

See assumptions in section 1.

A1 and A2 together with processing each UTXO at most once (bar crashes) gives us protection against spamming attacks.
Early stages of processing will filter out each UTXO without Registration Token.

A3 is nice property, but Midnight code has to parse Registration Datum anyway. Parsing is in the Node code.
If there is a Datum that Node fails to parse it will stop processing (but can't stop livness).
Such situation should be resolved by a new node release that either fixes own bug or have a special treatment
for Datums that Registration Policy let in.

A4 - there is Cold Stake Key hash and signature in the Datum, Midnight does not have to check this signature,
because Cardano did it.

A5 and A6 - observing UTXO with either Active or Inactive flag are separate events that move state machine in different directions.
Layers close to indexer do not have to track UTXO consumption.

A8 and A9 - possiblity shirinking SQL join making queries faster

**A validator address is permissionless, and nothing gates deposit.**
Make spamming attack easy. Mitigated by observing UTXO with Registration Token.
This is a quick/cheap filter.

### 2.3 Automatic session key saving for `pallet_session`

The Midnight runtime saves committee session keys **on the operator's behalf**. This is a
deliberate product decision (circa 2024): operators are not asked to submit `pallet_session`
`set_keys` themselves. That call path is closed, and permissioned candidates already rely on the
automatic one.

The consequences of this decision are important. Attempt to `set_keys` of a session key,
that is already owned by another `AccountId`, at worst stops the chain.

Selection, that happens after managing candidates, has a post-filter that tries to `set_keys`
of all selected candidates and in case of failure it returns identical committee to one
currently operating. This is a saftely valve for chain liveness.

Currently Midnight call `set_keys` after the committee is selected.
This design adds another place from which `set_keys` is called - when managing registration.
`purge_keys` will not be called.

Cardano (minting policy) cannot check session keys in the registration message.

This will be responsibility of the registration pallet.

Rules are:

- Cross-chain key (mapped to `AccountId`) can not change owner (SPO). It is given from Registration Policy. Used as assumption in Midnight code.
- SPO can rotate its cross-chain key.
- Cross-chain ('crch') key rotation requires rotation of every session key.
- Session keys can be rotated individually when cross-chain key does not change.

If there is illegal session key re-use, then given registration is saved as Invalid.
Such registration will be forgotten when superseded by a valid registration.
Invalid registration is kept in pallet for user convenience - it would point to why it is invalid.

Design decision: pallet does not store 'crch' to/from 'session key' mapping.
During registration storing it will call `set_keys` (after validating PoP),
if `set_keys` fails registration will be invalid with this reason specified.

This makes pallet simpler and remove duplication of session keys mapping from the runtime.

Consequence of this decision is that change of the SessionKeys shape will always require
a migration strategy that isn't asking users to register their new key upfront.
This design does not solve the problem of how a new key is added to the SessionKeys.
Assumption is that code is deployed after BABE and BEEFY keys are added to SessionKeys.
AURA key removal is planned and it should work out of the box.

### 2.4 Observation by events

Registrations are **event-sourced**, following `pallets/cnight-observation`: the node observes
outputs at the candidates address appearing since a stored cursor, in bounded
batches, and the runtime maintains the registration set in its own storage. The pattern is already
in production here — `NextCardanoCursor`. Because of A8 cursor needs to point to transaction,
doesn't need anyting inside it.

Where this design departs from cNIGHT is in what bounds a batch. The bound will be on registration events,
which are at most 1 to 1 with deposited UTXOs.

---

## 3. Requirements and threat model

| #   | Requirement                                                                                                                                                            |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R1  | Every session key in a registration is accompanied by a proof that the registrant holds the private key.                                                               |
| R2  | A proof is valid only for the registration carrying it — not liftable into another operator's registration, nor replayable onto another network.                       |
| R3  | A registration is attributable to a stake pool only with that pool's signature, and that signature commits to the exact key set.                                       |
| R4  | `register_committee_keys` cannot fail for a committee Ariadne selected. Key conflicts are resolved before selection.                                                   |
| R5  | Removed                                                                                                                                                                |
| R6  | Verification is paid once per observed registration, not per candidate per epoch.                                                                                      |
| R7  | Operators can tell, before and after posting, whether their registration is valid and why not.                                                                         |
| R8  | Observation cost per block is bounded independently of what exists on Cardano. Spam delays observation; it never halts or forks the chain.                             |
| R9  | A registration's validity is decided once, from the facts at its observation, and never revisited. Nothing that happens afterwards makes an invalid registration valid |

### Threat model

Trusted: the Midnight runtime; the governance origin setting the D-parameter and the permissioned
list; **the permissioned candidates themselves, including their key choices**;
Cardano's ledger for UTxO uniqueness, stake distribution, pool registration facts, and for
executing the validator as written.

Not trusted:
UTXO at validator address. Having Registration Policy token is the first validation.
Cross-chain and session-keys in the registration Datum. Cross-chain key proof-of-possesion is not
checked by Registration Policy nor session keys PoP are. Midnight has to validate them.

Out of scope: stake-weighted attacks on Ariadne itself; compromise of a validator's node keystore;
compromise of a pool's **cold key**, which is a Cardano-level loss whose blast radius far exceeds
Midnight candidacy.

R2 is implemented via separate (per-chain) Registration Smart Contract code / address.
Invoking it requires SPO signature of the different transaction.

---

## 4. On-chain design

### 4.1 The registration record

This is high level overview, details can vary, they will be specified later.
A registration is a UTxO (with registration token attached) at the candidates validator address whose datum contains.

| Field                   | Purpose                                                              |
| ----------------------- | -------------------------------------------------------------------- |
| `cold_pub_key`          | The pool's cold Ed25519 verification key — the pool's identity       |
| `pool_signature`        | That key's signature over the registration message                   |
| `cross_chain_pub_key`   | ECDSA key determining the candidate's `AccountId`                    |
| `cross_chain_signature` | Proof of possession of that key                                      |
| `keys`                  | Session keys, as a list of `(key-type-id, key bytes, proof)` triples |

#### Decoder can't fail

There was an error in legacy registrations code that caused a crash in Datum decoding and hypothetical
lose of livenss - no block production. This can happen in the node, even for UTXOs with Registration token (extra check over assumption A2 - we don't trust Minting Policy so much we allow its imperfection to allow block production halt).
Proper behavior in such a case is halt in registration processing with error logging - this means
policy failed or there is programmatic error in Registrations processing.

#### Key record shape

This one is specified as:

```rust
pub struct CandidateKey {
    pub id: KeyTypeId,     // 4 bytes
    pub bytes: Vec<u8>,    // the public key
    pub proof: Vec<u8>,    // proof of possession of it (§4.4)
}
```

Keeping a key next to its own proof means the two cannot be misaligned, cannot be of different
lengths, and cannot be reordered independently — the failure modes a parallel `key_proofs` list
would have invited. The list is bounded in length by the node's decode. Runtime will not allow
too long vectors in.
Limit of registration batch processed at once should practially be at few per block. This makes
passing their data to Runtime safe. There isn't much expected traffic left after initial filtering.
All signatures and proofs cover one canonical message:

```rust
pub struct RegistrationMessage {
    pub cold_pub_key: StakePoolPublicKey,  // 32-byte Ed25519
    pub cross_chain_pub_key: Vec<u8>,      // 33-byte compressed ECDSA
    pub keys: Vec<(KeyTypeId, Vec<u8>)>,   // canonically ordered, proofs dropped
}
```

**The message carries the key pairs, not the triples.** `proof` is projected out, which is what
keeps the construction non-circular: the message is fully
determined before any proof exists. Everything else in a triple is covered.

**`keys` must be canonically ordered** — ascending by `id`, with no duplicate `id` — and every
verifier re-serialises from canonical form rather than trusting the datum's ordering.

The runtime reconstructs these bytes to check the signature, so the serialisation is chosen to be
cheap and unambiguous to rebuild, and it follows the fixed Datum-to-bytes rules. Golden
tests over the encoding are what hold the signing side and the verifying side together.

### 4.2 Registration state machine

When registration first appears on Cardano it will be Active. Then registration can become Inactive by Cardano transaction (updated datum).
Inactive registration can become Active again.

In Midnight there is additional state of Invalid Registration. A new registration can appear as Active or Invalid (Invalid is Active from Cardano point of view, but invalid from Midnight point of view).
Inactive can be updated to Active or Invalid.

If configured in a way that observation of Cardano starts after some registration was deposited this registration is ignored.
Inactivation of registration that Midnight does not know about should not stop processing

### 4.3 Proving control of the stake pool

It is done on Cardano side in Registration policy. Midnight does no re-check it.

**Newest usage wins.** Minting policy check that no other registration exists for given
SPO key. Validator does not allow to spend UTXOs with Registration Token in a transaction other
than one that deposits UTXO with Active/Inactive Registration.

### 4.4 Proving possession of session keys

Done by Midnight node, by Runtime code, that knows required session keys set.
**ECDSA note.** Store full 65-byte recoverable signatures.

---

## 5. Observation

### 5.1 The event stream

The unit of observation is an output at the candidates validator address. We track only deposits
of UTXO that have active/inactive flag in Datum. Policy takes care of keeping Datum format
and SPO signatures. This means that we can have rather low limit on SQL/indexer lookup layer.

This lookup size limit is used by node, but consensus is sensitive on it.
It is the same limit of max new registrations (activations or deactivations, valid or not) passed to the runtime.
It means that for consensus, the node has to behave well. Which is nothing new.
Limit itself should be read from the runtime.

The pipeline uses limit to get UTXOs with a Registration Token from indexer/SQL layer.
If subsequent processing of any UTXO fails (Datum parsing), processing stops and never moves cursor.
Error is logged and bug has to be fixed. Block production does not stop.

If the processing ends up as Invalid Registration, it is still passed to the Runtime (as invalid).
Runtime function can be used to process each UTXO individually and then Inherent Data Provider
containing all of processed UTXOs is created.

The cursor is the total order `(block_no, tx_index_in_block)`. There is at most one Registration UTXO created per transaction.

### 5.5 Residual bounds

**The map holds at most one record per pool** — it is keyed by pool id — so the only storage bound that remains is on the number of pools:

1. **Across pools.** A bound on the size of the map, sized comfortably above Cardano's pool count
   so it never binds in practice. On overflow the pallet stops progress. It will fail on the same Cardano Tx each time unless some fix is introduced.
   It is called a fix, because under assumption of one entry per SPO it should not overflow.
2. **Decode bounds, in the node, as the only copy.** Keys and signatures are not checked by Policy,
   Decode bounded, then verify signatures. If invalid, write the message along Invalid state of
   registration.

#### Stake is read for committee selection only

Registration Policy allowing mint is enough for candidates registration. Their stake will be observed
by selection input. Selection IDP should therefore see registrations pallet state and the IDP that
is produced in the current block.

---

## 6. Runtime

### 6.1 Admission

Admission runs once, when an observed output is applied — not per epoch, and not per candidate.

#### Three rules decide what the map holds

They are stated together because each only makes sense with the other two.

1. **Newest attributed registration wins.** A registration **attributed to** pool `P` replaces
   `P`'s entry — even if "invalid on valid".
2. **Validity is decided once.** A registration is judged against the facts at the position where
   it was observed, and never judged again.
3. **Superseded is final.** A record that has been replaced is gone. Pallet keeps only the state resulting from the latest event.

### The checks

Per observed output, in order. **The order is a security property, not a presentational choice.**

**Phase A — free. No crypto, no state read, no state change.**

1. `keys` well-formed and canonical → else `NonCanonicalKeys`.

**Phase B — four proofs of possession, one ECDSA verification, and four index lookups.**

2. Every session key type present and proven → else `MissingKeyProof(id)`, `UnknownKeyType(id)`, or `InvalidKeyProof(id)`.
3. Cross-chain signature verifies → else `InvalidCrossChainSignature`.
4. No key collides with a key any other pool has ever proved → else `SessionKeyReuse`. Answered from
   by `set_keys` execution.

An invalid registration is stored - for user convenience, to see what is wrong.
Getting a Registration Token is reserved for SPOs, we can handle a record per SPO - regardless
it is malicious or not.

The session keys verification runs by check PoP over SCALE representation of a message
that contains the keys themselves. The set of required keys is current runtime session keys.
In the future it can be extended if Session Keys are going to change. It is out-of-scope
of the first version.

The implementation is a small match — `BABE` → sr25519, `GRANDPA` → ed25519, `BEEFY` → ecdsa (first implementation will require AURA also) —
each delegating to `ProofOfPossessionVerifier`. It is the single place that changes when a key
type is added or migrates scheme. The cross-chain key needs no
entry: `cross_chain_signature` over the same message already is a proof of possession for it.

In the implementation period we also use `AURA`, but it be removed before public release is made.

### 6.3 Collision resolution

Collision of cross-chain key cannot happen - Registration policy does not allow
two different Stake keys own same cross-chain key. This is our assumption.
TODO: investigate fallout of two registrations with the same cross-chain key and decide if pallet should store
mapping from cross-chain key to SPO key/pool id to protect downstream.

Stake key is identity of registration. If it changes, it is a different registration.

Combined: when new stake key registration appears, then cross-chain key is never seen before as well.

Session key collision (re-use): we use `set_keys` as indicator. It if passes, it means that downstream
will not fail as well. Otherwise registration is invalid due session key reuse.

**Registrations mapping is the one unbounded structure in the design, and the bound is a rate rather than a size.**
Cardano's pool count caps registrations count; the index of session keys has no equivalent cap,
because a pool can re-register with fresh keys indefinitely and each one is several more entries.
What limits an adversary is how fast they can get new proven keys onto Cardano: a transaction per
registration, Cardano's own throughput, and observation budget on top. That is slow — far
slower than the chain's own state grows in normal operation — so the growth is accepted rather
than capped. A cap would need an overflow rule, and every overflow rule here is either aimable at
an incumbent or a way to lock out a legitimate operator.

### 6.4 Registering keys in `pallet_session`

With this design in place `set_keys` is called for each registration and current code also
calls it for selected committee. `set_keys` is resilient to being called few times with
the same set. There is a known not-protected path leading to session keys collision.
Permissioned-candidates session keys are not checked for collision with registered candidates.
Permissioned-candidates are trusted and by assumption they do not collide with themselves.
Collision with registered candidates can happen when they lose one of private keys.
In such a case the best case scenario is that adversary (or the legitimate owner who registers despite being in permissioned candidates)
will have registration marked as invalid due session keys reuse.
The worst case scenario is committee rotation blocked until governance fixes the permissioned list.

### 6.5 Reaching committee selection

Ariadne needs, per candidate, an `AccountId`, a `SessionKeys`, and a weight. The first two are in
the map. The third is Cardano's and changes every epoch, so it is the only part that still has to
be observed.

**Registered candidates are read from storage, not carried in the inherent.** Selection runs
inside `select_authorities`, which the committee pallet calls from both `create_inherent` and
`check_inherent` — and both execute against _parent_ state. A proposer and a verifier therefore
read the same map, by construction. Sending the records through the inherent instead would hand
the verifier a list to compare against storage it already has, and would put a bound on how many
candidates can exist that the map does not otherwise need.

This is also what retires the question of how stale a candidate may be. The old snapshot path
needed registrations settled an epoch or two back, because each node queried its own db-sync and
agreement depended on them all seeing the same answer. Runtime state is consensus state: there is
nothing to settle, and **no cutoff is applied**. A pool is a candidate from the block its
registration is admitted in.

**Stake comes through the inherent, per epoch.** The node reads the pool ids at the parent hash —
the same state selection will run against — and asks db-sync for those pools' stake at
`data_epoch(epoch)`, the settled epoch the snapshot query used. The inherent carries
`(pool_id, stake)` pairs and nothing else about registered candidates.

The join is an **inner** one: a pool the stake list does not name is not a candidate this epoch.
Two cases produce that, and both are intended.

- A pool Cardano states no stake for. The stake gate (§4.4) asked that question once, at
  registration; this asks it again every epoch, because a seat should track stake even though
  validity does not. A pool that falls to zero keeps its registration and stops being selected.
- A pool admitted by the _same block's_ observation inherent, which was not in the map when the
  node built the stake list. It waits one selection. Weighting it zero would reach the same
  outcome by a longer route.
