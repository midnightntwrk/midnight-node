# Federated Authority Governance System

## Overview

The Federated Authority Governance System is a comprehensive on-chain governance framework that enables decentralized decision-making through multiple governance bodies. The system bridges Cardano mainchain governance with the Midnight sidechain, allowing governance bodies defined on Cardano to control critical operations on the Midnight network.

## Two-Body Governance Model

The system employs two independent governance collectives—**Council** and **Technical Committee**—that must both approve critical actions before they can be executed. This federated approach provides checks and balances, ensuring that no single body can unilaterally control the network.

### How It Works:

1. **Independent Collectives:** Each governance body (Council and Technical Committee) operates as an independent collective with its own member set. Members can propose motions, vote on proposals, and close motions within their respective bodies.

2. **Internal Voting:** When a motion is proposed within a collective (e.g., Council proposes a runtime upgrade), members of that body vote on the proposal. A 2/3 majority is required for the motion to pass within that collective.

3. **Federated Approval:** Once a collective reaches its internal threshold (2/3 approval), it can register its approval with the federated authority pallet. However, this alone is not sufficient to execute the motion.

4. **Multi-Body Requirement:** For the motion to actually execute, **both** the Council **and** the Technical Committee must independently vote on and approve the **same proposal** (identified by the same motion hash). Each body must reach its 2/3 internal threshold and then register approval with the federated pallet.

5. **Unanimous Agreement:** The system currently requires unanimous approval between bodies (2 out of 2 bodies must approve) before execution can proceed. This means both Council and Technical Committee must approve the same call.

6. **Time Window:** Motions have a 5-day time window during which bodies can vote and register their approvals. If a motion doesn't receive sufficient approvals within this window, it expires and cannot be executed.

7. **Execution:** Once both bodies have approved the motion (unanimous inter-body approval), anyone can call `motion_close` to dispatch the approved call with Root privileges. The motion is then removed from storage.

### Example Flow:

```
Day 0: Council proposes and votes on "Upgrade Runtime to v2.0"
       → Council reaches 2/3 approval → Registers approval with federated pallet

Day 1: Technical Committee proposes and votes on same "Upgrade Runtime to v2.0"
       → TC reaches 2/3 approval → Registers approval with federated pallet
       → Both bodies have now approved (2/2)

Day 2: Anyone calls motion_close
       → Federated pallet checks: both bodies approved? Yes
       → Executes "Upgrade Runtime to v2.0" with Root origin
       → Motion removed from storage
```

This multi-step process ensures that significant governance actions receive thorough deliberation from multiple perspectives before execution, while the mainchain-synchronized membership ensures that the voting power remains anchored to Cardano's security.

## Architecture

The system consists of four main components:

1. **Governance Observation Layer** - Monitors and synchronizes governance membership from Cardano
2. **Governance Execution Layer** - Manages governance bodies and their memberships on-chain
3. **Motion Management Layer** - Handles proposal approval and execution through federated voting
4. **Runtime Integration** - Configures and coordinates all governance components

---

## 1. Governance Observation Layer

### Purpose

Observes and synchronizes governance body memberships from Cardano to Midnight, ensuring that governance authority remains anchored to the mainchain.

### Components

#### pallet-federated-authority-observation

**Location:** `pallets/federated-authority-observation/`

The observation pallet provides inherent-based synchronization of governance membership from Cardano UTXOs.

**Key Responsibilities:**

- Accepts federated authority data through inherents
- Validates and decodes sr25519 public keys from mainchain
- Updates Council and Technical Committee memberships
- Tracks mainchain member identifiers (Cardano public key hash)
- Emits events only when memberships change

**Storage:**

- `MainChainCouncilAddress` - Script address for Council members on Cardano
- `MainChainCouncilPolicyId` - Policy ID for Council members
- `MainChainTechnicalCommitteeAddress` - Script address for Technical Committee
- `MainChainTechnicalCommitteePolicyId` - Policy ID for Technical Committee
- `CouncilMainchainMembers` - Mainchain member identifiers for Council
- `TechnicalCommitteeMainchainMembers` - Mainchain member identifiers for Technical Committee

