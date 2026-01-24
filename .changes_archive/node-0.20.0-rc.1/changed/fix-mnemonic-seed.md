# Fix one seed used in a local undeployed environment

Undeployed configuration is supposed to assign some initial tokens to a wallet 
initialized from a mnemonic `abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon diesel`,
but the seed used in configuration was invalid, leading to assingments to 
wrong addresses. Now this is fixed.


PR: https://github.com/midnightntwrk/midnight-node/pull/363