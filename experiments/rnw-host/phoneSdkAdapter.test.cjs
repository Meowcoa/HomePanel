const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createPhoneSdk} = require('./phoneSdkAdapter');

test('device message events contain the legacy Map and timestamped source records', async () => {
  let receive, removed = false;
  const phone = createPhoneSdk({device: {testData: true, model: 'homepanel.test.plug'},
    bridge: {subscribe(targets, listener) {
      assert.deepEqual(targets, [{siid: 2, piid: 1}]);
      receive = listener;
      return {remove() {removed = true;}};
    }}});
  const events = [];
  phone.sdk.DeviceEvent.deviceReceivedMessages.addListener((...args) => events.push(args));
  const subscription = await phone.sdk.Device.subscribeMessages('prop.2.1');
  receive({values: [{siid: 2, piid: 1, available: true, value: true, checkedAt: 1700000000123},
    {siid: 2, piid: 2, available: false, value: 99, checkedAt: 1700000000123}]});
  assert.equal(events.length, 1);
  assert.equal(events[0][0], phone.sdk.Device);
  assert.ok(events[0][1] instanceof Map);
  assert.deepEqual([...events[0][1]], [['prop.2.1', [true]]]);
  assert.deepEqual(events[0][2], [{key: 'prop.2.1', value: [true], time: 1700000000}]);
  subscription.remove();
  assert.equal(removed, true);
});

test('product sessions use the public SPEC, alias the device id, and reject writes in read-only mode', async () => {
  const spec = {type: 'urn:miot-spec-v2:device:outlet:1', services: [{iid: 7, properties: [{iid: 3, format: 'bool'}]}]};
  let written = 0;
  const phone = createPhoneSdk({device: {testData: false, spec, writesAllowed: false, model: 'vendor.plug.v1'},
    bridge: {setValue() {written++;}}});
  assert.equal(phone.instance, spec);
  assert.equal(phone.sdk.Device.deviceID, 'homepanel-selected-device');
  await assert.rejects(phone.sdk.Service.spec.setPropertiesValue([{did: 'homepanel-selected-device', siid: 7, piid: 3, value: true}]), /READ_ONLY_SESSION/);
  assert.equal(written, 0);
  assert.throws(() => createPhoneSdk({device: {testData: false}}), /PUBLIC_SPEC_REQUIRED/);
});

test('real-session writes require a confirmed echo and reject cross-device requests; exit removes polling', async () => {
  let removed = false, writes = 0;
  const phone = createPhoneSdk({device: {testData: false, writesAllowed: true,
    spec: {type: 'urn:miot-spec-v2:device:outlet:1', services: []}}, bridge: {
    async setValue() {writes++;return {available: true, value: false};},
    subscribe() {return {remove() {removed = true;}};},
  }});
  await assert.rejects(phone.sdk.Service.spec.setPropertiesValue([{did: 'other', siid: 2, piid: 1, value: true}]), /CROSS_DEVICE_REQUEST_DENIED/);
  assert.equal(writes, 0);
  await assert.rejects(phone.sdk.Service.spec.setPropertiesValue([{did: 'homepanel-selected-device', siid: 2, piid: 1, value: true}]), /WRITE_UNCONFIRMED/);
  await phone.sdk.Device.subscribeMessages('prop.2.1');
  phone.stopLifecycle();
  assert.equal(removed, true);
});

test('downloaded package identity, API level and published actions are scoped to selected device', async () => {
  const calls = [];
  const phone = createPhoneSdk({device: {testData: false, writesAllowed: true,
    spec: {type: 'urn:miot-spec-v2:device:speaker:1', services: []}},
    manifest: {package: 'com.vendor.speaker', version: '1.2.3', sdkApiLevel: 10116}, bridge: {
      action: async (target, inputs) => {calls.push({target, inputs});return {code: 0, accepted: true, confirmed: false};},
    }});
  assert.equal(phone.sdk.API_LEVEL, 10116);
  assert.equal(phone.sdk.Package.packageID, 'com.vendor.speaker');
  const request = {did: 'homepanel-selected-device', siid: 2, aiid: 1, in: []};
  await assert.rejects(phone.sdk.Service.spec.doAction({...request, did: 'other'}), /CROSS_DEVICE/);
  await assert.rejects(phone.sdk.Service.spec.doAction({...request, endpoint: '/arbitrary'}), /INVALID_SPEC_ACTION/);
  await assert.rejects(phone.sdk.Service.spec.doAction({...request, in: {}}), /INVALID_SPEC_ACTION/);
  assert.equal(calls.length, 0);
  const result = await phone.sdk.Service.spec.doAction(request);
  assert.deepEqual(calls, [{target: {siid: 2, aiid: 1}, inputs: []}]);
  assert.equal(result.confirmed, false);
  assert.equal(result.did, request.did);
});

test('real-session read-only mode rejects actions before broker access', async () => {
  let called = false;
  const phone = createPhoneSdk({device: {writesAllowed: false, spec: {type: 'urn:miot-spec-v2:device:light:1', services: []}},
    bridge: {action: () => {called = true;}}});
  await assert.rejects(phone.sdk.Service.spec.doAction({did: 'homepanel-selected-device', siid: 2, aiid: 1, in: []}), /READ_ONLY_SESSION/);
  assert.equal(called, false);
});

