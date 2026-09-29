#cnight-observation #logging
# Stop cNIGHT observation logging expected on-chain data as fatal errors

A registration whose DUST public key is out of range for the Fr field now logs the
skipped `CNightGeneratesDustEvent` at debug level instead of `Fatal:` at error. Any
other failure to construct the event logs a warning that the cNIGHT observation was
dropped, with the UTXO reference.

`handle_registration` no longer logs `fatal integrity error` when an address that
already has two or more registrations gains another; that is normal on-chain activity
and now logs at debug. Re-observing a registration UTXO that is already stored is
still logged at error, reworded to point at the cursor or data source.

No state or consensus change.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/1819
