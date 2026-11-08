#toolkit

# Do not create genesis state with Reserve having less than in reserve-config.json

Before this change, when "genesis seeds" and "funding args" were used, it was common that
genesis state had Reserve pool smaller than what is declared in the config file.
Genesis creation will still use Midnight mechanism of paying block rewards transactions
from reserve pool to feed seeds wallets, but it will account it when creating reserve pool
before payouts

Issue: https://github.com/midnightntwrk/midnight-node/issues/1674
PR: https://github.com/midnightntwrk/midnight-node/pull/1791
