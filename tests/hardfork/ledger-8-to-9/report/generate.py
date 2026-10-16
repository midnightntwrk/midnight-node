#!/usr/bin/env python3
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
"""Writes the hard-fork test report from a results directory.

    generate.py <results-dir> [--out <dir>] [--date <text>] [--tested-by <text>]
                [--explorer <url with {height}>] [--notes <file.md>]

Writes REPORT.md and report.html, a single-file page, from
<results-dir>/*.tsv, context.json and ../state/fork.env. --notes adds a hand-written
Markdown section (paragraphs, "- " bullets, **bold**, `code`, [links](url)).

A check counts as its worst row, so a second verdict, or a re-run appended with T_APPEND,
never hides a FAIL. Re-run a table without T_APPEND to start it over.
"""
import argparse
import csv
import html
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent

TABLES = [
    ("PRE", "Network preflight"),
    ("L8", "Ledger-8 baseline"),
    ("CLI-L8", "Client stack on ledger 8"),
    ("HF", "The fork"),
    ("WAVE-*", "Binary rollout wave"),
    ("SNAP", "Pre-fork snapshot"),
    ("SNAPDIFF", "Snapshot read back after the fork"),
    ("L9", "Preserved by the fork"),
    ("FEAT", "Added by ledger 9"),
    ("CLI-L9", "Client stack on ledger 9"),
    ("HF11", "Validator re-sync"),
    ("HF12", "Indexer re-sync"),
    ("SAFE", "SafeMode"),
    ("SEC", "Security and release completeness"),
]
STATUS_ORDER = ["FAIL", "KNOWN", "WARN", "PASS", "SKIP"]
FIELDS = ["table", "id", "status", "refs", "title", "detail"]
PRE_FORK_TABLES = {"PRE", "L8", "CLI-L8", "SNAP"}
WORST_ROW_RULE = "A check counts as its worst row: a second verdict or a T_APPEND re-run never hides a FAIL."
TICKET = re.compile(r"\b(midnight-(?:node|indexer|ledger|wallet|js)|node|indexer|ledger|wallet)#(\d+)")
FAILURES = ("Failures", "Checks that did not hold.")
KNOWN = ("Known issues", "Limitations filed upstream; each names its ticket.")
OBSERVATIONS = ("Observations", "Checks that ran and saw something unexpected that is not a defect of the fork.")
NOT_RUN = ("Not run", "Checks this target could not run, with the reason.")


def table_entry(name):  # (rank, title)
    for i, (key, title) in enumerate(TABLES):
        if key == name:
            return i, title
        if key.endswith("*") and name.startswith(key[:-1]):
            return i, f"{title} {name[len(key) - 1:]}"
    return len(TABLES), name


def table_title(name):
    return table_entry(name)[1]


def load_rows(results):  # (rows, unreadable "file:line" list)
    rows, bad = [], []
    for tsv in sorted(results.glob("*.tsv"), key=lambda p: (table_entry(p.stem)[0], p.stem)):
        with tsv.open(newline="") as f:
            for line, r in enumerate(csv.DictReader(f, delimiter="\t"), 2):
                if any(r.get(k) is None for k in FIELDS) or not r["id"] or r["status"] not in STATUS_ORDER:
                    bad.append(f"{tsv.name}:{line}")
                else:
                    rows.append(r)
    return rows, bad


def merge_checks(rows):  # one entry per (table, id), status the worst of its rows
    checks = {}
    for r in rows:
        c = checks.setdefault((r["table"], r["id"]), {**r, "rows": []})
        c["rows"].append(r)
    for c in checks.values():
        if len(c["rows"]) > 1:
            c["status"] = worst({r["status"] for r in c["rows"]})
            c["detail"] = " · ".join(f"{r['status']}: {r['detail']}" for r in c["rows"])
    return list(checks.values())


def load_env(path):
    if not path.is_file():
        return {}
    pairs = (line.split("=", 1) for line in path.read_text().splitlines() if "=" in line and not line.startswith("#"))
    return {k.strip(): v.strip().strip("'\"") for k, v in pairs}


