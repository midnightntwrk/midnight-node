#runtime

# Record historical session roots

`pallet_session::historical` was wired into the runtime but never fed: the session manager was
`SessionCommitteeManagement` directly, so `HistoricalSessions` stayed empty and no validator could
prove membership in a past session. The manager is now wrapped in
`pallet_session::historical::NoteHistoricalRoot`, which stores each session's validator trie root
and count when the session is planned, and the committee pallet implements the historical session
manager with the unit identification, as upstream `pallet_staking` does.

Roots are recorded from the first rotation after the upgrade on; earlier sessions have none. The
consensus key-ownership-proof APIs are unchanged and still answer `None`, since equivocation
reporting stays disabled (`KeyOwnerProof = Void`). Nothing prunes the recorded roots yet; they
cost one hash and a count per session.
