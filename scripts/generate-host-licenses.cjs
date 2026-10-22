// Inventory only JS packages present in the production Metro bundle source map.
const fs = require('node:fs');
const path = require('node:path');
const modules = path.resolve(process.argv[2]);
const sources = JSON.parse(fs.readFileSync(process.argv[4], 'utf8')).sources;
const packages = new Set();
for (const source of sources) {
  const parts = source.split(/[\\/]/);
  const i = parts.lastIndexOf('node_modules');
  if (i >= 0) packages.add(parts.slice(0, i + 1 + (parts[i + 1].startsWith('@') ? 2 : 1)).join('/'));
}
let output = 'HomePanel optional Windows plugin host - JavaScript dependency notices\n';
for (const directory of [...packages].sort()) {
  const item = JSON.parse(fs.readFileSync(path.join(directory, 'package.json'), 'utf8'));
  const {name} = item;
  const licenses = fs.readdirSync(directory).filter(name => /^(LICENSE|LICENCE|NOTICE|COPYING)([._-]|$)/i.test(name));
  output += `\n===== ${name} ${item.version} (${item.license || 'See license text'}) =====\n`;
  if (!licenses.length) {
    if (name.startsWith('@react-native/')) output += fs.readFileSync(path.join(modules, 'react-native/LICENSE'), 'utf8') + '\n';
    else if (name === 'metro-runtime') output += fs.readFileSync(path.join(__dirname, '../LICENSES/Metro-0.82.5.txt'), 'utf8') + '\n';
    else throw new Error(`Missing license text: ${name}`);
  }
  for (const file of licenses) {
    if (fs.statSync(path.join(directory, file)).isFile()) output += fs.readFileSync(path.join(directory, file), 'utf8') + '\n';
  }
}
fs.writeFileSync(process.argv[3], output, 'utf8');
