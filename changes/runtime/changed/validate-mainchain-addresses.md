#runtime #cnight-observation #federated-authority-observation
# Validate Cardano addresses set for governance and cNIGHT observation

The council, technical-committee and mapping-validator address setters and genesis now accept
only a Bech32 enterprise script address. This makes sure the node can read the value back; it
does not check that the address points at the intended contract or network.

PR: https://github.com/midnightntwrk/midnight-node/pull/2370
