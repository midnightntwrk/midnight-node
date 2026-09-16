#node #test
# Interleave the batch-verify A/B benchmark and report a paired statistic

`benchmark.sh` ran every OFF repeat and then every ON repeat, which made run order
a confound with the thing being measured: the second config always ran later, on a
hotter machine with a warmer page cache. Over a ~2 minute run that drift is around
a second — comparable to the effect — and it loaded entirely onto ON, producing a
clean-looking, reproducible and entirely spurious "batch verification is slower"
signal.

The two configs now alternate within each repeat, with the order inside the pair
flipped every repeat, so drift is split evenly and the advantage of going first
cancels. The report adds the paired deltas, their median, and a sign test; the old
"spread >= delta" warning has been dropped, because comparing unpaired spreads
discards the pairing and reports "unresolved" on data that is in fact unanimous.

The "not resolved" warning now fires on the sign test's p-value rather than on the
pairs disagreeing. Unanimity is the wrong criterion in both directions: with enough
pairs a few disagreements are expected and the result is still decisive (17/21 is
p = 0.004), while three pairs agreeing establishes very little (p = 0.125).

Two further harness fixes:

- `CHAIN` is read back from the archive meta. It defaulted to the built-in `dev`
  spec, so an archive primed against a custom genesis was silently unusable: the
  producer simply authored a different chain from its own genesis and the syncer
  never reached the target height.
- A run far from the median (the syncer occasionally spends a minute finding the
  producer peer before importing anything) is flagged, so a swamped mean is not
  read as a result.

PR: <link to PR>
