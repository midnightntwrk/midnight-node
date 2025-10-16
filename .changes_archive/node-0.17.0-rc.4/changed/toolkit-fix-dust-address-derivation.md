#toolkit
# Fix dust wallet address derivation

The toolkit was using the wrong method to generate DUST addresses from seeds or mnemonic phrases, causing it to generate addresses which other components don't recognize.

PR: https://github.com/midnightntwrk/midnight-node/pull/93