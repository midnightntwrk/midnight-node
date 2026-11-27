#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
"""The hard fork test report of one environment, generated from its results.

  generate.py <results dir> [--out <dir>] [--tested-by <name>] [--date <text>]

Reads results/G*.tsv and SETUP.tsv (one row per check), results/context.json (what was
tested) and state/state.env (the fork block). Writes REPORT.md and report.html: the verdict,
the environment, coverage by runbook group and bullet, failures, known issues, observations,
what did not run, and every check.
"""
import argparse
import csv
import html
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

GROUPS = {
    "SETUP": "Setting up the network",
    "G1": "1. Release artefacts verification",
    "G2": "2. Old-ledger features before the fork",
    "G3": "3. Smoke test after each binary wave",
    "G4": "4. Fork block integrity",
    "G5": "5. Ledger state migration",
    "G6": "6. Pre-fork state still works after the fork",
    "G7": "7. New-ledger features after the fork",
    "G8": "8. Outdated and invalid inputs are refused",
    "G9": "9. Indexer, wallets and SDKs across the fork",
    "G10": "10. Failure and recovery drills",
}
GROUPS_AFTER_FORK = {"G5", "G6", "G7", "G8", "G9"}
COLUMNS = ["group", "check", "status", "bullet", "title", "detail"]
# How bad a status is, worst first.
SEVERITY = ["FAIL", "KNOWN", "WARN", "PASS", "SKIP"]
SECTIONS = [
    ("FAIL", "Failures", "Checks that did not hold."),
    ("KNOWN", "Known issues", "Limitations filed upstream; each detail names its issue."),
    ("WARN", "Observations", "Checks that ran and saw something unexpected that is not a defect of the fork."),
    ("SKIP", "Not run", "Checks that did not run on this environment, with the reason."),
]
ISSUE = re.compile(r"\b(midnight-(?:node|indexer|ledger|wallet|js)|node|indexer|ledger|wallet)#(\d+)")


def load_checks(results):
    checks = []
    for group in GROUPS:
        table = results / f"{group}.tsv"
        if not table.is_file():
            continue
        with table.open(newline="") as handle:
            for row in csv.DictReader(handle, delimiter="\t", quoting=csv.QUOTE_NONE):
                if all(row.get(column) is not None for column in COLUMNS):
                    checks.append(row)
    return checks


def remembered(state_file, key):
    if not state_file.is_file():
        return None
    for line in state_file.read_text().splitlines():
        if line.startswith(f"{key}="):
            return line.split("=", 1)[1].strip("'\"")
    return None


def worst(statuses):
    return min(statuses, key=SEVERITY.index)


def tally(checks):
    counts = {status: sum(1 for check in checks if check["status"] == status) for status in SEVERITY}
    return ", ".join(f"{counts[status]} {status.lower()}" for status in SEVERITY if counts[status])


def issue_url(match):
    repository = match.group(1) if match.group(1).startswith("midnight-") else f"midnight-{match.group(1)}"
    return f"https://github.com/midnightntwrk/{repository}/issues/{match.group(2)}"


