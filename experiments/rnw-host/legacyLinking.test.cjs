const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createLegacyLinking} = require('./legacyLinking');

test('old removeEventListener removes the modern subscription once, including a later remove call', () => {
  const calls = [];
  const source = {
    addEventListener(type, listener) {
      calls.push(['add', type, listener]);
      return {remove() {calls.push(['remove', type, listener]);}};
    },
    openURL(url) {return Promise.resolve(url);},
  };
  const legacy = createLegacyLinking(source), listener = () => {};
  const subscription = legacy.addEventListener('url', listener);
  legacy.removeEventListener('url', listener);
  subscription.remove();
  assert.deepEqual(calls, [['add', 'url', listener], ['remove', 'url', listener]]);
  assert.equal(legacy.openURL, source.openURL);
});

test('falls back to the old native remover when the modern API returns no subscription', () => {
  const calls = [];
  const listener = () => {};
  const source = {
    addEventListener(type, callback) {calls.push(['add', type, callback]);},
    removeEventListener(type, callback) {calls.push(['remove', type, callback]);},
  };
  const legacy = createLegacyLinking(source);
  const subscription = legacy.addEventListener('url', listener);
  legacy.removeEventListener('url', listener);
  subscription.remove();
  assert.deepEqual(calls, [['add', 'url', listener], ['remove', 'url', listener]]);
});