**Extrinsics:**

- `reset_members(council_authorities, technical_committee_authorities)` - Updates both governance bodies (inherent only, `RawOrigin::None`)
- `set_council_address(address)` - Updates Council contract address (root only)
- `set_council_policy_id(policy_id)` - Updates Council policy ID (root only)
- `set_technical_committee_address(address)` - Updates Technical Committee address (root only)
- `set_technical_committee_policy_id(policy_id)` - Updates Technical Committee policy ID (root only)

**Events:**

- `CouncilMembersReset { members, members_mainchain }` - Emitted when Council membership or mainchain members change
- `TechnicalCommitteeMembersReset { members, members_mainchain }` - Emitted when Technical Committee membership or mainchain members change

**Change Detection Logic:** The pallet independently tracks changes to:

1. Account member sets (sr25519 public keys)
2. Mainchain member sets (Policy IDs)

Events are emitted when *either* the account members or mainchain members change, allowing for:

- Pure account member rotation (same mainchain identifiers)
- Pure mainchain identifier updates (same account members)
- Combined updates (both change)

### Mainchain Follower Integration

#### Database Queries (`primitives/mainchain-follower/src/db/queries/`)

`get_governance_body_utxo()` - Queries db-sync for the most recent unspent UTXO containing governance membership data:

- Filters by script address and policy ID
- Returns UTXO datum containing member public keys
- Uses Cardano block number for historical lookups

#### Data Source (`primitives/mainchain-follower/src/data_source/`)

`FederatedAuthorityObservationDataSourceImpl` - Implements the data fetching logic:

- Retrieves governance UTXOs for both Council and Technical Committee
- Decodes Plutus datums containing member information
- Extracts sr25519 public keys (32 bytes) and Cardano public key hashes (28 bytes for Policy ID)
- Handles missing or invalid data gracefully with empty lists

**Datum Format (VersionedMultisig):**

The datum uses a `@list` annotation, structured as:

```
[
  [total_signers: Int, Map<CborBytes(32), Sr25519PubKey(32)>],  // Multisig (also @list)
  logic_round: Int
]
```

- **Outer list (VersionedMultisig):**
  - Index 0: `data` field - the Multisig structure
  - Index 1: `logic_round` field - round number (u8, 0-255)

- **Inner list (Multisig):**
  - Index 0: `total_signers`
  - Index 1: Members map

- **Members map:**
  - Keys: CBOR-encoded Cardano public key hash (32 bytes, first 4 bytes stripped to get 28-byte PolicyId)
  - Values: Sr25519 public keys (32 bytes)

#### Inherent Data Provider (`primitives/mainchain-follower/src/idp/`)

`FederatedAuthorityInherentDataProvider` - Prepares inherent data for block authoring:

- Fetches current addresses and policy IDs from runtime
- Queries data source for latest governance state
- Packages data into `FederatedAuthorityData` format
- Block authors include this data as an unsigned inherent

---

## 2. Governance Execution Layer

### Purpose

Manages governance body memberships and integrates with Substrate's collective pallet for voting functionality.

### Components

#### pallet-membership Instances

Two instances manage member lists:

- `CouncilMembershipInstance` (Instance1)
- `TechnicalCommitteeMembershipInstance` (Instance2)

**Configuration Highlights:**

- `ResetOrigin = EnsureNone<AccountId>` - Only inherents can update members
- `AddOrigin/RemoveOrigin/SwapOrigin = NeverEnsureOrigin` - Individual modifications disabled
- `MembershipInitialized/MembershipChanged = MembershipHandler` - Handles sufficient references
- `MaxMembers = 10` - Maximum members per body

**Key Behavior:**

- Members can only be set wholesale through `reset_members` inherent
- No individual add/remove operations allowed
- Ensures atomic updates from mainchain observations

#### pallet-collective Instances

Two instances provide proposal and voting functionality:

- `CouncilCollectiveInstance` (Instance1)
- `TechnicalCommitteeCollectiveInstance` (Instance2)

**Configuration Highlights:**

