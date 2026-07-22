# Node keys and keystore files

Keys and keystore of nodes in local-environment reflect few possible scenarios of AURA to BABE migration states.
All are setup as validators and are expect to produce blocks. In other words there are no invalid configurations.

- `midnight-node-1` is run as `--alice`, therefore it does not have `keystore` folder,
  its `keys` folder has keys generated with `//Alice` Secret Key UI,
  for this reason its `aura.vkey` and `babe.vkey` are equal (both are same scheme and same SURI)
- `midnight-node-2` is a node that does not have configured BABE key,
  see [midnight-setup](../../../../../res/local/permissioned-candidates-config.json) related file,
  this node should fall back to its AURA keys, keys of this node match `//Bob` SURI
- `midnight-node-3` does have BABE configured on Cardano (BABE equals AURA),
  this node is using seeds files (env variables `<key_type>_SEED_FILE` are used to set their paths) that are used to insert keys to keystore on the node startup,
  there is no BABE seed file (so it should fallback to AURA)
- `midnight-node-4` does not have BABE configured on Cardano, but it has BABE key in the keystore (which equals AURA key)
- `midnight-node-5` is a node that has both keys configured,

## permissioned-candidates-config.json

`midnight-setup` and `contracts-compiler` use `res/local/permissioned-candidates-config.json` file to set
chain-spec initial validators and on Cardano state respectively.
