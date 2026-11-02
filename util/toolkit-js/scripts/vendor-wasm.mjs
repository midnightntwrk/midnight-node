// Overlay the vendored ledger wasm packages onto every installed copy.
//
// npm cannot override a transitive dependency with a `file:` tarball whose version
// falls outside the dependant's range (compact-js wants `^1.0.0-rc.5`, the vendored
// build is `1.0.1-rc.2`), so the registry versions still land in the workspaces'
// nested node_modules. This postinstall step replaces each copy with the vendored
// tarball's contents. See vendor/README.md for why the tarballs exist and when to
// delete all of this.
import { execFileSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const vendor = join(root, 'vendor');
if (!existsSync(vendor)) {
  console.error('vendor-wasm: vendor/ is missing; remove this postinstall script together with it');
  process.exit(1);
}

const tarballs = readdirSync(vendor).filter((f) => f.endsWith('.tgz')).sort();
if (tarballs.length === 0) {
  console.error('vendor-wasm: vendor/ holds no .tgz; remove this postinstall script together with it');
  process.exit(1);
}
const packageName = (tgz) =>
  JSON.parse(execFileSync('tar', ['-xOzf', tgz, 'package/package.json'], { encoding: 'utf8' })).name;

// Every `node_modules/<name>` directory under the workspace, nested ones included.
function findInstalls(dir, name, out) {
  const nm = join(dir, 'node_modules');
  if (!existsSync(nm)) return out;
  const target = join(nm, name);
  if (existsSync(target) && !lstatSync(target).isSymbolicLink()) out.push(target);
  for (const entry of readdirSync(nm)) {
    const p = join(nm, entry);
    // lstat: workspace links (node_modules/@midnight-ntwrk/node-toolkit -> ../..) would recurse forever.
    if (!lstatSync(p).isDirectory()) continue;
    if (entry.startsWith('@')) {
      for (const scoped of readdirSync(p)) {
        const sp = join(p, scoped);
        if (lstatSync(sp).isDirectory()) findInstalls(sp, name, out);
      }
    } else {
      findInstalls(p, name, out);
    }
  }
  return out;
}

for (const file of tarballs) {
  const tgz = join(vendor, file);
  const name = packageName(tgz);
  const installs = findInstalls(root, name, []);
  for (const ws of readdirSync(root).filter((d) => d.startsWith('compact-'))) findInstalls(join(root, ws), name, installs);
  if (installs.length === 0) {
    console.error(`vendor-wasm: no installed copy of ${name} found to replace`);
    process.exit(1);
  }
  for (const dir of new Set(installs)) {
    const before = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')).version;
    // Extract beside the target and swap, so a failed extraction leaves the install intact.
    const tmp = `${dir}.vendor-wasm-tmp`;
    rmSync(tmp, { recursive: true, force: true });
    mkdirSync(tmp, { recursive: true });
    execFileSync('tar', ['-xzf', tgz, '--strip-components=1', '-C', tmp]);
    rmSync(dir, { recursive: true, force: true });
    renameSync(tmp, dir);
    const after = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')).version;
    console.log(`vendor-wasm: ${name} ${before} -> ${after} at ${dir.replace(root + '/', '')}`);
  }
}
