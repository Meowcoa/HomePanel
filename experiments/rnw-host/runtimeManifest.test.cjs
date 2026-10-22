const {test} = require('node:test');
const assert = require('node:assert/strict');
const {parseRuntimeManifest} = require('./runtimeManifest');
test('legacy candidate retains old entry; downloaded packages supply isolated numeric entry and matched SDK', () => {
  assert.equal(parseRuntimeManifest('{}').entryModule, '1006883-10001');
  const manifest = {schema: 1, kind: 'downloaded-rn', package: 'com.vendor.plug', version: '1.0.85', sdkApiLevel: 10116, entryModule: 10001};
  assert.deepEqual(parseRuntimeManifest(JSON.stringify(manifest)), manifest);
  for (const patch of [{schema: 2}, {kind: 'arbitrary'}, {entryModule: '../entry'}, {sdkApiLevel: 1}, {package: '../escape'}]) {
    assert.throws(() => parseRuntimeManifest(JSON.stringify({...manifest, ...patch})), /MANIFEST/);
  }
  assert.throws(() => parseRuntimeManifest('[]'), /MANIFEST/);
});