def load_plan():
    return [tuple(line.split("\t", 1)) for line in (HERE / "test-plan.tsv").read_text().splitlines()
            if "\t" in line and not line.startswith("#")]


def load_notes(path):
    lines = path.read_text().splitlines()
    if lines and lines[0].startswith("# "):
        return lines[0][2:].strip(), lines[1:]
    return "Notes", lines


def count(rows):
    return {s: sum(1 for r in rows if r["status"] == s) for s in STATUS_ORDER}


def tally(rows, sep):
    n = count(rows)
    return sep.join(f"{n[s]} {s.lower()}" for s in STATUS_ORDER if n[s])


def worst(statuses):
    return next((s for s in STATUS_ORDER if s in statuses), None)


def repo_of(match):
    name = match.group(1)
    return name if name.startswith("midnight-") else f"midnight-{name}"


def ticket_url(repo, number):
    return f"https://github.com/midnightntwrk/{repo}/issues/{number}"


def tickets(text):
    return sorted({(repo_of(m), int(m.group(2))) for m in TICKET.finditer(text or "")})


def safe_url(url):  # http(s) or relative only
    return not re.match(r"[a-z][a-z0-9+.-]*:", url, re.I) or re.match(r"https?://", url, re.I)


def notes_inline(text):
    text = html.escape(text)
    text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"`(.+?)`", r"<code>\1</code>", text)
    return re.sub(r"\[([^\]]+)\]\(([^)]+)\)",
                  lambda m: f'<a href="{m.group(2)}">{m.group(1)}</a>' if safe_url(html.unescape(m.group(2))) else m.group(1), text)


def notes_html(lines):
    out, para, items = [], [], []

    def flush():
        if para:
            out.append(f"<p>{notes_inline(' '.join(para))}</p>")
            para.clear()
        if items:
            out.append("<ul>" + "".join(f"<li>{notes_inline(i)}</li>" for i in items) + "</ul>")
            items.clear()

    for line in lines:
        if line.startswith("- "):
            if para:
                flush()
            items.append(line[2:].strip())
        elif not line.strip():
            flush()
        elif items and line.startswith("  "):
            items[-1] += " " + line.strip()
        else:
            if items:
                flush()
            para.append(line.strip())
    flush()
    return "".join(out)


def scrollable(content):
    return content.replace("<table>", "<div class=scroll><table>").replace("</table>", "</table></div>")


