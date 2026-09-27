#node #beefy #mock

# Mock candidates carry their BEEFY key

The mock candidates data source reads each candidate's required `beefy_pub_key`
and passes it on as the `beef` session key; before, it passed only the AURA and
GRANDPA keys, and a runtime that needs a BEEFY key found none. Every mock
registrations file carries `beefy_pub_key`; where it was missing it is the
candidate's `sidechain_pub_key`, the key the BEEFY session-key migration uses.
