# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Summarise `midnight::tx_budget` node logs from a load run.

The node emits one JSON line per applied transaction — user (`"k":"tx"`) and
system (`"k":"sys"`) alike — and one per block (`"k":"blk"`) when run with
`-lmidnight::tx_budget=debug`. This turns a run's worth of those lines into
an answer to "what is actually filling our blocks?": which of the five ledger
block limits binds, how much of it each transaction takes, and which aspect of a
transaction — proof verification, state application, sheer size — that share is
spent on.

    python3 scripts/tx-budget-report.py node.log
    docker logs midnight-node 2>&1 | python3 scripts/tx-budget-report.py -
    python3 scripts/tx-budget-report.py run/*.log.gz --csv per-tx.csv --json report.json

Lines may carry any log prefix (timestamp, level, target); everything before the
first `{` is ignored. A transaction that appears more than once — the authoring
node also imports the block it produced — is counted once, keyed on
(parent block, transaction hash).
"""

import argparse
import gzip
import json
import sys
from collections import Counter, defaultdict

# Ledger cost dimensions, in the order the node writes the `fb` / `fa` arrays.
DIMS = ["rt", "ct", "bu", "bw", "bc"]
DIM_NAMES = {
    "rt": "read_time",
    "ct": "compute_time",
    "bu": "block_usage",
    "bw": "bytes_written",
    "bc": "bytes_churned",
}
# Dimensions measured in picoseconds; the rest are byte counts.
TIME_DIMS = {"rt", "ct"}

# Substrate side of the budget, from `runtime/src/lib.rs`. A transaction's weight
# is `ledger_share * max_block + tx_size_weight`, and only NORMAL_RATIO of
# max_block is available to normal-class extrinsics.
DEFAULT_MAX_BLOCK_WEIGHT = 2_000_000_000_000  # 2s of ref_time, in picoseconds
DEFAULT_NORMAL_RATIO = 0.75
DEFAULT_TX_SIZE_WEIGHT = 20_000_000_000  # pallet_midnight::EXTRA_WEIGHT_TX_SIZE


def open_log(path):
    if path == "-":
        return sys.stdin
    if path.endswith(".gz"):
        return gzip.open(path, "rt", errors="replace")
    return open(path, "r", errors="replace")


def parse_lines(paths):
    """Yields the decoded tx_budget records found in the given logs."""
    for path in paths:
        with open_log(path) as handle:
            for line in handle:
                start = line.find('{"k":"')
                if start < 0:
                    continue
                try:
                    record = json.loads(line[start:])
                except ValueError:
                    continue
                if record.get("k") in ("tx", "sys", "blk"):
                    yield record


def percentile(values, fraction):
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, int(round(fraction * (len(ordered) - 1))))
    return ordered[index]


def mean(values):
    return sum(values) / len(values) if values else 0.0


def derive_limits(txs, blocks):
    """The block limits in force during the run.

    Block lines state them outright. Failing that (a truncated log with no
    end-of-block update in it), they are recovered from any transaction line:
    the node reports both a raw cost and its share of the limit, so the limit is
    their quotient.
    """
    for block in blocks:
        if "lim" in block:
            return {dim: block["lim"].get(dim, 0) for dim in DIMS}, "block lines"

    limits = {dim: 0 for dim in DIMS}
    for tx in txs:
        for dim in DIMS:
            raw, share = tx["c"].get(dim, 0), tx["s"].get(dim, 0.0)
            if raw and share:
                limits[dim] = max(limits[dim], int(round(raw / share)))
    return limits, "inferred from transaction shares (no block lines in input)"


def format_amount(value, dim):
    if dim in TIME_DIMS:
        return f"{value / 1e12:.6f} s"
    return f"{value:,} B"


def dedupe(records):
    """One entry per (block, transaction); later repeats are re-executions."""
    seen = {}
    for record in records:
        seen.setdefault((record.get("p"), record.get("tx")), record)
    return list(seen.values())


def aspect_rows(txs, limits):
    """Per-aspect totals across the run, keyed by aspect name."""
    totals = defaultdict(lambda: {"n": 0, "count": 0, **{dim: 0 for dim in DIMS}})
    for tx in txs:
        for aspect in tx.get("a", []):
            row = totals[aspect["n"]]
            row["n"] += 1
            row["count"] += aspect.get("q", 0)
            for dim in DIMS:
                row[dim] += aspect.get(dim, 0)
    return totals


def capacity(share):
    """How many transactions of this share fit in one block."""
    return (1.0 / share) if share > 0 else float("inf")


def report(txs, system, blocks, limits, limits_source, args, out):
    def line(text=""):
        print(text, file=out)

    line(
        f"tx-budget report — {len(txs):,} transactions, "
        f"{len(system):,} system transactions, {len(blocks):,} blocks"
    )
    line()

    line(f"Block limits ({limits_source})")
    for dim in DIMS:
        line(f"  {DIM_NAMES[dim]:<14} {format_amount(limits[dim], dim)}")
    line()

    line("Per-transaction share of the block budget")
    line(f"  {'dimension':<14} {'mean':>8} {'p50':>8} {'p95':>8} {'max':>8}  {'tx/block at p50':>15}")
    for dim in DIMS:
        shares = [tx["s"].get(dim, 0.0) for tx in txs]
        p50 = percentile(shares, 0.50)
        line(
            f"  {DIM_NAMES[dim]:<14} {mean(shares) * 100:>7.3f}% {p50 * 100:>7.3f}% "
            f"{percentile(shares, 0.95) * 100:>7.3f}% {max(shares, default=0) * 100:>7.3f}%  "
            f"{capacity(p50):>15,.1f}"
        )
    binding_shares = [tx["bs"] for tx in txs]
    p50_binding = percentile(binding_shares, 0.50)
    line(
        f"  {'BINDING':<14} {mean(binding_shares) * 100:>7.3f}% {p50_binding * 100:>7.3f}% "
        f"{percentile(binding_shares, 0.95) * 100:>7.3f}% "
        f"{max(binding_shares, default=0) * 100:>7.3f}%  {capacity(p50_binding):>15,.1f}"
    )
    line()

    line("Which dimension binds")
    binds = Counter(tx["bind"] for tx in txs)
    for dim, count in binds.most_common():
        line(f"  {DIM_NAMES[dim]:<14} {count / len(txs) * 100:>6.1f}%  ({count:,} transactions)")
    line()

    line("Where the budget goes — mean share of the block budget per transaction")
    header = f"  {'aspect':<32} {'per tx':>10}"
    for dim in DIMS:
        header += f" {DIM_NAMES[dim]:>13}"
    line(header)
    rows = aspect_rows(txs, limits)
    ordered = sorted(
        rows.items(),
        key=lambda item: max(
            item[1][dim] / limits[dim] if limits[dim] else 0.0 for dim in DIMS
        ),
        reverse=True,
    )
    for name, row in ordered:
        text = f"  {name:<32} {row['count'] / len(txs):>10,.2f}"
        for dim in DIMS:
            if not limits[dim] or not row[dim]:
                text += f" {'-':>13}"
                continue
            text += f" {row[dim] / limits[dim] / len(txs) * 100:>12.3f}%"
        line(text)
    line()

    if system:
        line("System transactions (applied by the runtime, same block budget)")
        per_variant = defaultdict(list)
        for record in system:
            for aspect in record.get("a", []):
                per_variant[aspect["n"]].append(record["bs"])
        line(f"  {'variant':<40} {'count':>8} {'mean share':>12} {'max share':>12}")
        for name, shares in sorted(per_variant.items(), key=lambda i: -mean(i[1])):
            line(
                f"  {name:<40} {len(shares):>8,} {mean(shares) * 100:>11.3f}% "
                f"{max(shares) * 100:>11.3f}%"
            )
        if blocks:
            line(f"  {len(system) / len(blocks):.2f} per block")
        line()

    if blocks:
        line("Block fill at the end of the block")
        line(f"  {'dimension':<14} {'mean':>8} {'p50':>8} {'p95':>8} {'max':>8}")
        for dim in DIMS:
            shares = [block["s"].get(dim, 0.0) for block in blocks]
            line(
                f"  {DIM_NAMES[dim]:<14} {mean(shares) * 100:>7.3f}% "
                f"{percentile(shares, 0.50) * 100:>7.3f}% "
                f"{percentile(shares, 0.95) * 100:>7.3f}% "
                f"{max(shares, default=0) * 100:>7.3f}%"
            )
        per_block = Counter(tx.get("p") for tx in txs)
        counts = [per_block.get(block.get("p"), 0) for block in blocks]
        line(
            f"  transactions   mean {mean(counts):.2f}  p50 {percentile(counts, 0.50):,}  "
            f"p95 {percentile(counts, 0.95):,}  max {max(counts, default=0):,}"
        )
        line()

    line("Substrate weight (the other ceiling)")
    normal_budget = args.max_block_weight * args.normal_ratio
    mean_ledger_weight = mean(binding_shares) * args.max_block_weight
    per_tx_weight = mean_ledger_weight + args.tx_size_weight
    line(f"  max_block                 {args.max_block_weight / 1e12:.3f} s ref_time")
    line(
        f"  normal-class budget       {normal_budget / 1e12:.3f} s "
        f"({args.normal_ratio * 100:.0f}% of max_block)"
    )
    line(
        f"  ledger share -> weight    {mean(binding_shares) * 100:.3f}% "
        f"= {mean_ledger_weight / 1e9:.1f} ms"
    )
    line(
        f"  flat per-tx size weight   {args.tx_size_weight / 1e9:.1f} ms "
        f"({args.tx_size_weight / args.max_block_weight * 100:.2f}% of max_block)"
    )
    line(f"  mean weight per tx        {per_tx_weight / 1e9:.1f} ms")
    if per_tx_weight > 0:
        weight_capacity = normal_budget / per_tx_weight
        ledger_capacity = capacity(mean(binding_shares))
        line(f"  fits per block (weight)   {weight_capacity:,.1f}")
        line(f"  fits per block (ledger)   {ledger_capacity:,.1f}")
        binds_first = "Substrate weight" if weight_capacity < ledger_capacity else "ledger limits"
        line(f"  binds first               {binds_first}")
    line()

    heaviest = sorted(txs, key=lambda tx: tx["bs"], reverse=True)[: args.top]
    if heaviest:
        line(f"Heaviest {len(heaviest)} transactions")
        line(f"  {'tx':<20} {'bytes':>8} {'bind':<14} {'share':>8}  top aspect")
        for tx in heaviest:
            aspects = tx.get("a", [])
            top = max(aspects, key=lambda a: a.get("s", 0.0)) if aspects else {"n": "-"}
            line(
                f"  {tx['tx'][:16]:<20} {tx.get('sz', 0):>8,} "
                f"{DIM_NAMES.get(tx['bind'], tx['bind']):<14} {tx['bs'] * 100:>7.3f}%  {top['n']}"
            )


def write_csv(txs, path):
    with open(path, "w") as handle:
        columns = ["tx", "parent", "tblock", "bytes", "bind", "block_share"]
        columns += [f"cost_{DIM_NAMES[dim]}" for dim in DIMS]
        columns += [f"share_{DIM_NAMES[dim]}" for dim in DIMS]
        handle.write(",".join(columns) + "\n")
        for tx in txs:
            row = [
                tx.get("tx", ""),
                tx.get("p", ""),
                str(tx.get("tb", "")),
                str(tx.get("sz", "")),
                tx.get("bind", ""),
                f"{tx.get('bs', 0.0):.9f}",
            ]
            row += [str(tx["c"].get(dim, 0)) for dim in DIMS]
            row += [f"{tx['s'].get(dim, 0.0):.9f}" for dim in DIMS]
            handle.write(",".join(row) + "\n")


def write_json(txs, system, blocks, limits, path):
    rows = aspect_rows(txs, limits)
    summary = {
        "transactions": len(txs),
        "system_transactions": len(system),
        "blocks": len(blocks),
        "limits": {DIM_NAMES[dim]: limits[dim] for dim in DIMS},
        "block_share": {
            "mean": mean([tx["bs"] for tx in txs]),
            "p50": percentile([tx["bs"] for tx in txs], 0.50),
            "p95": percentile([tx["bs"] for tx in txs], 0.95),
        },
        "binding": dict(Counter(tx["bind"] for tx in txs)),
        "aspects": {
            name: {
                "count_per_tx": row["count"] / len(txs) if txs else 0,
                "share_per_tx": {
                    DIM_NAMES[dim]: (
                        row[dim] / limits[dim] / len(txs) if limits[dim] and txs else 0.0
                    )
                    for dim in DIMS
                },
            }
            for name, row in rows.items()
        },
    }
    with open(path, "w") as handle:
        json.dump(summary, handle, indent=2)
        handle.write("\n")


def init_argparse():
    parser = argparse.ArgumentParser(
        description="Summarise midnight::tx_budget logs from a load run",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="Run the node with -lmidnight::tx_budget=debug to produce the input.",
    )
    parser.add_argument("logs", nargs="+", help="node log files ('-' for stdin, .gz accepted)")
    parser.add_argument("--csv", help="also write the per-transaction table here")
    parser.add_argument("--json", help="also write the machine-readable summary here")
    parser.add_argument(
        "--top", type=int, default=10, help="how many heaviest transactions to list"
    )
    parser.add_argument(
        "--max-block-weight",
        type=float,
        default=DEFAULT_MAX_BLOCK_WEIGHT,
        help="Substrate max_block ref_time, in picoseconds",
    )
    parser.add_argument(
        "--normal-ratio",
        type=float,
        default=DEFAULT_NORMAL_RATIO,
        help="fraction of max_block available to normal-class extrinsics",
    )
    parser.add_argument(
        "--tx-size-weight",
        type=float,
        default=DEFAULT_TX_SIZE_WEIGHT,
        help="flat per-transaction weight the midnight pallet adds, in picoseconds",
    )
    return parser


def main():
    args = init_argparse().parse_args()

    buckets = {"tx": [], "sys": [], "blk": []}
    for record in parse_lines(args.logs):
        buckets[record["k"]].append(record)

    txs = dedupe(buckets["tx"])
    system = dedupe(buckets["sys"])
    blocks = dedupe(buckets["blk"])
    if not txs:
        print("no tx_budget transaction lines found in the input", file=sys.stderr)
        return 1

    limits, limits_source = derive_limits(txs, blocks)
    report(txs, system, blocks, limits, limits_source, args, sys.stdout)

    if args.csv:
        write_csv(txs, args.csv)
    if args.json:
        write_json(txs, system, blocks, limits, args.json)
    return 0


if __name__ == "__main__":
    sys.exit(main())