class Report:
    def __init__(self, results, tested_by, date):
        self.checks = load_checks(results)
        if not self.checks:
            sys.exit(f"no results in {results}")
        context_file = results / "context.json"
        self.context = json.loads(context_file.read_text()) if context_file.is_file() else {}
        self.fork_block = remembered(results.parent / "state" / "state.env", "FORK_HEIGHT")
        self.tested_by = tested_by
        self.date = date or datetime.now(timezone.utc).strftime("%d %B %Y").lstrip("0")
        self.old, self.new = self.context.get("old_ledger", "X"), self.context.get("new_ledger", "X+1")
        network = self.context.get("network") or "local"
        self.network = "local-env" if network == "local" else network
        self.counts = {status: sum(1 for check in self.checks if check["status"] == status) for status in SEVERITY}
        self.groups = [group for group in GROUPS if any(check["group"] == group for check in self.checks)]

    def with_status(self, status):
        return [check for check in self.checks if check["status"] == status]

    def verdict(self):
        """(kind, headline): kind is pass, known, partial or fail."""
        fork = f"ledger {self.old} to ledger {self.new} hard fork"
        failures = self.counts["FAIL"]
        failing = "1 check fails" if failures == 1 else f"{failures} checks fail"
        ran_after_fork = any(check["group"] in GROUPS_AFTER_FORK and check["status"] == "PASS" for check in self.checks)
        if not self.fork_block:
            headline = f"{self.network} before the fork: the fork block is not confirmed"
            return ("fail", f"{headline}; {failing}") if failures else ("partial", headline)
        if failures:
            return "fail", f"The {fork} on {self.network}: {failing}"
        if not ran_after_fork:
            return "partial", f"{self.network} forked; the checks after the fork have not run yet"
        if self.counts["KNOWN"]:
            return "known", f"{self.network} completed the {fork}, with known issues"
        return "pass", f"{self.network} completed the {fork}"

    def summary(self):
        counts = self.counts
        sentences = [f"The runtime upgrade took effect at block {self.fork_block}."] if self.fork_block else []
        sentences.append(f"{len(self.checks)} checks ran in {len(self.groups)} groups: {counts['PASS']} pass, {counts['FAIL']} fail, "
                         f"{counts['KNOWN']} are known issues, {counts['WARN']} are observations and {counts['SKIP']} did not run.")
        return " ".join(sentences)

    def environment(self):
        context = self.context
        rows = [("Target", context.get("target_description") or self.network)]
        if context.get("old_ref"):
            rows.append(("Before the fork", f"{context['old_ref']} ({context.get('old_node_image', '')})"))
        if context.get("new_ref"):
            rows.append(("After the fork", f"{context['new_ref']} ({context.get('new_node_image', '')})"))
        if self.fork_block:
            rows.append(("Fork block", self.fork_block))
        for key, label in [("indexer", "Indexer"), ("proof_servers", "Proof servers"), ("clients", "Client libraries"),
                           ("suite_commit", "Test suite commit")]:
            if context.get(key):
                rows.append((label, context[key]))
        if self.tested_by:
            rows.append(("Tested by", self.tested_by))
        return rows

    def coverage(self):
        """One row per runbook bullet: (group title, bullet, worst status of what ran, tally)."""
        rows = []
        for group in self.groups:
            in_group = [check for check in self.checks if check["group"] == group]
            for bullet in dict.fromkeys(check["bullet"] for check in in_group):
                under_bullet = [check for check in in_group if check["bullet"] == bullet]
                ran = [check["status"] for check in under_bullet if check["status"] != "SKIP"]
                rows.append((GROUPS[group], bullet or "-", worst(ran) if ran else "SKIP", tally(under_bullet)))
        return rows

    # --- Markdown ---------------------------------------------------------------------
    def markdown(self):
        link = lambda text: ISSUE.sub(lambda m: f"[{m.group(0)}]({issue_url(m)})", text).replace("|", "\\|")
        counts = self.counts
        lines = [f"# {self.verdict()[1]}", "", f"_Hard fork test report, {self.date}_", "", self.summary(), "",
                 " · ".join(f"**{counts[status]} {status.lower()}**" for status in ["PASS", "FAIL", "KNOWN", "WARN", "SKIP"]), "",
                 "| | |", "|---|---|"]
        lines += [f"| {label} | {link(value)} |" for label, value in self.environment()]
        lines += ["", "## Coverage against the runbook", "", "| Group | What to test | Result | Checks |", "|---|---|---|---|"]
        lines += [f"| {group} | {link(bullet)} | **{status.title()}** | {counted} |" for group, bullet, status, counted in self.coverage()]
        for status, title, intro in SECTIONS:
            found = self.with_status(status)
            if found:
                lines += ["", f"## {title}", "", intro, "", "| Check | What it checks | Detail |", "|---|---|---|"]
                lines += [f"| {check['check']} | {link(check['title'])} | {link(check['detail'])} |" for check in found]
        lines += ["", "## Every check"]
        for group in self.groups:
            lines += ["", f"### {GROUPS[group]}", "", "| Check | Status | What it checks | Detail |", "|---|---|---|---|"]
            lines += [f"| {check['check']} | {check['status']} | {link(check['title'])} | {link(check['detail'])} |"
                      for check in self.checks if check["group"] == group]
        return "\n".join(lines) + "\n"

    # --- HTML -------------------------------------------------------------------------
    def html(self):
        def text(value):
            return ISSUE.sub(lambda m: f'<a href="{issue_url(m)}">{m.group(0)}</a>', html.escape(value or ""))

        def table(headers, rows):
            head = "".join(f"<th>{html.escape(header)}</th>" for header in headers)
            body = "".join("<tr>" + "".join(f"<td>{cell}</td>" for cell in row) + "</tr>" for row in rows)
            return f"<div class=scroll><table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table></div>"

        def chip(status):
            return f'<span class="chip {status.lower()}">{status.title()}</span>'

        kind, headline = self.verdict()
        counts = self.counts
        parts = [f'<header class="{kind}"><p class=kicker>Hard fork test report, {html.escape(self.date)}</p>',
                 f"<h1>{html.escape(headline)}</h1><p>{html.escape(self.summary())}</p>",
                 "<p>" + " ".join(f"{chip(status)} {counts[status]}" for status in ["PASS", "FAIL", "KNOWN", "WARN", "SKIP"]) + "</p></header>",
                 table(["", ""], [(html.escape(label), text(value)) for label, value in self.environment()]),
                 "<h2>Coverage against the runbook</h2>",
                 table(["Group", "What to test", "Result", "Checks"],
                       [(html.escape(group), text(bullet), chip(status), html.escape(counted)) for group, bullet, status, counted in self.coverage()])]
        for status, title, intro in SECTIONS:
            found = self.with_status(status)
            if found:
                parts += [f"<h2>{title}</h2><p>{intro}</p>",
                          table(["Check", "What it checks", "Detail"],
                                [(f"<code>{html.escape(check['check'])}</code>", text(check["title"]), text(check["detail"])) for check in found])]
        parts.append("<h2>Every check</h2>")
        for group in self.groups:
            parts += [f"<h3>{html.escape(GROUPS[group])}</h3>",
                      table(["Check", "Status", "What it checks", "Detail"],
                            [(f"<code>{html.escape(check['check'])}</code>", chip(check["status"]), text(check["title"]), text(check["detail"]))
                             for check in self.checks if check["group"] == group])]
        return PAGE.replace("__TITLE__", html.escape(headline)).replace("__BODY__", "\n".join(parts))