test('speaker native RPC exposes only selected read-only play-status and preserves raw result', async () => {
  const raw = {code: 0, result: {code: 0, info: '{"status":1,"volume":10}'}};
  const calls = [];
  const phone = createPhoneSdk({device: {testData: true, model: 'xiaomi.wifispeaker.l05c'},
    bridge: {speakerRequest: async (...args) => {calls.push(args);return raw;}}});
  const request = {did: 'homepanel-test-device', path: 'mediaplayer', method: 'player_get_play_status', message: {}};
  assert.equal(await phone.nativeCall('/v2/remote/ubus', request), raw);
  const response = await phone.sdk.Service.callXiaoaiNetworkAPI({host: 'normal', path: '/remote/ubus', method: 1,
    params: {deviceId: 'homepanel-test-device', path: 'mediaplayer', method: 'player_get_play_status', message: '{}'}});
  assert.deepEqual(response, {code: 0, data: raw.result});
  await assert.rejects(phone.nativeCall('/v2/remote/ubus', {...request, did: 'other'}), /CROSS_DEVICE/);
  await assert.rejects(phone.nativeCall('/arbitrary', request), /UNSUPPORTED_PHONE_RPC/);
  await assert.rejects(phone.nativeCall('/v2/remote/ubus', {...request, method: 'player_play_operation', message: {action: 'play'}}), /UNSUPPORTED_PHONE_API/);
  await assert.rejects(phone.nativeCall('/v2/remote/ubus', {...request, message: {unexpected: true}}), /UNSUPPORTED_PHONE_API/);
  assert.equal(calls.length, 2);
});

test('loaded timer scenes cannot be deleted or saved by plugin lifecycle side effects', async () => {
  const phone = createPhoneSdk({device: {testData: true}, bridge: {loadScenes: async type => {
    assert.equal(type, 8);return [{name: '测试定时器', setting: {enable_timer: '0', timer_type: '1'}}];
  }}});
  const rows = await phone.sdk.Service.scene.loadScenes('homepanel-test-device', phone.sdk.SceneType.Timer);
  assert.equal(rows.length, 1);
  await assert.rejects(rows[0].remove(), /UNSUPPORTED_PHONE_API:Scene.remove/);
  await assert.rejects(rows[0].save(), /UNSUPPORTED_PHONE_API:Scene.save/);
  await assert.rejects(phone.sdk.Service.scene.loadScenes('other', 8), /CROSS_DEVICE/);
  await assert.rejects(phone.sdk.Service.scene.loadScenes('homepanel-test-device', 30), /UNSUPPORTED_PHONE_API/);
  let consentEvents = 0;
  phone.sdk.PrivacyEvent.cloudPrivacyEvent.addListener(() => consentEvents++);
  phone.startLifecycle();
  assert.equal(consentEvents, 0);
});

test('Wi-Fi facade subscriptions keep the same selected-device scope and are removed on exit', async () => {
  let receive, removed = false;
  const phone = createPhoneSdk({device: {testData: true}, bridge: {subscribe(targets, listener) {
    assert.deepEqual(targets, [{siid: 2, piid: 1}, {siid: 2, piid: 6}]);
    receive = listener;
    return {remove() {removed = true;}};
  }}});
  const events = [];
  phone.sdk.DeviceEvent.deviceReceivedMessages.addListener((...args) => events.push(args));
  await phone.sdk.Device.getDeviceWifi().subscribeMessages('prop.2.1', 'prop.2.6');
  receive({values: [{siid: 2, piid: 1, available: true, value: false, checkedAt: 1700000000000}]});
  assert.equal(events[0][0].deviceID, 'homepanel-test-device');
  assert.deepEqual([...events[0][1]], [['prop.2.1', [false]]]);
  phone.stopLifecycle();
  assert.equal(removed, true);
});

test('privacy lifecycle never emits consent on startup or accepts a fixture as a real consent record', async () => {
  let record = {authorized:false,source:'existing-cloud-record'};
  const phone = createPhoneSdk({device:{model:'xiaomi.wifispeaker.l05c',
    spec:{type:'urn:miot-spec-v2:device:speaker:1',services:[]}},bridge:{getPrivacyStatus:async()=>record}});
  const events=[];
  phone.sdk.PrivacyEvent.cloudPrivacyEvent.addListener(value=>events.push(value));
  phone.startLifecycle();
  assert.equal(events.length,0);
  await assert.rejects(phone.startPrivacyLifecycle(),/PRIVACY_REQUIRED/);
  record={authorized:true,source:'test-fixture'};
  await assert.rejects(phone.startPrivacyLifecycle(),/PRIVACY_REQUIRED/);
  assert.equal(events.length,0);
  record={authorized:true,source:'existing-cloud-record'};
  await phone.startPrivacyLifecycle();
  assert.deepEqual(events,[{eventType:'agreed',source:'existing-cloud-record'}]);
  await assert.rejects(phone.nativeStandardCall('/device/batch_set_props',{}),/UNSUPPORTED_PHONE_RPC/);
});

test('speaker discovery view is scoped to the selected known model and keeps offline state', async () => {
  const phone=createPhoneSdk({device:{testData:true,region:'cn',model:'xiaomi.wifispeaker.l05c',name:'测试音箱',online:false},bridge:{}});
  const req={host:'normal',path:'/admin/v2/device_list',method:0,needDevice:0,params:{presence:true,master:false}};
  const rows=await phone.sdk.Service.callXiaoaiNetworkAPI(req);
  assert.equal(rows.data.length,1);
  assert.equal(rows.data[0].deviceID,'homepanel-test-device');
  assert.equal(rows.data[0].presence,'offline');
  await assert.rejects(phone.sdk.Service.callXiaoaiNetworkAPI({...req,needDevice:1}),/UNSUPPORTED_PHONE_API/);
  assert.equal((await phone.sdk.Service.getServerName()).countryCode,'CN');
});
