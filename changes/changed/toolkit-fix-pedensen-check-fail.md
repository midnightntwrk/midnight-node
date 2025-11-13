#toolkit
# Fix PedersenCheckFailure when sending shielded transactions

Fixes an issue where the toolkit was adding an intent after calling `Transaction::new`

Binding randomness is computed in the `new` call, so changing randomness after this point will result in failures when verifying the transaction.

PR: 
Fixes: https://shielded.atlassian.net/browse/PM-20272
