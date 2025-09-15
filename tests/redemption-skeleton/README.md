# Skeleton for the Glacier Drop Redemption contract

The contract has the same data structure as the real redemption contract and run some validation, but ignores all the timestamp fields.

It's imported by the localenv e2e tests for cNight Generates Dust integration testing.

To re-build it, run `earthly +rebuild-redemption-skeleton` from the root of the repo.
