#cnight
# Fix unbounded allocation in cNight Observation pallet

We were using an unbounded vec to store all cNight mappings for a given user. Replaced this with two new storage maps - inserting and removing mappings is now O(1) in space and time.

PR: 
Issue: https://github.com/midnightntwrk/midnight-security/issues/116
