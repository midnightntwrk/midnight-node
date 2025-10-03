---
name: Node release checklist
about: Things to check when releasing node
title: Node release x.y.z
labels: ''
assignees: ''

---
---
**Release captain**: (tag a team member)

**Release branch**: (add release branch name)

----

- [ ] Start your release journey [HERE](https://shielded.atlassian.net/wiki/spaces/MN/pages/27929002/Node+Release+Process+WIP)

# Code checklist
 - [ ] Bump [runtime version](https://github.com/input-output-hk/midnight-substrate-prototype/blob/node-0.8.0-rc3/runtime/src/lib.rs#L234)
 - [ ] Bump [node version](https://github.com/input-output-hk/midnight-substrate-prototype/blob/node-0.8.0-rc3/node/Cargo.toml#L3)
- Run [subwasm](https://github.com/chevdor/subwasm) diff testnet-runtime new-runtime
  - [ ] Check runtimes are compatible
  - [ ] Bump [transaction version](https://github.com/input-output-hk/midnight-substrate-prototype/blob/c8861812ab5da19eb1a1253299e7b82919cc052c/runtime/src/lib.rs#L237) if needed
- Release PR has indexer passing on local env test
- The release includes a new Ledger version?
 - Need to regenerate genesis?
   - [ ] No
   - [ ] [Yes](https://shielded.atlassian.net/wiki/spaces/MN/pages/27992121/Runbook#Regenerate-Genesis)
- The release includes a new Partner Chains version?
  - [ ] No
  - [ ] Yes
    - [ ] `_from_env` methods are replaced by custom Midnight configuration inputs. [Example](https://github.com/midnightntwrk/midnight-node/pull/697/files)
    - [ ] `local-environment` works and is upgraded accordingly following Partner Chains matrix compatibility release notes.

# QA checklist
 - [ ] Deployed and burned in for 24h in qanet?
 - [ ] SPO env was also ok?
 - [ ] Approved by change process? (attach link to approved jira release here)
 - [ ] Do docs need updates?
 - [ ] Can it sync from genesis to testnet to at least 7000+ blocks? `sync-with-testnet.sh`

# Rollout checklist

 - [ ] Re-tag without `-rc` suffix: [retag release](https://github.com/input-output-hk/midnight-substrate-prototype/actions/workflows/release-image.yml)
 - [ ] Release to one node - [example patch](https://github.com/midnight-ntwrk/midnight-gitops/pull/1071)
 - [ ] Release to 1/3 of nodes - [example patch](https://github.com/midnight-ntwrk/midnight-gitops/pull/1072)
 - [ ] Release to all nodes (update image tag everywhere, remove kustomisation patch)
 - [ ] Release image to dockerhub: https://github.com/midnight-ntwrk/artifacts/actions/workflows/push-docker-images.yml
 - [ ] PR raised for updating https://github.com/midnight-ntwrk/midnight-node-docker ?
 - [ ] [Github Release](https://github.com/input-output-hk/midnight-substrate-prototype/releases) upgraded from 'pre-release' to 'released' status?
 - [ ] Announced on discord validators chat?
 - [ ] [Node support matrix](https://shielded.atlassian.net/wiki/spaces/MN/pages/27953053/Node+support+matrix) updated?
 - [ ] Translate Github release notes of current release to functional, user-facing notes and provide to documentation team.

# Post-release checklist

 - [ ] All code on release branch backported to `main`?
 - [ ] Runtime upgrade has been enacted? (at block: _)

(Reference: https://shielded.atlassian.net/wiki/spaces/MN/pages/27999401/Release+Playbook )