class Report:
    def __init__(self, results, args):
        self.all_rows, self.unreadable = load_rows(results)
        if not self.all_rows:
            sys.exit(f"no results in {results}")
        self.rows = merge_checks(self.all_rows)
        context = results / "context.json"
        self.ctx = json.loads(context.read_text()) if context.is_file() else {}
        self.fork_height = load_env(results.parent / "state" / "fork.env").get("FORK_HEIGHT")
        self.args = args
        network = self.ctx.get("network") or "local"
        self.network = "local-env" if network == "local" else network
        self.date = args.date or datetime.now(timezone.utc).strftime("%d %B %Y").lstrip("0")
        explorer = args.explorer or self.ctx.get("explorer_block_url") or ""
        self.explorer = explorer if "{height}" in explorer and safe_url(explorer) else ""
        self.notes = load_notes(args.notes) if args.notes else None
        self.counts = count(self.rows)
        self.tables = list(dict.fromkeys(r["table"] for r in self.rows))

    def rows_with(self, statuses):
        return [r for r in self.rows if r["status"] in statuses]

    def block_url(self, height):
        return self.explorer.replace("{height}", height)

    def forked(self):  # the fork block PASSed locally; on a network, a verified fork height was saved
        upg = [r["status"] for r in self.rows if r["id"] == "HF-UPG-2"]
        return upg == ["PASS"] if upg else bool(self.fork_height)

    def verdict(self):
        c, net = self.counts, self.network
        fails = "1 check fails" if c["FAIL"] == 1 else f"{c['FAIL']} checks fail"
        post_ran = any(r["table"] == "L9" and r["status"] == "PASS" for r in self.rows)
        if not c["FAIL"] and c["SKIP"] == len(self.rows):
            return "partial", f"Nothing ran on {net}: every check was skipped"
        if not self.forked():
            if any(r["table"] not in PRE_FORK_TABLES and not r["table"].startswith("WAVE-") for r in self.rows):
                return "fail", f"The fork on {net} is not confirmed" + (f": {fails}" if c["FAIL"] else "")
            return ("fail" if c["FAIL"] else "partial"), f"{net} before the fork: ledger-8 checks only" + (f", {fails}" if c["FAIL"] else "")
        if c["FAIL"]:
            return "fail", f"The ledger 8 to ledger 9 hard fork on {net}: {fails}"
        if not post_ran:
            return "partial", f"{net} forked; the post-fork checks have not run yet"
        if self.unreadable:
            return "partial", f"{net} forked, but {len(self.unreadable)} result row{'s' if len(self.unreadable) > 1 else ''} could not be read"
        if c["KNOWN"]:
            return "known", f"{net} completed the ledger 8 to ledger 9 hard fork, with known issues"
        return "pass", f"{net} completed the ledger 8 to ledger 9 hard fork"

    def summary_sentence(self):
        c = self.counts
        parts = [f"The runtime upgrade took effect at block {self.fork_height}."] if self.fork_height else []
        parts.append(f"{len(self.rows)} checks ran across {len(self.tables)} tables.")
        if self.unreadable:
            parts.append(f"{len(self.unreadable)} unreadable result rows were left out: {', '.join(self.unreadable)}.")
        parts.append(f"{c['PASS']} pass, {c['FAIL']} fail, {c['KNOWN']} are filed limitations, "
                     f"{c['WARN']} are observations and {c['SKIP']} did not run.")
        return " ".join(parts)

    def environment(self):
        c = self.ctx
        env = [("Target", c.get("target_description") or self.network)]
        for key, label in [("l8", "Before the fork"), ("l9", "After the fork")]:
            if c.get(f"{key}_ref"):
                env.append((label, f"{c[f'{key}_ref']} ({c.get(f'{key}_node_image', '')})"))
        if self.fork_height:
            env.append(("Fork block", self.fork_height))
        for key, label in [("indexer", "Indexer"), ("proof_servers", "Proof servers"),
                           ("clients", "Wallet SDK / Midnight.js"), ("suite_commit", "Test suite commit")]:
            if c.get(key):
                env.append((label, c[key]))
        if self.args.tested_by:
            env.append(("Tested by", self.args.tested_by))
        return env

    def coverage(self):  # (item, title, worst status or NONE, tally)
        out = []
        for item, title in load_plan():
            rows = [r for r in self.rows if re.search(rf"\b{re.escape(item)}\b", r["refs"])]
            if not rows:
                out.append((item, title, "NONE", "not covered by this suite"))
                continue
            ran = {r["status"] for r in rows if r["status"] != "SKIP"}
            out.append((item, title, worst(ran) if ran else "SKIP", tally(rows, ", ")))
        return out

    def md_link(self, text):
        text = TICKET.sub(lambda m: f"[{m.group(0)}]({ticket_url(repo_of(m), int(m.group(2)))})", text)
        if self.explorer:
            text = re.sub(r"(?<![\w/])#(\d{2,})\b", lambda m: f"[#{m.group(1)}]({self.block_url(m.group(1))})", text)
        return text.replace("|", "\\|")

    def markdown(self):
        c = self.counts
        L = [f"# {self.verdict()[1]}", "", f"_Hard-fork test report · {self.date}_", "", self.summary_sentence(), "",
             " · ".join(f"**{c[s]} {s.lower()}**" for s in ["PASS", "FAIL", "KNOWN", "WARN", "SKIP"]), "",
             "| | |", "|---|---|"]
        L += [f"| {k} | {self.md_link(v)} |" for k, v in self.environment()]
        L += ["", "## Test plan coverage", "", "| Item | What must hold | Result | Checks |", "|---|---|---|---|"]
        L += [f"| {item} | {title} | **{'Not covered' if st == 'NONE' else st.title()}** | {desc} |"
              for item, title, st, desc in self.coverage()]
        for (label, intro), status in [(FAILURES, "FAIL"), (KNOWN, "KNOWN"), (OBSERVATIONS, "WARN"), (NOT_RUN, "SKIP")]:
            rows = self.rows_with({status})
            if rows:
                L += ["", f"## {label}", "", intro, "", "| Check | What was checked | Detail |", "|---|---|---|"]
                L += [f"| {self.md_link(r['id'])} | {self.md_link(r['title'])} | {self.md_link(r['detail'])} |" for r in rows]
        if self.notes:
            L += ["", f"## {self.notes[0]}", "", *self.notes[1]]
        L += ["", "## Every check", "", WORST_ROW_RULE]
        for t in self.tables:
            L += ["", f"### {table_title(t)} ({t})", "", "| ID | Status | Refs | Check | Detail |", "|---|---|---|---|---|"]
            L += [f"| {self.md_link(r['id'])} | {r['status']} | {self.md_link(r['refs'])} | {self.md_link(r['title'])} | {self.md_link(r['detail'])} |"
                  for r in self.rows if r["table"] == t]
        return "\n".join(L) + "\n"

    def h(self, text):
        out = html.escape(text or "")
        out = TICKET.sub(lambda m: f'<a href="{ticket_url(repo_of(m), int(m.group(2)))}">{m.group(0)}</a>', out)
        if self.explorer:
            out = re.sub(r"(?<![\w/&;])#(\d{2,})\b",
                         lambda m: f'<a href="{html.escape(self.block_url(m.group(1)))}">#{m.group(1)}</a>', out)
        return out

    @staticmethod
    def chip(st):
        return f'<span class="chip {st.lower()}">{"Not covered" if st == "NONE" else st.title()}</span>'

    @staticmethod
    def table(headers, body):
        return f"<table><thead><tr>{''.join(f'<th>{h}</th>' for h in headers)}</tr></thead><tbody>{body}</tbody></table>"

    def issue_table(self, statuses):
        body = ""
        for r in self.rows_with(statuses):
            links = ", ".join(f'<a href="{ticket_url(repo, n)}">{repo.replace("midnight-", "")} #{n}</a>'
                              for repo, n in tickets(r["detail"] + " " + r["refs"]))
            body += (f"<tr><td class=mono>{html.escape(r['id'])}</td><td>{self.h(r['title'])}</td><td>{self.chip(r['status'])}</td>"
                     f"<td>{self.h(r['detail'])}</td><td>{links or '<span class=muted>none</span>'}</td></tr>")
        return body and self.table(["Check", "What was checked", "Result", "What happened", "Ticket"], body)

    def html(self):
        kind, headline = self.verdict()
        c = self.counts
        env = "".join(f"<tr><th>{html.escape(k)}</th><td>{self.h(v)}</td></tr>" for k, v in self.environment())
        cov = "".join(f"<tr><td class=mono>{html.escape(item)}</td><td>{html.escape(title)}</td><td>{self.chip(st)}</td><td class=muted>{html.escape(desc)}</td></tr>"
                      for item, title, st, desc in self.coverage())
        sections = [("Test plan coverage", "Each item of the hard-fork test plan and the worst result among the checks that reference it.",
                     self.table(["Item", "What must hold", "Result", "Checks"], cov))]
        problems = self.issue_table({"FAIL", "KNOWN"})
        if problems:
            sections.append(("Failures and known issues", "None of these is hidden by the totals above. A known issue is filed upstream and does not fail the run.", problems))
        observations = self.issue_table({"WARN"})
        if observations:
            sections.append((*OBSERVATIONS, observations))
        skipped = "".join(f"<tr><td class=mono>{html.escape(r['id'])}</td><td>{self.h(r['title'])}</td><td>{self.h(r['detail'])}</td></tr>"
                          for r in self.rows_with({"SKIP"}))
        if skipped:
            sections.append(("Not run on this target", "Each with the reason it did not run.",
                             self.table(["Check", "What would be checked", "Reason"], skipped)))
        if self.notes:
            sections.append((self.notes[0], "", notes_html(self.notes[1])))
        every = ""
        for t in self.tables:
            rows = [r for r in self.rows if r["table"] == t]
            body = "".join(f"<tr><td class=mono>{html.escape(r['id'])}</td><td>{self.chip(r['status'])}</td><td>{self.h(r['title'])}</td>"
                           f"<td class=detail>{self.h(r['detail'])}</td></tr>" for r in rows)
            every += (f"<details><summary><span>{html.escape(table_title(t))}</span> <span class=muted>{html.escape(t)} · {tally(rows, ' · ')}</span></summary>"
                      f"{self.table(['ID', 'Result', 'Check', 'Detail'], body)}</details>")
        sections.append(("Every check", "One row per check, grouped by results table.", every))
        body = "".join(f'<section><h2><span class=num>{i}</span>{html.escape(title)}</h2>{f"<p class=lead>{html.escape(lead)}</p>" if lead else ""}{scrollable(content)}</section>'
                       for i, (title, lead, content) in enumerate(sections, 1))
        kpis = "".join(f'<div class="kpi {s.lower() if c[s] else "zero"}"><b>{c[s]}</b><span>{s.title()}</span></div>' for s in STATUS_ORDER)
        return TEMPLATE.format(
            title=html.escape(f"Ledger 9 hard fork · {self.network}"), headline=html.escape(headline), verdict=kind,
            date=html.escape(self.date), summary=self.h(self.summary_sentence()), kpis=kpis, env=env, body=body,
            footer=html.escape(f"Ledger 8 → 9 hard fork · {self.network} · {WORST_ROW_RULE} · generated {datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M UTC')}"),
        )