- `MotionDuration = 5 days` - Time window for voting
- `MaxProposals = 100` - Maximum concurrent proposals
- `MaxMembers = 10` - Must match membership pallet
- `DefaultVote = AlwaysNo` - Abstentions count as "no"
- `SetMembersOrigin = NeverEnsureOrigin` - Membership managed by pallet-membership

**Capabilities:** Each collective can:

- Propose runtime calls
- Vote on proposals
- Execute approved motions
- Close expired proposals

#### MembershipHandler Adapter

**Location:** `runtime/common/src/governance.rs`

Bridges membership changes with Substrate's account system:

```rust
pub struct MembershipHandler<T, P>(PhantomData<(T, P)>)
where
    T: frame_system::Config,
    P: InitializeMembers<T::AccountId> + ChangeMembers<T::AccountId>;
```

**Responsibilities:**

- Increments `sufficients` counter for incoming members (allows zero-balance accounts)
- Decrements `sufficients` counter for outgoing members
- Delegates to underlying collective pallet

This enables governance members to exist without token balances, as they don't pay fees for governance actions.

#### MembershipObservationHandler Adapter

**Location:** `runtime/common/src/governance.rs`

Provides the link between observation pallet and membership pallet:

```rust
pub struct MembershipObservationHandler<T, I>(PhantomData<(T, I)>);
```

**Key Functions:**

- `sorted_members()` - Returns current member list from pallet-membership
- `set_members_sorted()` - Dispatches `reset_members` call to pallet-membership

**Integration:**

- Used as `CouncilMembershipHandler` in observation pallet config
- Used as `TechnicalCommitteeMembershipHandler` in observation pallet config
- Ensures observation pallet can query and update membership atomically

---

## 3. Motion Management Layer

### Purpose

Implements federated voting where multiple governance bodies must approve actions, providing checks and balances.

### Component

#### pallet-federated-authority

**Location:** `pallets/federated-authority/`

A sophisticated motion management system enabling multi-body approval workflows.

**Core Concepts:**

**Authority Body:** A governance body that can approve motions, defined as:

```rust
AuthorityBody<Pallet, EnsureProportionAtLeast<2, 3>>
```

- `Pallet`: The collective instance (Council or TechnicalCommittee)
- `EnsureProportionAtLeast<2, 3>`: Requires 2/3 approval within that body

**Motion Lifecycle:**

1. **Proposal** - Any authorized body submits a motion
2. **Approval** - Bodies vote internally, then approve the motion if threshold reached
3. **Aggregation** - Federated pallet tracks approvals from different bodies
4. **Execution** - When enough bodies approve, motion executes
5. **Cleanup** - Motion removed after execution or expiration

**Storage:**

- `Motions<Hash, MotionInfo>` - Maps motion hashes to their state

**MotionInfo Structure:**

```rust
{
    approvals: BoundedBTreeSet<AuthId, MaxAuthorityBodies>,  // Which bodies approved
    ends_block: BlockNumber,                                  // Expiration time
    call: RuntimeCall,                                        // The action to execute
}
```

**Extrinsics:**

1. **`motion_approve(call)`** - An authority body approves a motion
   - Origin: `MotionApprovalOrigin` (collective with threshold met)
   - Creates motion if new, adds approval if existing
   - Returns the approving body's pallet index (`AuthId`)
   - Prevents duplicate approvals from same body
   - Rejects approvals for expired motions

2. **`motion_revoke(motion_hash)`** - An authority body revokes its approval
   - Origin: `MotionRevokeOrigin` (collective with threshold met)
   - Removes approval from motion
   - Deletes motion if all approvals revoked
   - Prevents revocation of expired motions

3. **`motion_close(motion_hash, proposal_weight_bound)`** - Anyone can attempt to close a motion
   - If approved (reached proportion): Executes call with Root origin, removes motion
   - If expired without approval: Marks as expired, removes motion
   - Cannot close ongoing (not expired) unapproved motions

**Events:**

