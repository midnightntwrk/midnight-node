#audit #runtime
# Fix member association corruption when sorting authority lists

In `reset_members`, sort the paired `(AccountId, MainchainMember)` vectors by AccountId before calling `.unzip()` to preserve the positional correspondence. The previous approach sorted only the AccountId vector after unzipping, which corrupted the mapping between accounts and their mainchain identities in CouncilMainchainMembers storage, emitted events, and set_members_sorted calls.

PR: 
Ticket: https://shielded.atlassian.net/browse/PM-22086
