#runtime #migration #committee-selection

# Merge committee v1-to-v2 migration with the BABE session-key migration

`MigrateV1ToV2AddBabeSessionKeys` is provided and it combines `pallet-session-validator-management`
`V1ToV2Migration` and migration of session keys. Partner-chains `V1ToV2Migration` is not ready
to be run together with session keys change, so we use a custom migration that does both.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
