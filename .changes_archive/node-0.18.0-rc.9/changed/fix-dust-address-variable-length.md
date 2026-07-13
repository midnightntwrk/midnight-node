#cnight-generates-dust
#client
#runtime
# Fix `DustPublicKey` to allow variable-lengths less than 33 bytes

DustPublicKey is variable-length encoded, with a maximum length of 33 bytes.
The MappingValidator and cNight inherents + pallet have been updated to respect
this.

PR: https://github.com/midnightntwrk/midnight-node/pull/297
Fixes: https://shielded.atlassian.net/browse/PM-20677
