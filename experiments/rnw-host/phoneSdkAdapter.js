// Original compatibility implementation. No phone SDK source is embedded here.
function channel() {
  const listeners = new Set();
  return {addListener(fn) {listeners.add(fn); return {remove: () => listeners.delete(fn)};},
    emit(...args) {for (const fn of [...listeners]) fn(...args);}};
}
function eventMap() {
  const events = new Map();
  return new Proxy({}, {get(_, name) {
    if (!events.has(name)) events.set(name, channel());
    return events.get(name);
  }});
}
function unsupported(name) {return () => Promise.reject(new Error(`UNSUPPORTED_PHONE_API:${name}`));}
function createPhoneSdk({device, bridge, onEntry, onNotice = () => {}, width = 388, height = 540, manifest = {}}) {
  if (device.testData !== true && (!/^urn:miot-spec-v2:device:[a-z0-9-]+:/.test(device.spec?.type || '') || !Array.isArray(device.spec?.services))) throw new Error('PUBLIC_SPEC_REQUIRED');
  const apiLevel = Number.isSafeInteger(manifest.sdkApiLevel) ? manifest.sdkApiLevel : 10110;
  const did = device.testData ? 'homepanel-test-device' : 'homepanel-selected-device';
  const subscriptions = new Set();
  const packageEvents = eventMap(), deviceEvents = eventMap(), hostEvents = eventMap();
  const local = new Map();
  const service = new Proxy({}, {get: (obj, key) => key in obj ? obj[key] : unsupported(`Service.${String(key)}`)});
  const nativeStandardCall = async path => {throw new Error(`UNSUPPORTED_PHONE_RPC:${path}`);};
  service.getServerName = () => {
    if (device.region !== 'cn') return Promise.reject(new Error('UNSUPPORTED_PHONE_REGION'));
    return Promise.resolve({countryCode: 'CN', server: 'cn'});
  };
  async function speakerRequest(path, method, message) {
    if (path !== 'mediaplayer' || method !== 'player_get_play_status' || !message || typeof message !== 'object' ||
        Array.isArray(message) || Object.keys(message).length) throw new Error('UNSUPPORTED_PHONE_API:Speaker.request');
    return bridge.speakerRequest(path, method, message);
  }
  async function nativeCall(path, params) {
    if (path !== '/v2/remote/ubus' || !params || typeof params !== 'object' || Array.isArray(params) ||
        Object.keys(params).some(key => !['did','path','method','message'].includes(key))) throw new Error('UNSUPPORTED_PHONE_RPC');
    if (String(params.did) !== did) throw new Error('CROSS_DEVICE_REQUEST_DENIED');
    return speakerRequest(params.path, params.method, params.message);
  }
  function validate(params) {
    if (!Array.isArray(params) || params.length > 12) throw new Error('INVALID_SPEC_TARGETS');
    return params.map(p => {
      if (String(p.did) !== did) throw new Error('CROSS_DEVICE_REQUEST_DENIED');
      return {siid: p.siid, piid: p.piid, ...(p.miid === undefined ? {} : {miid: p.miid})};
    });
  }
  service.spec = {
    async getPropertiesValue(params) {
      const rows = await bridge.read(validate(params));
      if (rows.some(row => !row.available)) throw new Error('DIRECT_STATE_UNAVAILABLE');
      return rows.map(row => ({did, siid: row.siid, piid: row.piid, code: 0, value: row.value}));
    },
    async setPropertiesValue(params) {
      if (!device.testData && !device.writesAllowed) throw new Error('READ_ONLY_SESSION');
      const targets = validate(params);
      const result = [];
      for (let i = 0; i < targets.length; i++) {
        const row = await bridge.setValue(targets[i], params[i].value);
        if (!row.available || row.value !== params[i].value) throw new Error('WRITE_UNCONFIRMED');
        result.push({did, siid: row.siid, piid: row.piid, code: 0});
      }
      return result;
    },
    async doAction(params) {
      if (!device.testData && !device.writesAllowed) throw new Error('READ_ONLY_SESSION');
      if (!params || typeof params !== 'object' || Array.isArray(params) || !Array.isArray(params.in) ||
          Object.keys(params).some(key => !['did','siid','aiid','miid','in'].includes(key))) throw new Error('INVALID_SPEC_ACTION');
      if (String(params.did) !== did) throw new Error('CROSS_DEVICE_REQUEST_DENIED');
      const target = {siid: params.siid, aiid: params.aiid, ...(params.miid === undefined ? {} : {miid: params.miid})};
      // Rust checks the selected published SPEC and reports acceptance, never invented state.
      return {did, ...await bridge.action(target, params.in)};
    },
  };
  // Telemetry calls in the phone package must not become rejected promises
  // during Windows rendering. They carry no device-control semantics here.
  service.smarthome = new Proxy({reportEvent() {onNotice('analytics-disabled'); return Promise.resolve({code: 0});}},
    {get: (obj, name) => name in obj ? obj[name] : unsupported(`Smarthome.${String(name)}`)});
  service.scene = new Proxy({async loadScenes(selectedId, type) {
    if (String(selectedId) !== did) throw new Error('CROSS_DEVICE_REQUEST_DENIED');
    if (type !== 8) throw new Error('UNSUPPORTED_PHONE_API:Scene.type');
    const rows = await bridge.loadScenes(type);
    if (!Array.isArray(rows)) throw new Error('INVALID_SCENE_RESPONSE');
    return rows.map(row => ({...row,
      // Phone plugins sometimes delete disabled timers during initialization; never authorize that.
      remove: unsupported('Scene.remove'), save: unsupported('Scene.save'),
    }));
  }}, {get: (obj, name) => name in obj ? obj[name] : unsupported(`Scene.${String(name)}`)});
  service.callXiaoaiNetworkAPI = async options => {
    if (options?.host === 'normal' && options.path === '/admin/v2/device_list' && options.method === 0 &&
        options.needDevice === 0 && options.params?.presence === true && options.params?.master === false &&
        Object.keys(options.params).every(key => ['presence','master'].includes(key)) &&
        device.model === 'xiaomi.wifispeaker.l05c') {
      // This is an explicitly scoped view of the already selected MIoT device,
      // not an invented account-wide Xiaoai discovery result.
      return {code: 0, data: [{deviceID: did, miotDID: did, hardware: 'L05C',
        presence: device.online ? 'online' : 'offline', name: device.name, deviceSNProfile: null}]};
    }
    if (!options || typeof options !== 'object' || options.host !== 'normal' || options.path !== '/remote/ubus' ||
        options.method !== 1 || Object.keys(options).some(key => !['host','path','method','needDevice','params'].includes(key))) throw new Error('UNSUPPORTED_PHONE_API:Xiaoai.request');
    const params = options.params;
    if (!params || typeof params !== 'object' || Object.keys(params).some(key => !['deviceId','path','method','message'].includes(key))) throw new Error('INVALID_SPEAKER_REQUEST');
    if (String(params.deviceId) !== did) throw new Error('CROSS_DEVICE_REQUEST_DENIED');
    let message;
    try {message = typeof params.message === 'string' ? JSON.parse(params.message) : params.message;} catch {throw new Error('INVALID_SPEAKER_REQUEST');}
    const reply = await speakerRequest(params.path, params.method, message);
    if (!reply || reply.code !== 0 || reply.result == null) throw new Error('DIRECT_STATE_UNAVAILABLE');
    return {code: reply.code, data: reply.result};
  };
  service.analytics = {track() {onNotice('analytics-disabled');}};
  const storage = {
    get: key => Promise.resolve(local.get(key)),
    set: (key, value) => {local.set(key, value); return Promise.resolve();},
    remove: key => {local.delete(key); return Promise.resolve();},
    getStorage: key => Promise.resolve(local.get(key)),
    setStorage: (key, value) => {local.set(key, value); return Promise.resolve();},
    removeStorage: key => {local.delete(key); return Promise.resolve();},
  };
  const host = {
    locale: {language: 'zh', systemLanguage: 'zh_CN', is24HourTime: true},
    systemInfo: {sysVersion: 'Windows', hostApiLevel: apiLevel, language: 'zh', model: 'HomePanel'},
    storage, file: new Proxy({}, {get: (_, name) => unsupported(`File.${String(name)}`)}),
    isPad: false, isIphoneXSeries: false, isDebug: device.testData === true, apiLevel, appVersion: '0.4.3', version: '0.4.3',
    safeAreaInsets: {top: 0, right: 0, bottom: 0, left: 0},
    getSafeAreaInsets: () => Promise.resolve({top: 0, right: 0, bottom: 0, left: 0}),
    screen: {width, height},
    ui: new Proxy({
      showToast: text => onNotice(String(text)),
      openDeviceMorePage: unsupported('Host.ui.openDeviceMorePage'),
      alertLegalInformationAuthorization: unsupported('Host.ui.alertLegalInformationAuthorization'),
      checkStoreSupporttedOnAndroid: () => false,
    }, {get: (obj, key) => key in obj ? obj[key] : unsupported(`Host.ui.${String(key)}`)}),
  };
  const nativeModules = {
    getServerName(callback) {service.getServerName().then(reply => callback(true, reply), () => callback(false, {code:'UNSUPPORTED_PHONE_REGION'}));},
  };
  host.crypto = {decodeBase64(value) {
    if (value == null || value === '') return Promise.resolve(null);
    return Promise.reject(new Error('UNSUPPORTED_PHONE_API:Crypto.decodeBase64'));
  }};
  const instance = device.spec || {type: 'urn:miot-spec-v2:device:outlet:0000A002:homepanel-test:1', description: 'Outlet',
    services: [{iid: 2, type: 'urn:miot-spec-v2:service:switch:0000780C:homepanel-test:1', description: 'Switch',
      properties: [{iid: 1, type: 'urn:miot-spec-v2:property:on:00000006:homepanel-test:1', description: 'On', format: 'bool', access: ['read', 'write', 'notify']}]}]};
  const labels = {};
  for (const p of device.properties || []) {
    labels[`service:${p.siid}:property:${p.piid}`] = p.name;
    for (const [i, option] of (p.options || []).entries()) labels[`service:${p.siid}:property:${p.piid}:valuelist:${i}`] = option.name;
  }
  const pkg = {
    entryInfo: {resource: {instance, i18n: {zh: {...(device.testData ? {'service:2': '插座', 'service:2:property:1': '电源'} : {}), ...labels}}, custom_config: {}}},
    entrance: 0, isFirstEnter: false, isNewRoom: false, isDebug: device.testData === true, apiLevel,
    packageID: manifest.package || 'miot.plugin.spec', version: manifest.version || '1.1.10', versionCode: 1,
    models: device.model, pageParams: {},
    entry(Component) {onEntry(Component);},
    exit: () => {onNotice('plugin-requested-exit');},
  };
  const selected = {
    deviceID: did, model: device.model, name: device.name, isOnline: device.online, isOnlineFromCloud: device.online, type: 0,
    specType: instance.type, spec_urn: instance.type, deviceIcon: '',
    deviceConfigInfo: {},
    mac: '', isOwner: false, isShared: false, isSetPinCode: false,
    DEVICE_TYPE: {WIFI_DEVICE: 0, BLUETOOTH_SINGLE_MODEL_DEVICE: 6, BLE_MESH_DEVICE: 16},
    getDeviceInfo: () => Promise.resolve({did, model: device.model, name: device.name, isOnline: device.online}),
    subscribeMessages(...properties) {
      const keys = properties.flat();
      const targets = keys.map(key => {
        const parts = String(key).match(/(?:prop\.)?(\d+)\.(\d+)$/);
        if (!parts) throw new Error('UNSUPPORTED_PROPERTY_SUBSCRIPTION');
        return {siid: Number(parts[1]), piid: Number(parts[2])};
      });
      const subscription = bridge.subscribe(targets, event => {
        if (event.error) {onNotice(`property-poll-failed:${event.error}`); return;}
        const props = new Map(), records = [];
        for (const row of event.values) if (row.available && Number.isFinite(row.checkedAt)) {
          const key = `prop.${row.siid}.${row.piid}`, value = [row.value];
          props.set(key, value);
          records.push({key, value, time: Math.floor(row.checkedAt / 1000)});
        }
        if (props.size) deviceEvents.deviceReceivedMessages.emit(selected, props, records);
      });
      const managed = {remove() {subscription.remove();subscriptions.delete(managed);}};
      subscriptions.add(managed);
      return Promise.resolve(managed);
    },
    unsubscribeMessages: unsupported('Device.unsubscribeMessages'),
    // The phone SDK's Wi-Fi facade also owns MIoT property subscriptions. Keep
    // these on the selected-device broker; this does not expose a LAN socket.
    getDeviceWifi: () => ({subscribeMessages: (...properties) => selected.subscribeMessages(...properties)}),
  };
  const sdk = {__esModule: true, API_LEVEL: apiLevel, Device: selected, Package: pkg, Host: host, Service: service,
    DeviceEvent: deviceEvents, PackageEvent: packageEvents, HostEvent: hostEvents,
    FileEvent: eventMap(), PrivacyEvent: eventMap(),
    CLOUD_PRIVACY_EVENT_TYPES: Object.freeze({AGREED:'agreed',FAILED:'failed',POP_DIALOG_SUCCESS:'pop_dialog_success'}),
    SceneType: Object.freeze({Timer: 8, Artificial: 30, Automatic: 15}),
    Entrance: {Main: 0, Scene: 1}, DarkMode: {getColorScheme: () => 'light', addChangeListener: () => ({remove() {}})},
    System: {
      darkmode: {isDarkMode: false},
      network: {getGatewayIpAddress: () => Promise.resolve({data: {ipAddress: ''}})},
    },
    Resources: {getLanguage: () => host.locale.language}};
  sdk.default = sdk;
  return {sdk, instance, nativeCall, nativeStandardCall, nativeModules,
    startLifecycle() {
      packageEvents.packageWillLoad.emit();
      packageEvents.packageDidLoaded.emit(); packageEvents.packageViewWillAppear.emit();
    },
    async startPrivacyLifecycle() {
      const status = await bridge.getPrivacyStatus();
      const expectedSource = device.testData ? 'test-fixture' : 'existing-cloud-record';
      if (status?.authorized !== true || status.source !== expectedSource) throw new Error('PRIVACY_REQUIRED');
      sdk.PrivacyEvent.cloudPrivacyEvent.emit({eventType:'agreed',source:expectedSource});
    },
    stopLifecycle() {for (const sub of [...subscriptions]) sub.remove();packageEvents.packageWillExit.emit();},
    channels: {packageEvents, deviceEvents, hostEvents}};
}
module.exports = {createPhoneSdk, channel};
