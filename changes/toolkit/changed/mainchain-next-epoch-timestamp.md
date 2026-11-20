#toolkit #rpc
# Fix Cardano next epoch timestamp in sidechain_getStatus

`sidechain_getStatus` returned `mainchain.nextEpochTimestamp` too late on any Cardano network
whose first Shelley epoch is not 0. `mainchain_epoch_to_timestamp` multiplied the epoch duration
by the full epoch number instead of the epochs since `first_epoch_number`. On mainnet
(first epoch 208) the timestamp was 208 epochs, about 2.8 years, late; on preprod (first
epoch 4) it was 20 days late. Preview was not affected.
