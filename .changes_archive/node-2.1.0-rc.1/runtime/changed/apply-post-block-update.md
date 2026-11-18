#runtime

# Use `apply_post_block_update` in `pallet_midnight` `on_finalize`

This change makes `on_finalize` use function that has theoretically one way less to fail.
In practice the error couldn't happen becase block fullness is checked for each included transaction.
