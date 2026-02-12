#node #memory
# Reduce steady-state memory footprint on fully synced validators

Heaptrack profiling and production monitoring show that fully synced validators
exhibit linear memory growth over time, eventually approaching memory limits.
The previous fix (storage_cache_size bounded LRU) addressed the dominant allocator
during chain sync, but several other memory sources continue growing on synced nodes.

Tuned for an 8 GiB memory budget (~1.2 GiB baseline, ~6.8 GiB headroom):

1. **Substrate trie cache (1 GiB → 128 MiB):** The trie cache fills gradually and
   never shrinks. 128 MiB is sufficient for 6-second block validators.

2. **WASM runtime instances (8 → 2):** Each pooled instance reserves 128 MiB of
   heap that is allocated on demand and never released. Default of 8 = up to 1 GiB.
   Validators only need 1–2 concurrent instances.

3. **Midnight ledger arena cache (1M → 512K nodes):** Halved to ~40 MiB resident.
   Synced validators rarely need more; sync takes a minor cache-miss penalty.

4. **Transaction validation caches (1000 → 200 entries, add 5-minute TTI):** The
   moka caches for VerifiedTransaction objects had no time-based eviction and an
   oversized capacity. Each entry holds ZK proof data (50–200 KiB). Reduced capacity
   and added time_to_idle eviction to prevent stale entries from accumulating.

5. **Transaction pool (8192 → 1024):** Low-traffic validator networks don't need
   8192 pooled transactions.

Combined savings: ~1.8 GiB reduction in steady-state memory per validator.
