// Local build compatibility patch for react-native-svg 15.12.1 (MIT).
// Exact upstream sources are backed up outside the project and restored after MSBuild.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function replaceOnce(text, before, after) {
  const nl = text.includes('\r\n') ? '\r\n' : '\n';
  before = before.replaceAll('\n', nl);
  after = after.replaceAll('\n', nl);
  const at = text.indexOf(before);
  if (at < 0 || text.indexOf(before, at + before.length) >= 0) throw new Error('SVG_PATCH_CONTEXT_MISMATCH');
  return text.slice(0, at) + after + text.slice(at + before.length);
}
const specs = [
  {relative: 'windows/RNSVG/RNSVG.vcxproj', backup: 'RNSVG.vcxproj.original',
    hash: '7700e9a316a612816bad4e3279ca8e5588d9583175b1cc0eea442c918a076faf',
    patch(text) {
      text = replaceOnce(text,
        `  <PropertyGroup Condition="'$(Configuration)'=='Debug'" Label="Configuration">`,
        `  <PropertyGroup Condition="'$(UseFabric)'=='true'">\n    <ConfigurationType>StaticLibrary</ConfigurationType>\n  </PropertyGroup>\n  <PropertyGroup Condition="'$(Configuration)'=='Debug'" Label="Configuration">`);
      text = replaceOnce(text,
        `  <Import Project="$(VCTargetsPath)\\Microsoft.Cpp.props" />`,
        `  <Import Project="$(VCTargetsPath)\\Microsoft.Cpp.props" />\n  <PropertyGroup Condition="'$(IntermediateDirectoryRoot)'!=''">\n    <IntDir>$(IntermediateDirectoryRoot)$(ProjectName)\\$(Configuration)\\$(Platform)\\</IntDir>\n  </PropertyGroup>`);
      return replaceOnce(text,
        `  <ImportGroup Label="ReactNativeWindowsPropertySheets">`,
        `  <ImportGroup Label="ReactNativeWindowsPropertySheets">\n    <Import Project="$(ReactNativeWindowsDir)\\PropertySheets\\HybridCRT.props" Condition="'$(UseFabric)'=='true'" />`);
    }},
  {relative: 'react-native.config.js', backup: 'react-native.config.js.original',
    hash: 'a851f3794be3e73785a5946b5e769e76a5a9571b9933cc449a951bce2788f5de',
    patch: text => replaceOnce(text,
      `        cmakeListsPath: '../android/src/main/jni/CMakeLists.txt',\n      },`,
      `        cmakeListsPath: '../android/src/main/jni/CMakeLists.txt',\n      },\n      windows: {\n        sourceDir: 'windows',\n        solutionFile: 'RNSVG.sln',\n        projects: [{projectFile: 'RNSVG/RNSVG.vcxproj', directDependency: true}],\n      },`)}
];

function restore(backupDirectory) {
  const backup = path.resolve(backupDirectory);
  const statePath = path.join(backup, 'state.json');
  const state = JSON.parse(fs.readFileSync(statePath, 'utf8'));
  if (state.schema !== 1 || state.files.length !== specs.length) throw new Error('INVALID_SVG_PATCH_STATE');
  const originals = specs.map((spec, index) => {
    const file = state.files[index];
    if (file.relative !== spec.relative || file.originalHash !== spec.hash) throw new Error('INVALID_SVG_PATCH_STATE');
    const target = path.join(state.projectRoot, 'node_modules/react-native-svg', spec.relative);
    const original = fs.readFileSync(path.join(backup, spec.backup));
    const currentHash = digest(fs.readFileSync(target));
    if (digest(original) !== spec.hash || ![spec.hash, file.patchedHash].includes(currentHash)) {
      throw new Error(`SVG_SOURCE_CHANGED_DURING_BUILD; original retained at ${backup}`);
    }
    return {target, original};
  });
  for (const {target, original} of originals) fs.writeFileSync(target, original);
  state.restored = true;
  fs.writeFileSync(statePath, JSON.stringify(state, null, 2) + '\n');
  return statePath;
}

function prepare(projectDirectory, backupDirectory) {
  const root = fs.realpathSync(projectDirectory);
  const backup = path.resolve(backupDirectory);
  const relativeBackup = path.relative(root, backup);
  if (!relativeBackup || (!relativeBackup.startsWith('..' + path.sep) && !path.isAbsolute(relativeBackup))) {
    throw new Error('SVG_PATCH_BACKUP_MUST_BE_OUTSIDE_PROJECT');
  }
  const packageRoot = path.join(root, 'node_modules/react-native-svg');
  const samePath = (a, b) => process.platform === 'win32' ? a.toLowerCase() === b.toLowerCase() : a === b;
  if (!samePath(fs.realpathSync(packageRoot), packageRoot)) throw new Error('SVG_PATCH_REQUIRES_PRIVATE_NPM_COPY');
  if (JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8')).version !== '15.12.1') {
    throw new Error('SVG_PATCH_REQUIRES_VERSION_15_12_1');
  }
  const files = specs.map(spec => {
    const target = path.join(packageRoot, spec.relative);
    const original = fs.readFileSync(target);
    if (digest(original) !== spec.hash) throw new Error(`SVG_UPSTREAM_HASH_MISMATCH:${spec.relative}`);
    const patched = Buffer.from(spec.patch(original.toString('utf8')), 'utf8');
    return {spec, target, original, patched};
  });
  fs.mkdirSync(backup); // Never overwrite an earlier build's backup.
  for (const file of files) fs.writeFileSync(path.join(backup, file.spec.backup), file.original);
  const state = {schema: 1, version: '15.12.1', projectRoot: root, restored: false,
    files: files.map(file => ({relative: file.spec.relative, originalHash: file.spec.hash, patchedHash: digest(file.patched)}))};
  fs.writeFileSync(path.join(backup, 'state.json'), JSON.stringify(state, null, 2) + '\n');
  try {
    for (const file of files) fs.writeFileSync(file.target, file.patched);
  } catch (error) {
    restore(backup);
    throw error;
  }
  return path.join(backup, 'state.json');
}

module.exports = {prepare, restore};
if (require.main === module) {
  try {
    const [operation, first, second] = process.argv.slice(2);
    if (operation === 'apply' && first && second) console.log(prepare(first, second));
    else if (operation === 'restore' && first) console.log(restore(first));
    else throw new Error('Usage: prepareSvgFabric.cjs apply <project> <external-backup> | restore <backup>');
  } catch (error) {console.error(error.message); process.exitCode = 1;}
}
