#bridge #c2m
# Keep subminimal-transfer accumulator when the flush system tx fails

Previously the C2M bridge cleared the accumulated subminimal-transfer state
unconditionally after attempting a flush. If constructing or executing the
`unlock_to_treasury` system transaction failed (e.g. block full), the
accumulated cNIGHT total was silently discarded and the funds were never
credited. The accumulator is now retained on failure so the next subminimal
transfer retries the flush with the full amount.

PR: TBD
