#runtime #beefy

# Remove the BeefyStakesApi runtime API

`BeefyStakesApi` served the stake entries of the BEEFY payload, which now holds
only the MMR root. The API, its stake types and the runtime's stake helpers are
removed; `BeefyMmrApi` serves the committee commitments.