- `MotionApproved { motion_hash, auth_id }` - Body approved motion
- `MotionDispatched { motion_hash, motion_result }` - Motion executed (includes success/failure)
- `MotionExpired { motion_hash }` - Motion expired without sufficient approvals
- `MotionRevoked { motion_hash, auth_id }` - Body revoked approval
- `MotionRemoved { motion_hash }` - Motion cleaned up

**Configuration Parameters:**

- `MaxAuthorityBodies` - Maximum number of governance bodies (typically 2: Council + TC)
- `MotionDuration` - How long motions remain open (5 days)
- `MotionApprovalProportion` - Required proportion for execution (e.g., 1/1 = unanimous)

**Origin Validation:**

The pallet uses custom origin managers to validate collective approvals:

**`FederatedAuthorityOriginManager<(CouncilApproval, TechnicalCommitteeApproval)>`**

- Checks if origin comes from an authorized collective
- Returns the pallet index (`AuthId`) on success
- Used for both approval and revocation

**`AuthorityBody<Council, EnsureProportionAtLeast<2, 3>>`**

- Ensures call comes from Council collective
- Requires 2/3 of Council members approved the proposal
- Returns Council pallet index (40) as `AuthId`

**`AuthorityBody<TechnicalCommittee, EnsureProportionAtLeast<2, 3>>`**

- Ensures call comes from Technical Committee collective
- Requires 2/3 of TC members approved the proposal
- Returns TC pallet index (42) as `AuthId`

**Approval Proportion Logic:**

```rust
pub struct FederatedAuthorityEnsureProportionAtLeast<const N: u32, const D: u32>;

fn reached_proportion(n: u32, d: u32) -> bool {
    n * D >= N * d
}
```

Example: `FederatedAuthorityEnsureProportionAtLeast<1, 1>`

- Requires: `approvals * 1 >= 1 * total_bodies`
- With 2 bodies: `2 * 1 >= 1 * 2` → requires 2/2 (unanimous)

---

## 4. Runtime Integration

### Configuration

**Location:** `runtime/src/lib.rs`

#### Collective Pallets

```rust
/// Council (Instance1)
type CouncilCollectiveInstance = pallet_collective::Instance1;

/// Technical Committee (Instance2)
type TechnicalCommitteeCollectiveInstance = pallet_collective::Instance2;
```

Both configured identically with:

- 5-day motion duration
- 100 max proposals
- 10 max members
- "Always No" default vote
- Root origins for administrative actions

#### Membership Pallets

```rust
type CouncilMembershipInstance = pallet_membership::Instance1;
type TechnicalCommitteeMembershipInstance = pallet_membership::Instance2;
```

Both configured with:

- `ResetOrigin = EnsureNone` (inherent-only updates)
- `MembershipInitialized/Changed = MembershipHandler` (manages sufficient references)
- All other origins disabled (no individual modifications)

#### Federated Authority Observation

```rust
impl pallet_federated_authority_observation::Config for Runtime {
    type CouncilMaxMembers = ConstU32<MAX_MEMBERS>;
    type TechnicalCommitteeMaxMembers = ConstU32<MAX_MEMBERS>;
    type CouncilMembershipHandler =
        MembershipObservationHandler<Runtime, CouncilMembershipInstance>;
    type TechnicalCommitteeMembershipHandler =
        MembershipObservationHandler<Runtime, TechnicalCommitteeMembershipInstance>;
}
```

Links observation pallet to membership pallets through `MembershipObservationHandler`.

#### Federated Authority Motion Management

```rust
impl pallet_federated_authority::Config for Runtime {
    type MotionCall = RuntimeCall;
    type MaxAuthorityBodies = ConstU32<2>;   // Council + TC
    type MotionDuration = ConstU32<MOTION_DURATION>;  // 5 days
    type MotionApprovalProportion = FederatedAuthorityEnsureProportionAtLeast<1, 1>;
    type MotionApprovalOrigin =
        FederatedAuthorityOriginManager<(CouncilApproval, TechnicalCommitteeApproval)>;
    type MotionRevokeOrigin =
        FederatedAuthorityOriginManager<(CouncilRevoke, TechnicalCommitteeRevoke)>;
}
```