TEMPLATE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=DM+Mono:wght@400;500&family=Outfit:wght@300;400;500&display=swap" rel="stylesheet">
<style>
:root {{
  --bg: #101010; --panel: #181818; --line: #2a2a2a; --text: #ffffff; --muted: #9e9e9e; --faint: #4b4b4b;
  --primary: #0000fe; --primary-dark: #0000d0; --link: #8f8fff;
  --pass: #3ddc97; --fail: #ff5c5c; --known: #f5b642; --warn: #c9c9c9; --skip: #6f6f6f;
}}
* {{ box-sizing: border-box; }}
body {{ margin: 0; background: var(--bg); color: var(--text); font: 300 16px/1.55 Outfit, system-ui, sans-serif; }}
main {{ max-width: 1120px; margin: 0 auto; padding: 48px 16px 64px; }}
a {{ color: var(--link); text-decoration: none; }} a:hover {{ text-decoration: underline; }}
header {{ display: flex; gap: 16px; align-items: flex-start; }}
header > div {{ min-width: 0; }}
.logo {{ flex: none; width: 44px; height: 44px; }}
.eyebrow {{ font: 400 13px/1.4 "DM Mono", ui-monospace, monospace; color: var(--muted); letter-spacing: .04em; text-transform: uppercase; }}
h1 {{ font-weight: 400; font-size: clamp(26px, 4vw, 42px); line-height: 1.15; margin: 6px 0 12px; overflow-wrap: anywhere; }}
h1::after {{ content: ""; display: block; width: 64px; height: 4px; margin-top: 16px; background: var(--primary); }}
.verdict-fail h1::after {{ background: var(--fail); }} .verdict-known h1::after {{ background: var(--known); }}
.verdict-partial h1::after {{ background: var(--warn); }}
p {{ margin: 0 0 12px; }} ul {{ margin: 0 0 12px; padding-left: 20px; }} li {{ margin: 4px 0; }}
code {{ font: 400 13px "DM Mono", monospace; }} .lead {{ color: var(--muted); }}
.kpis {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(96px, 1fr)); gap: 8px; margin: 28px 0; }}
.kpi {{ background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 14px 16px; }}
.kpi b {{ display: block; font: 500 28px/1 "DM Mono", monospace; }} .kpi span {{ color: var(--muted); font-size: 14px; }}
.kpi.zero b {{ color: var(--faint); }} .kpi.pass b {{ color: var(--pass); }} .kpi.fail b {{ color: var(--fail); }} .kpi.known b {{ color: var(--known); }}
section {{ margin-top: 48px; }}
h2 {{ font-weight: 400; font-size: 24px; margin: 0 0 6px; display: flex; gap: 12px; align-items: baseline; }}
.num {{ font: 500 14px "DM Mono", monospace; color: var(--bg); background: var(--text); border-radius: 50%; width: 26px; height: 26px; display: inline-grid; place-items: center; flex: none; }}
.scroll {{ overflow-x: auto; margin-top: 12px; }}
table {{ width: 100%; border-collapse: collapse; font-size: 14.5px; }}
th, td {{ text-align: left; vertical-align: top; padding: 10px 12px; border-bottom: 1px solid var(--line); }}
thead th {{ font: 400 12px "DM Mono", monospace; text-transform: uppercase; letter-spacing: .05em; color: var(--muted); }}
.env {{ margin-top: 8px; }} .env th {{ width: 220px; color: var(--muted); font-weight: 300; }} .env td {{ overflow-wrap: anywhere; }}
.mono {{ font-family: "DM Mono", monospace; font-size: 13px; white-space: nowrap; }}
.muted {{ color: var(--muted); }} .detail {{ color: #d6d6d6; word-break: break-word; }}
.chip {{ display: inline-block; font: 500 12px/1 "DM Mono", monospace; padding: 5px 9px; border-radius: 999px; border: 1px solid currentColor; white-space: nowrap; }}
.chip.pass {{ color: var(--pass); }} .chip.fail {{ color: var(--fail); }} .chip.known {{ color: var(--known); }}
.chip.warn {{ color: var(--warn); }} .chip.skip, .chip.none {{ color: var(--skip); }}
details {{ background: var(--panel); border: 1px solid var(--line); border-radius: 10px; margin-top: 10px; }}
summary {{ cursor: pointer; padding: 14px 16px; display: flex; justify-content: space-between; gap: 12px; flex-wrap: wrap; }}
details table {{ margin: 0; }} details td, details th {{ padding: 8px 16px; }}
footer {{ margin-top: 56px; padding-top: 16px; border-top: 1px solid var(--line); color: var(--faint); font: 400 12px "DM Mono", monospace; }}
@media (max-width: 720px) {{ .env th {{ width: 38%; }} th, td {{ padding: 8px; }} }}
@media print {{
  :root {{ --bg: #fff; --panel: #fff; --line: #ddd; --text: #101010; --muted: #555; --link: #0000d0; }}
  details {{ break-inside: avoid; }} details:not([open]) > *:not(summary) {{ display: block; }}
}}
</style>
</head>
<body class="verdict-{verdict}">
<main>
<header>
<svg class="logo" viewBox="0 0 44 44" aria-hidden="true"><circle cx="22" cy="22" r="20" fill="none" stroke="currentColor" stroke-width="2.5"/><rect x="19" y="11" width="6" height="6" fill="currentColor"/><rect x="19" y="19" width="6" height="6" fill="currentColor"/><rect x="19" y="27" width="6" height="6" fill="currentColor"/></svg>
<div>
<div class="eyebrow">Hard-fork test report · {date}</div>
<h1>{headline}</h1>
<p>{summary}</p>
</div>
</header>
<div class="kpis">{kpis}</div>
<table class="env"><tbody>{env}</tbody></table>
{body}
<footer>{footer}</footer>
</main>
</body>
</html>
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("results", type=Path)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--date")
    ap.add_argument("--tested-by")
    ap.add_argument("--explorer", help="block URL template containing {height}")
    ap.add_argument("--notes", type=Path, help="Markdown file; its first '# ' line is the section title")
    args = ap.parse_args()
    out = args.out or args.results
    out.mkdir(parents=True, exist_ok=True)
    report = Report(args.results, args)
    (out / "REPORT.md").write_text(report.markdown())
    (out / "report.html").write_text(report.html())
    print(f"{out / 'REPORT.md'}\n{out / 'report.html'}")


if __name__ == "__main__":
    main()
