const {test} = require('node:test');
const assert = require('node:assert/strict');
const {parseRamBundle, decodeBase64, decodeUtf8} = require('./ramBundle');
const {createLegacyMetro} = require('./legacyMetro');
function ram(rows, startup = '__r(1);') {
  const slots = Math.max(...Object.keys(rows).map(Number)) + 1;
  const head = Buffer.alloc(12 + slots * 8), chunks = [Buffer.from(startup + '\0')];
  head.writeUInt32LE(0xfb0bd1e5); head.writeUInt32LE(slots, 4); head.writeUInt32LE(chunks[0].length, 8);
  let offset = chunks[0].length;
  for (const [id, source] of Object.entries(rows)) {
    const code = Buffer.from(source + '\0');
    head.writeUInt32LE(offset, 12 + Number(id) * 8); head.writeUInt32LE(code.length, 16 + Number(id) * 8);
    chunks.push(code); offset += code.length;
  }
  return Buffer.concat([head, ...chunks]);
}
test('RAM local IDs are independent from shared SDK and host Metro', () => {
  const sdk = createLegacyMetro();
  sdk.register('__d(function(g,r,d,a,m){m.exports=9;},1,[],"sdk");__d(function(g,r,d,a,m){m.exports=2;},7,[],"external");');
  const globals = {__r: () => 'host'};
  const plugin = createLegacyMetro({globalObject: globals, fallback: sdk.require});
  plugin.registerRam(ram({1: '__d(function(g,r,d,a,m,e,deps){m.exports=r(deps[0])+3;},1,[7]);'}));
  assert.equal(plugin.stats().evaluated, 0);
  assert.equal(plugin.require(1), 5);
  assert.equal(sdk.require(1), 9);
  assert.equal(globals.__r(), 'host');
  assert.deepEqual(plugin.stats().startupRequests, [1]);
});
test('RAM cycles, imports and startup comments retain Metro semantics', () => {
  const plugin = createLegacyMetro();
  plugin.registerRam(ram({
    1: '__d(function(g,r,d,a,m,e,ds){e.name="a";e.other=r(ds[0]);},1,[2]);',
    2: '__d(function(g,r,d,a,m,e,ds){m.exports={cycle:r(ds[0]).name,def:d(ds[1]),all:a(ds[1])};},2,[1,3]);',
    3: '__d(function(g,r,d,a,m){m.exports={value:4};},3,[]);',
  }, '__r(1);\n//# sourceMappingURL=map.json'));
  assert.deepEqual(plugin.require(1).other, {cycle: 'a', def: {value: 4}, all: {value: 4, default: {value: 4}}});
});
test('RAM rejects malformed tables, overlap and invalid source before execution', () => {
  const original = ram({1: '__d(function(){},1,[]);', 2: '__d(function(){},2,[]);'});
  const mutate = fn => {const copy = Buffer.from(original); fn(copy); return copy;};
  assert.throws(() => parseRamBundle(original.subarray(0, 11)), /SIZE/);
  assert.throws(() => parseRamBundle(mutate(b => b.writeUInt32LE(250001, 4))), /SLOTS/);
  assert.throws(() => parseRamBundle(mutate(b => b.writeUInt32LE(1, 20))), /MODULE_BOUNDS/);
  assert.throws(() => parseRamBundle(mutate(b => b.writeUInt32LE(b.readUInt32LE(20), 28))), /MODULE_OVERLAP/);
  assert.throws(() => parseRamBundle(mutate(b => b[b.length - 1] = 1)), /TERMINATOR/);
  assert.throws(() => parseRamBundle(mutate(b => b[b.length - 2] = 0xff)), /UTF8/);
});
test('RAM rejects definition identity mismatch and extra definitions', () => {
  for (const code of ['__d(function(){},9,[]);', '__d(function(){},1,[]);__d(function(){},1,[]);']) {
    const loader = createLegacyMetro(); loader.registerRam(ram({1: code}));
    assert.throws(() => loader.require(1), /INVALID_RAM_DEFINITION/);
  }
  assert.throws(() => createLegacyMetro().registerRam(ram({1: '__d(function(){},1,[]);'}, 'global.foo=1;')), /STARTUP/);
});
test('legacy bare React and IntlPolyfill references remain scoped to the package', () => {
  const nativeIntl = {DateTimeFormat() {}, NumberFormat() {}};
  const host = {Intl: nativeIntl};
  const react = {cloneElement: () => 'cloned'};
  const loader = createLegacyMetro({globalObject: host, react});
  loader.registerRam(ram({1: '__d(function(g,r,d,a,m){IntlPolyfill.__addLocaleData({});m.exports=React.cloneElement();},1,[]);'}));
  assert.equal(loader.require(1), 'cloned');
  assert.equal(host.React, undefined);
  assert.equal(host.IntlPolyfill, undefined);
  assert.equal(nativeIntl.__addLocaleData, undefined);
});
test('UTF8 and binary transport handle Chinese and supplementary characters strictly', () => {
  const text = '米家😀';
  assert.equal(decodeUtf8(Buffer.from(text)), text);
  assert.equal(Buffer.from(decodeBase64(Buffer.from(text).toString('base64'))).toString(), text);
  for (const bytes of [[0xc0, 0x80], [0xed, 0xa0, 0x80], [0xf4, 0x90, 0x80, 0x80], [0xe0, 0x80], [0xe2, 0x20, 0xa0]]) {
    assert.throws(() => decodeUtf8(Uint8Array.from(bytes)), /UTF8/);
  }
  for (const value of ['A', 'AB==', 'AAB=', 'AA A', '!!!!']) assert.throws(() => decodeBase64(value), /BASE64/);
});
module.exports = {ram};
