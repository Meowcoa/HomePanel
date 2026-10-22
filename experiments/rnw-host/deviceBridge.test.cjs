const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createDeviceBridge} = require('./deviceBridge');
const flush = () => new Promise(resolve => setImmediate(resolve));

test('single-device requests never require an account credential or device id', async () => {
  const calls = [];
  const bridge = createDeviceBridge({request: async raw => {
    calls.push(JSON.parse(raw)); return JSON.stringify({ok: true, result: []});
  }});
  await bridge.getDevice(); await bridge.read([{siid: 2, piid: 1}]);
  await bridge.setValue({siid: 2, piid: 1}, false);
  assert.deepEqual(calls, [{op: 'getDevice'}, {op: 'read', targets: [{siid: 2, piid: 1}]},
    {op: 'write', target: {siid: 2, piid: 1}, value: false}]);
});

test('missing and failed broker responses reject instead of returning success', async () => {
  await assert.rejects(createDeviceBridge(null).getDevice(), /BROKER_NOT_CONNECTED/);
  await assert.rejects(createDeviceBridge({request: async () => '{"ok":false,"error":"READ_ONLY_SESSION"}'}).setValue({}, true), /READ_ONLY_SESSION/);
});

test('polling is sequential, cancellation suppresses an in-flight result', async () => {
  let resolve;
  let calls = 0;
  const events = [], scheduled = [];
  const bridge = createDeviceBridge({request: () => {calls++; return new Promise(r => {resolve = r;});}},
    {setTimeout: (...args) => {scheduled.push(args); return 1;}, clearTimeout() {}});
  const sub = bridge.subscribe([{siid: 2, piid: 1}], event => events.push(event));
  await flush(); assert.equal(calls, 1); assert.equal(scheduled.length, 0);
  sub.remove(); resolve('{"ok":true,"result":[]}'); await flush();
  assert.deepEqual(events, []); assert.equal(scheduled.length, 0);
});

test('polling reports errors and backs off; removing it clears the timer', async () => {
  const events = [], scheduled = [], cleared = [];
  const bridge = createDeviceBridge({request: async () => '{"ok":false,"error":"DEVICE_OFFLINE"}'},
    {setTimeout: (...args) => {scheduled.push(args); return 9;}, clearTimeout: id => cleared.push(id)});
  const sub = bridge.subscribe([{siid: 2, piid: 1}], event => events.push(event));
  await flush();
  assert.deepEqual(events, [{transport: 'polling', error: 'DEVICE_OFFLINE'}]);
  assert.equal(scheduled[0][1], 15000);
  sub.remove(); assert.deepEqual(cleared, [9]);
});

test('a plugin listener exception is surfaced without becoming a device read failure', async () => {
  const scheduled = [], events = [];
  const failure = new Error('PLUGIN_CALLBACK_FAILED');
  const bridge = createDeviceBridge({request: async () => '{"ok":true,"result":[]}'},
    {setTimeout: (...args) => {scheduled.push(args); return scheduled.length;}, clearTimeout() {}});
  const sub = bridge.subscribe([{siid: 2, piid: 1}], event => {events.push(event); throw failure;});
  await flush();
  assert.deepEqual(events, [{transport: 'polling', values: []}]);
  assert.equal(scheduled[0][1], 3000);
  assert.equal(scheduled[1][1], 0);
  assert.throws(scheduled[1][0], error => error === failure);
  sub.remove();
});
