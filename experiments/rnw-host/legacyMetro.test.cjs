const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createLegacyMetro} = require('./legacyMetro');
test('shared numeric and per-plugin string modules stay isolated from host Metro', () => {
  const globalObject = {__r: () => 'host'};
  const loader = createLegacyMetro({globalObject});
  loader.register('__d(function(g,r,d,a,m,e,ds){m.exports={value:4};},10,[],"shared");__r(10);');
  loader.register('__d(function(g,r,d,a,m,e,ds){m.exports=r(ds[0]).value+1;},"plug-1",[10]);__r("plug-1");');
  assert.equal(loader.require('plug-1'), 5);
  assert.equal(globalObject.__r(), 'host');
  assert.deepEqual(loader.stats().startupRequests, [10, 'plug-1']);
});
test('cycles use partial exports; errors retain a module chain and never become success', () => {
  const loader = createLegacyMetro();
  loader.register('__d(function(g,r,d,a,m,e,ds){e.name="a";e.other=r(ds[0]).name;},1,[2]);__d(function(g,r,d,a,m,e,ds){e.name=r(ds[0]).name;},2,[1]);');
  assert.equal(loader.require(1).other, 'a');
  assert.throws(() => loader.require(99), /MISSING_METRO_MODULE/);
});
test('framework overrides do not run the obsolete framework factory', () => {
  const loader = createLegacyMetro({override: ({name}) => name === 'react/index.js' ? {version: 'host'} : undefined});
  loader.register('__d(function(){throw new Error("obsolete");},1,[],"react/index.js");');
  assert.equal(loader.require(1).version, 'host');
});
