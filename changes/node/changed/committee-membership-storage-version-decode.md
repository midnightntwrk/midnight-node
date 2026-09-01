#node #committee-selection #beefy
# Decode the committee by pallet storage version in the membership watcher

The committee-membership watcher called the `get_current_committee` runtime
API, which decodes committee members with the node binary's own `SessionKeys`.
Between updating the nodes and running the runtime upgrade, that meant decoding
pre-beefy (aura + grandpa) committee bytes as the three-key shape: the decode
falls short and yields the empty default, so the watcher logged every validator
as not being in a zero-size committee.

It now reads `CurrentCommittee` and `pallet-session-validator-management`'s
on-chain storage version straight from state and lets the runtime pick the
matching shape (`committee_keys_migrated` / `decode_current_committee`),
upgrading legacy-shaped members on the fly. Storage version `>= 2` means the
committee carries beefy keys; absent or lower means it does not.

PR: https://github.com/midnightntwrk/midnight-node/pull/1953
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
