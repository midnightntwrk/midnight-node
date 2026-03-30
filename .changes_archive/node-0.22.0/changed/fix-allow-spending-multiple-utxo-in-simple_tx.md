#toolkit
# Fix simple_tx panic when funding wallet has enough funds but not in one UTXO

Fixes toolkit panic when simple-tx needs to spend more than biggest UTXO of funding UTXO.
Also prevents high evaluation time moving offer to fallible if there are more than one input.

PR: https://github.com/midnightntwrk/midnight-node/pull/782
Ticket: https://shielded.atlassian.net/browse/PM-21914
