import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const manifest = await readFile(path.join(root, 'src-tauri', 'Cargo.toml'), 'utf8');
const cargoLock = await readFile(path.join(root, 'src-tauri', 'Cargo.lock'), 'utf8');
const licenseByName = {
  'eframe': 'MIT OR Apache-2.0',
  'image': 'MIT OR Apache-2.0',
  'raw-window-handle': 'MIT OR Apache-2.0',
  'tray-icon': 'MIT OR Apache-2.0',
  'serde': 'MIT OR Apache-2.0',
  'serde_json': 'MIT OR Apache-2.0',
  'tokio': 'MIT',
  'reqwest': 'MIT OR Apache-2.0',
  'base64': 'MIT OR Apache-2.0',
  'sha1': 'MIT OR Apache-2.0',
  'sha2': 'MIT OR Apache-2.0',
  'rand': 'MIT OR Apache-2.0',
  'zeroize': 'MIT OR Apache-2.0',
  'url': 'MIT OR Apache-2.0',
  'winreg': 'MIT OR Apache-2.0',
  'windows-sys': 'MIT OR Apache-2.0',
  'zip': 'MIT',
};

const direct = new Set();
let section = '';
for (const line of manifest.split(/\r?\n/)) {
  const header = line.match(/^\[([^\]]+)\]$/);
  if (header) { section = header[1]; continue; }
  if (section !== 'dependencies' && section !== 'build-dependencies' && !section.endsWith('.dependencies')) continue;
  const dependency = line.match(/^([A-Za-z0-9_-]+)\s*=/);
  if (dependency) direct.add(dependency[1]);
}

const locked = new Map();
for (const block of cargoLock.split(/^\[\[package\]\]\s*$/m).slice(1)) {
  const name = block.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
  const version = block.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const source = block.match(/^source\s*=\s*"([^"]+)"/m)?.[1];
  if (name && version && source?.startsWith('registry+')) locked.set(name, [...(locked.get(name) || []), version]);
}
const rootBlock = cargoLock.split(/^\[\[package\]\]\s*$/m).slice(1).find(block => /^name\s*=\s*"homepanel"/m.test(block));
const rootVersions = new Map();
for (const line of rootBlock?.match(/dependencies\s*=\s*\[([\s\S]*?)\]/)?.[1]?.split(/\r?\n/) || []) {
  const entry = line.match(/^\s*"([^"]+)"/);
  if (!entry) continue;
  const [name, version] = entry[1].split(/\s+/);
  if (version) rootVersions.set(name, version);
}
const rustPackages = [...direct].sort().map(name => {
  const candidates = locked.get(name) || [];
  const version = rootVersions.get(name) || (candidates.length === 1 ? candidates[0] : undefined);
  const license = licenseByName[name];
  if (!version || !license) throw new Error(`Missing pinned version or reviewed SPDX declaration for ${name}.`);
  return { name, version, license };
});

const table = items => items.map(pkg => `| \`${pkg.name}@${pkg.version}\` | ${pkg.license} |`).join('\n');
const report = [
  '# Direct dependency license summary',
  '',
  'Generated from `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`.',
  'This table covers direct Rust dependencies only. The lockfile pins the transitive dependency graph; each transitive package retains its upstream license terms.',
  '',
  '## Native application dependencies',
  '',
  '| Package | Declared license |',
  '|---|---|',
  table(rustPackages),
  '',
].join('\n');
await writeFile(path.join(root, 'LICENSES', 'DEPENDENCY_LICENSES.md'), report, 'utf8');
process.stdout.write(`Wrote direct dependency license summary (${rustPackages.length} Rust packages).\n`);
