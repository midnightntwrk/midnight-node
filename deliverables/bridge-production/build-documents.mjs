// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import fs from 'node:fs/promises';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';

// Usage: node build-documents.mjs NODE_MODULES_DIRECTORY MERMAID_BROWSER_BUNDLE [CHROMIUM_EXECUTABLE]
const [modules, mermaidBundle, executablePath, documentSelection] = process.argv.slice(2);
if (!modules || !mermaidBundle) throw new Error('Supply a package directory containing marked/playwright and a Mermaid browser bundle.');
const require = createRequire(path.join(path.resolve(modules), '__bridge_builder__.cjs'));
const {chromium} = require('playwright');
const {marked} = await import(require.resolve('marked'));
const output = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(output, '../..');
const sourceDirectory = path.join(root, 'docs/proposals');
const diagramDirectory = path.join(output, 'diagrams');
await fs.mkdir(diagramDirectory, {recursive: true});
const pdfDirectory = path.join(root, 'output/pdf');
const qaDirectory = path.join(root, 'tmp/pdfs/bridge-production');
await fs.mkdir(pdfDirectory, {recursive: true});
await fs.mkdir(qaDirectory, {recursive: true});
const browser = await chromium.launch({headless: true, ...(executablePath ? {executablePath} : {})});
const page = await browser.newPage({viewport: {width: 1440, height: 1100}, deviceScaleFactor: 1});
const qaPage = await browser.newPage({viewport: {width: 1440, height: 1100}, deviceScaleFactor: 1});
await page.setContent('<!doctype html><html><head><meta charset="utf-8"></head><body></body></html>');
await page.addScriptTag({path: path.resolve(mermaidBundle)});
await page.evaluate(() => mermaid.initialize({
  startOnLoad: false,
  securityLevel: 'strict',
  theme: 'base',
  themeVariables: {
    fontFamily: 'Arial, sans-serif', fontSize: '17px', primaryColor: '#edf2ff',
    primaryTextColor: '#17243f', primaryBorderColor: '#5674b9', lineColor: '#64748b',
    secondaryColor: '#e8f6ef', tertiaryColor: '#faf3e5', clusterBkg: '#f6f8fc',
    clusterBorder: '#bcc8df', actorBkg: '#edf2ff', actorBorder: '#5674b9',
    actorTextColor: '#17243f', signalColor: '#52627b', noteBkgColor: '#fff4d9',
    noteTextColor: '#17243f', edgeLabelBackground: '#ffffff',
  },
  flowchart: {htmlLabels: false, useMaxWidth: true, nodeSpacing: 30, rankSpacing: 45},
  sequence: {useMaxWidth: true, wrap: true, actorMargin: 28, width: 130, messageMargin: 38},
}));

