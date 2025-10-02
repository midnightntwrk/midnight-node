# Federated Authority Pallet

The `federated_authority` pallet implements a cross-collective governance mechanism. Its purpose is to enable a federation of distinct on-chain authority bodies to collectively approve a motion. Each participating body must first approve the motion individually, and only once all required approvals are collected will the motion be dispatched with elevated `Root` privileges. This creates a final checkpoint that requires consensus from multiple governance groups before any critical action can be executed.

The pallet is designed to be configurable so that a runtime can define:

- How many authority bodies participate in the federation.  
- Which specific collectives or governance groups those bodies represent.  
- The approval thresholds and voting mechanisms for each body.  
- The number of approvals required to dispatch a motion.  
- The lifetime of a motion before it expires.  

## The Motion Lifecycle

The lifecycle of a motion, from proposal to execution, is structured to ensure that every configured authority body independently agrees on a course of action.

### 1. Initiating a Motion
A motion is not created directly. Instead, one of the authority bodies signals its approval of a particular call.  

- The body conducts its own internal decision-making process (e.g., through a collective vote).  
- If its rules are satisfied, it dispatches a call to `federated_authority::motion_approve`, passing along the target call to be executed.  
- On the first approval of a new call, the pallet creates a motion entry in storage. That entry records which body approved it and sets an expiration period.  

### 2. Gathering Approvals
Once a motion is recorded, it is pending further approvals from the remaining authority bodies.  

- Each other body must go through its own internal process to approve the exact same call.  
- If they approve, they also dispatch `federated_authority::motion_approve`, which adds their approval to the motion.  

### 3. Executing or Closing a Motion
The `motion_close` extrinsic can be called by anyone to finalize a motion. A motion can only be closed if it has either been approved or has expired.

### 4. Revoking an Approval
The `motion_revoke` extrinsic allows an authority body to withdraw its approval before execution. If all approvals are revoked, the motion is immediately removed from storage.  

## Summary

In essence, the `federated_authority` pallet provides a **federated governance layer**, requiring independent approval from multiple on-chain bodies before a critical call can be executed with elevated privileges.  

The number of bodies, their internal voting rules, required approval proportions, and motion duration are all configurable by the runtime, making the pallet flexible enough to support various governance designs.
