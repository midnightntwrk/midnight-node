#node #runtime
# Single source for the cNIGHT token definition

The cNIGHT token (policy id + asset name) was previously duplicated across `cnight-config.json`,
`ics-config.json`, and `reserve-config.json` (and the `ics-addresses.json` / `reserve-addresses.json`
inputs), allowing the three to drift out of sync. The cNIGHT token is now defined in exactly one
place — `cnight-addresses.json` (`cnight_policy_id` / `cnight_asset_name`) — and everything else
references it.

- Removed the `asset` field from `IcsConfig`, `IcsAddresses`, `ReserveConfig`, `ReserveAddresses`
  (and the now-unused `IcsAsset` / `ReserveAsset` types), and from the corresponding JSON files.
- `generate-ics-genesis` / `generate-reserve-genesis` now read the cNIGHT policy id and asset name
  from `cnight-addresses.json` when querying db-sync for locked UTxOs.
- Chain-spec generation derives the bridge `BridgeMainChainScripts.token_policy_id` /
  `token_asset_name` from the cNight config instead of the ICS config.

PR:
Issue:
