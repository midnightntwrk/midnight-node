#toolkit
# `bridge-transfer` takes the cNIGHT token explicitly

`toolkit bridge-transfer` no longer reads the cNIGHT asset from `--ics-config` (the ICS config no
longer carries an `asset` field — the cNIGHT token is now defined once, in the cNight config).

The command's `--ics-config` flag is replaced by two explicit flags:
- `--cnight-asset-id <policy_id_hex>.<asset_name>` — the cNIGHT token, where the policy id is 56 hex
  chars (28 bytes) and the asset name is plain text (not hex); an empty asset name is `<hex>.`.
- `--ics-validator-address <bech32>` — the ICS validator address to send the bridged cNIGHT to.

PR:
Issue:
