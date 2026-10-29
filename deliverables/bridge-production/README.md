# Bridge architecture presentation sources

The editable documents are part of the bridge source branch:

- [Stakeholder brief](../../docs/proposals/snowbridge-sp1-stakeholder-brief.md).
- [Production architecture](../../docs/proposals/snowbridge-sp1-production-architecture.md).
- [Development and rollout work breakdown](../../docs/proposals/snowbridge-sp1-production-work-breakdown.md).

They distinguish the demonstrated local PoC from proposed production work.
See the [evidence guide](../../tests/snowbridge/evidence/README.md) for the public
validation record and the raw artifacts omitted from Git.

## Generate local presentation copies

The renderer needs Node, a package directory containing `marked` and `playwright`,
a Mermaid 11 browser bundle, and an installed Chromium-compatible executable or
Playwright browser. Supply those paths explicitly:

```bash
node deliverables/bridge-production/build-documents.mjs \
  /path/to/node_modules \
  /path/to/mermaid.min.js \
  /path/to/chromium
```

Append `--brief` after the browser argument to render only the stakeholder brief.
The default command renders the architecture and work breakdown.

Generated HTML, Mermaid/SVG diagrams, JSON manifests and ZIP packages in this
directory are ignored, as are the bridge PDFs under `output/pdf/` and QA captures
under `tmp/pdfs/bridge-production/`. Existing local presentation files are
preserved. The renderer and this guide remain in Git; the Markdown sources are
the source of truth.

The local renderer resolves repository links against its checkout, so generated
PDFs can contain absolute file links. Excluding these generated files keeps those
paths out of source commits. Before sharing a separately generated presentation
bundle, regenerate it from the current sources, replace local-only links with
public links, and visually review the PDF pages. The old local ZIP and manifests
are historical exports, not automatically updated by source edits.
