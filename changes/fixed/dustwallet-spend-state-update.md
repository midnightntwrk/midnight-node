---
title: Fix DustWallet spend state propagation
issue: PM-20016
---

Fix `DustWallet::spend` to propagate updated local state from `do_spend`
back into `self.dust_local_state`, preventing stale-state bugs where
consecutive spends may select already-spent outputs.

Addresses Least Authority audit finding Issue AO.