**Approval/Revoke authorities:**

```rust
type CouncilApproval = AuthorityBody<
    Council,
    pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollectiveInstance, 2, 3>
>;

type TechnicalCommitteeApproval = AuthorityBody<
    TechnicalCommittee,
    pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCommitteeCollectiveInstance, 2, 3>
>;
```

Requires 2/3 approval within each body, then unanimous approval between bodies (1/1) for execution.

---

## Complete Workflow

### 1. Membership Synchronization (Mainchain → Sidechain)

```
Cardano UTXO (Governance Membership)
                ↓
db-sync Database (SQL queries)
                ↓
FederatedAuthorityObservationDataSourceImpl (decode Plutus datum)
                ↓
FederatedAuthorityInherentDataProvider (prepare inherent data)
                ↓
Block Author (include inherent in block)
                ↓
pallet-federated-authority-observation::reset_members (inherent execution)
                ↓
MembershipObservationHandler::set_members_sorted
                ↓
pallet-membership::reset_members (update member list)
                ↓
MembershipHandler::change_members_sorted (manage sufficient references)
                ↓
pallet-collective (updated membership for voting)
```

**Key Points:**

- Runs every block (inherent data always included)
- Events only emitted when membership actually changes
- Atomic update ensures consistency
- Zero-downtime membership rotation

### 2. Motion Approval Flow

```
Council Proposal
        ↓
Council Internal Vote (2/3 threshold)
        ↓
pallet-federated-authority::motion_approve (creates/approves motion)
        ↓ (approval tracked)
Technical Committee Proposal (same motion hash)
        ↓
TC Internal Vote (2/3 threshold)
        ↓
pallet-federated-authority::motion_approve (second approval)
        ↓ (both bodies approved, proportion reached)
Anyone calls motion_close
        ↓
pallet-federated-authority::motion_close (checks proportion met)
        ↓
Motion Dispatched (Root origin)
        ↓
Call Executed
        ↓
Motion Removed
```

**Key Points:**

- Each body votes independently via pallet-collective
- Federated pallet aggregates approvals
- Requires 2/3 within each body, then unanimous (2/2) between bodies
- 5-day time window for approvals
- Executed with Root privileges

---

## Proposing and Approving a Motion

1. **Council proposes in pallet-collective**

   ```
   Council.propose(motion, lengthBound)
   ```

2. **Council members vote**

   ```
   Council.vote(proposalHash, index, approve)
   ```

3. **Council closes proposal (if 2/3 reached)**

   ```
   Council.close(proposalHash, index, proposalWeightBound, lengthBound)
   ```

4. **Collective calls federated pallet (automatically)**

   ```
   FederatedAuthority.motion_approve(call)
   ```

5. **Technical Committee repeats steps 1-4** (with same call)

6. **Anyone closes motion**

   ```
   FederatedAuthority.motion_close(motionHash, proposalWeightBound)
   ```

7. **Motion executes with Root origin**

---

## Monitoring Governance

### Key Events to Watch:

**Membership Changes:**

- `FederatedAuthorityObservation.CouncilMembersReset`
- `FederatedAuthorityObservation.TechnicalCommitteeMembersReset`

**Motion Lifecycle:**

- `FederatedAuthority.MotionApproved`
- `FederatedAuthority.MotionRevoked`
- `FederatedAuthority.MotionDispatched`
- `FederatedAuthority.MotionExpired`

**Collective Actions:**

- `Council.Proposed`
- `Council.Voted`
- `Council.Executed`
- `TechnicalCommittee.Proposed`
- `TechnicalCommittee.Voted`
- `TechnicalCommittee.Executed`

### Storage to Query:

**Current Members:**

```
CouncilMembership.Members
TechnicalCommitteeMembership.Members
```

**Mainchain Identifiers:**

```
FederatedAuthorityObservation.CouncilMainchainMembers
FederatedAuthorityObservation.TechnicalCommitteeMainchainMembers
```

**Active Motions:**

```
FederatedAuthority.Motions(hash) → MotionInfo
```