PAGE = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>__TITLE__</title>
<style>
:root { --text: #1a1a1a; --muted: #5c5c5c; --line: #dcdcdc; --surface: #f6f6f6; --page: #ffffff;
        --pass: #1a7f37; --fail: #c62828; --known: #8a5a00; --warn: #8a5a00; --skip: #5c5c5c; }
@media (prefers-color-scheme: dark) {
  :root { --text: #ececec; --muted: #a0a0a0; --line: #3a3a3a; --surface: #1e1e1e; --page: #121212;
          --pass: #57c478; --fail: #ff7b72; --known: #e3b341; --warn: #e3b341; --skip: #a0a0a0; } }
body { font: 15px/1.5 system-ui, sans-serif; color: var(--text); background: var(--page); margin: 0 auto; max-width: 1100px; padding: 16px; }
header { border-left: 6px solid var(--skip); padding: 4px 16px; background: var(--surface); }
header.pass { border-color: var(--pass); } header.fail { border-color: var(--fail); }
header.known, header.partial { border-color: var(--known); }
.kicker { color: var(--muted); margin: 0; } h1 { margin: 4px 0; font-size: 24px; } h2 { margin-top: 32px; }
.scroll { overflow-x: auto; } table { border-collapse: collapse; width: 100%; margin: 12px 0; }
th, td { text-align: left; vertical-align: top; padding: 6px 10px; border-bottom: 1px solid var(--line); }
th { color: var(--muted); font-weight: 600; } code { font-size: 13px; white-space: nowrap; }
.chip { font-weight: 600; } .chip.pass { color: var(--pass); } .chip.fail { color: var(--fail); }
.chip.known { color: var(--known); } .chip.warn { color: var(--warn); } .chip.skip { color: var(--skip); }
a { color: inherit; }
</style></head><body>
__BODY__
</body></html>
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("results", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--tested-by")
    parser.add_argument("--date")
    arguments = parser.parse_args()
    report = Report(arguments.results, arguments.tested_by, arguments.date)
    out = arguments.out or arguments.results.parent / "report"
    out.mkdir(parents=True, exist_ok=True)
    (out / "REPORT.md").write_text(report.markdown())
    (out / "report.html").write_text(report.html())
    print(out / "REPORT.md")


if __name__ == "__main__":
    main()