const css = `
:root{color-scheme:light;--ink:#17243f;--muted:#5c6b82;--accent:#365eb1;--line:#dce3ee;--paper:#fff}
*{box-sizing:border-box}html{scroll-behavior:smooth;scroll-padding-top:24px}body{margin:0;background:#f2f5fa;color:var(--ink);font:16px/1.65 -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif}
a{color:var(--accent);text-decoration:none}a:hover{text-decoration:underline}.shell{display:grid;grid-template-columns:260px minmax(0,1fr);max-width:1640px;margin:auto;min-height:100vh}
aside{padding:34px 24px;position:sticky;top:0;height:100vh;overflow:auto;border-right:1px solid var(--line)}aside .brand{font-size:12px;letter-spacing:.12em;font-weight:750;text-transform:uppercase;color:var(--accent)}aside .subtitle{font-size:13px;color:var(--muted);margin:8px 0 22px}aside nav a{display:block;font-size:13px;line-height:1.45;padding:7px 0;color:#425472}aside nav a.sub{padding-left:10px;font-size:12px}aside .related{border-top:1px solid var(--line);padding-top:16px;margin-top:20px;font-size:13px}
main{background:var(--paper);padding:54px 60px 90px;min-width:0}h1{font-size:40px;line-height:1.14;letter-spacing:-.025em;max-width:950px;margin:0 0 16px}h2{font-size:27px;line-height:1.3;margin:52px 0 18px;border-top:2px solid var(--line);padding-top:28px}h3{font-size:21px;line-height:1.35;margin:30px 0 12px}p{max-width:1050px;margin:14px 0}h1+p{color:var(--muted);font-size:14px}strong{font-weight:680}li{padding-left:3px;margin:7px 0}ul,ol{padding-left:26px;max-width:1050px}table{border-collapse:collapse;width:100%;font-size:13.5px;line-height:1.5;margin:20px 0 26px;table-layout:auto}th{text-align:left;background:#eef3fb;color:#253f70;font-size:12px;letter-spacing:.01em}td,th{padding:11px 12px;vertical-align:top;border:1px solid var(--line)}tr:nth-child(even) td{background:#fafbfd}td:first-child{font-weight:600}code{font:13px/1.55 ui-monospace,SFMono-Regular,Menlo,monospace;background:#eef2f7;padding:2px 4px;border-radius:3px}pre{padding:20px;background:#f4f6fa;border:1px solid var(--line);border-radius:8px;overflow:auto;line-height:1.5}pre code{padding:0;background:none}.diagram{border:1px solid var(--line);border-radius:12px;padding:24px 12px;background:#fff;margin:24px 0 10px;overflow:auto;text-align:center}.diagram svg{display:block;margin:auto;width:100%;height:auto;max-height:850px}.diagram+ p:has(em){font-size:13px;color:var(--muted);margin-top:8px}header.tools{display:flex;justify-content:space-between;gap:12px;align-items:center;color:var(--muted);font-size:12px;margin-bottom:28px}button{background:#edf2fc;border:1px solid #cbd7ed;color:#234779;border-radius:7px;padding:8px 12px;cursor:pointer}footer{margin-top:50px;padding-top:18px;border-top:1px solid var(--line);font-size:12px;color:var(--muted)}
@media(max-width:1050px){.shell{grid-template-columns:210px minmax(0,1fr)}aside{padding:24px 18px}main{padding:36px 28px}h1{font-size:34px}table{font-size:12px}}
@media(max-width:760px){.shell{display:block}aside{position:static;height:auto;border-bottom:1px solid var(--line)}aside nav{display:none}aside .related{margin:0}main{padding:28px 18px}table{display:block;overflow:auto}h1{font-size:30px}.diagram{padding:12px 3px}.diagram svg{min-width:620px}}
@media print{@page{size:A4;margin:17mm 14mm}body{background:#fff;font-size:10pt;line-height:1.5}.shell{display:block}aside,header.tools{display:none}main{padding:0}h1{font-size:28pt}h2{font-size:18pt;margin-top:24pt;padding-top:12pt;break-after:avoid}h3{font-size:13pt;break-after:avoid}h3+p{break-after:avoid}table{font-size:8pt;line-height:1.4}table.compact{break-inside:avoid}thead{display:table-header-group}tr{break-inside:avoid}td,th{padding:6pt}pre{font-size:8pt;white-space:pre-wrap}.diagram{break-inside:avoid;page-break-inside:avoid;padding:12pt 2pt}.diagram svg{max-height:195mm;width:100%;min-width:0}.diagram+p{break-before:avoid}p,li{orphans:3;widows:3}a{color:inherit}footer{font-size:8pt}}
`;
const escape = value => value.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
const slug = value => value.toLowerCase().replace(/[^a-z0-9\s-]/g,'').replace(/\s+/g,'-');
const docs = documentSelection === '--brief' ? [
  {file: 'snowbridge-sp1-stakeholder-brief.md', name: 'stakeholder-brief', label: 'Stakeholder call brief'},
] : [
  {file: 'snowbridge-sp1-production-architecture.md', name: 'architecture', label: 'Architecture review'},
  {file: 'snowbridge-sp1-production-work-breakdown.md', name: 'work-breakdown', label: 'Development and rollout'},
];
const audit = [];
try {
  for (const doc of docs) {
    const source = await fs.readFile(path.join(sourceDirectory, doc.file), 'utf8');
    const matches = [...source.matchAll(/```mermaid\n([\s\S]*?)```/g)];
    let substituted = source.replace(/[\u2010-\u2015\u2212]/g,'-');
    const tokens = [];
    for (let i = 0; i < matches.length; ++i) {
      const mermaidSource = matches[i][1].trim();
      const id = `${doc.name}-diagram-${i+1}`;
      const svg = await page.evaluate(async ({id, code}) => (await mermaid.render(id, code)).svg, {id, code: mermaidSource});
      await fs.writeFile(path.join(diagramDirectory, `${id}.mmd`), mermaidSource+'\n');
      await fs.writeFile(path.join(diagramDirectory, `${id}.svg`), svg);
      const token = `BRIDGEDIAGRAMTOKEN${i}END`;
      substituted = substituted.replace(matches[i][0], token);
      tokens.push({token,svg,id});
    }
    let body = marked.parse(substituted);
    for (const {token,svg,id} of tokens) body = body.replace(`<p>${token}</p>`, `<figure class="diagram" aria-label="${id}">${svg}</figure>`);
    const navigation = [];
    body = body.replace(/<h([123])>([\s\S]*?)<\/h\1>/g, (_,level,title) => {
      const plain = title.replace(/<[^>]+>/g,'');
      const id = slug(plain);
      if (level !== '1') navigation.push(`<a class="${level==='3'?'sub':''}" href="#${id}">${escape(plain)}</a>`);
      return `<h${level} id="${id}">${title}</h${level}>`;
    });
    body = body.replace(/<table>([\s\S]*?)<\/table>/g, (_,content) => {
      const rowCount = (content.match(/<tr>/g)||[]).length;
      const packageTable = /<td>W\d+\.\d+<\/td>/.test(content);
      const compact = !packageTable && rowCount <= 9 && content.replace(/<[^>]*>/g,'').length < 1800;
      return `<table${compact?' class="compact"':''}>${content}</table>`;
    });
    body = body.replace(/href="([^"#]+\.md)(#[^"]*)?"/g, (_,file,fragment='') => {
      const companion = docs.find(d=>d.file===file);
      if(companion) return `href="${companion.name}.html${fragment}"`;
      return `href="${escape(path.relative(output,path.resolve(sourceDirectory,file)))}${fragment}"`;
    });
    body = body.replace(/href="(\.\.\/\.\.\/artifacts\/[^"#]+|\.\.\/\.\.\/(?:tests|ledger|pallets|runtime|node|primitives)\/[^"#]+|\.\.\/\.\.\/SNOWBRIDGE_HANDOFF\.md)"/g, (_,file)=>`href="${escape(path.relative(output,path.resolve(sourceDirectory,file)))}"`);
    const title = source.match(/^# (.+)$/m)[1];
    const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>${escape(title)}</title><style>${css}${doc.name==='stakeholder-brief' ? '@media print{body{font-size:9.5pt;line-height:1.35}h1{font-size:23pt}h2{font-size:12pt;margin:13pt 0 6pt;padding-top:7pt}p{margin:6pt 0}table{margin:7pt 0;font-size:8.5pt}td,th{padding:5pt}.diagram{margin:8pt 0;padding:6pt}.diagram svg{max-height:28mm}li{margin:4pt 0}footer{display:none}}' : ''}</style></head><body><div class="shell"><aside><div class="brand">Midnight ↔ Ethereum</div><div class="subtitle">${escape(doc.label)}<br>Review draft · 28 October 2026</div><nav>${navigation.join('')}</nav><div class="related"><a href="architecture.html">Architecture document</a><br><a href="work-breakdown.html">Work breakdown</a><br><a href="${escape(path.relative(output,path.join(sourceDirectory,doc.file)))}">Editable Markdown source</a></div></aside><main><header class="tools"><span>Snowbridge ingress · SP1 BEEFY finality</span><button type="button" onclick="window.print()">Print or save PDF</button></header>${body}<footer>Architecture review draft. Implemented PoC behavior and proposed production changes are distinguished in the text. Diagrams are embedded SVG; this document needs no network connection.</footer></main></div></body></html>`;
    await fs.writeFile(path.join(output, `${doc.name}.html`), html);
    await qaPage.goto('file://'+path.join(output,`${doc.name}.html`));
    await qaPage.evaluate(() => document.fonts.ready);
    const layout = await qaPage.evaluate(() => ({
      svgCount: document.querySelectorAll('figure.diagram svg').length,
      unresolvedTokens: document.body.innerText.includes('BRIDGEDIAGRAMTOKEN'),
      pageOverflow: document.documentElement.scrollWidth > innerWidth,
      brokenAnchors: [...document.querySelectorAll('a[href^="#"]')].map(a=>a.getAttribute('href').slice(1)).filter(id=>!document.getElementById(id)),
    }));
    if(layout.svgCount !== tokens.length || layout.unresolvedTokens || layout.pageOverflow || layout.brokenAnchors.length) throw new Error(JSON.stringify({doc:doc.name,layout}));
    await qaPage.screenshot({path:path.join(qaDirectory,`${doc.name}-desktop.png`)});
    for(let i=0;i<tokens.length;i++) await qaPage.locator('figure.diagram').nth(i).screenshot({path:path.join(qaDirectory,`${doc.name}-diagram-${i+1}.png`)});
    await qaPage.pdf({
      path:path.join(pdfDirectory,`midnight-ethereum-bridge-${doc.name}.pdf`),
      format:'A4', printBackground:true, preferCSSPageSize:true,
      displayHeaderFooter:true,
      headerTemplate:'<div style="font:8px Arial;color:#64748b;margin:0 14mm;width:100%">MIDNIGHT / ETHEREUM BRIDGE · ARCHITECTURE REVIEW DRAFT</div>',
      footerTemplate:'<div style="font:8px Arial;color:#64748b;margin:0 14mm;width:100%;text-align:right"><span class="pageNumber"></span> / <span class="totalPages"></span></div>',
    });
    audit.push({file:doc.file,diagrams:tokens.length,words:source.split(/\s+/).length,layout});
    console.log(`Rendered ${doc.name}: ${tokens.length} diagrams`);
  }
  const bundleSha256 = createHash('sha256').update(await fs.readFile(mermaidBundle)).digest('hex');
  await fs.writeFile(path.join(output,documentSelection === '--brief' ? 'stakeholder-brief-render-manifest.json' : 'render-manifest.json'),JSON.stringify({browser:browser.version(),mermaidBundleSha256:bundleSha256,documents:audit},null,2)+'\n');
} finally {
  await browser.close();
}
