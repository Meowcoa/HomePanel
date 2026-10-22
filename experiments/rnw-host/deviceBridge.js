// Original per-device contract. Polling listeners are not cloud push subscriptions.
function createDeviceBridge(native, timers = {setTimeout, clearTimeout}) {
  async function request(payload) {
    if (!native?.request) throw new Error('BROKER_NOT_CONNECTED');
    const response = JSON.parse(await native.request(JSON.stringify(payload)));
    if (response.ok !== true) throw new Error(response.error || 'BROKER_RESPONSE_INVALID');
    return response.result;
  }
  const read = targets => request({op: 'read', targets});
  return {
    getDevice: () => request({op: 'getDevice'}),
    getCapabilities: () => request({op: 'getCapabilities'}),
    getPrivacyStatus: () => request({op: 'getPrivacyStatus'}),
    read,
    setValue: (target, value) => request({op: 'write', target, value}),
    action: (target, inputs) => request({op: 'action', target, inputs}),
    loadScenes: type => request({op: 'loadScenes', type}),
    speakerRequest: (path, method, message) => request({op: 'speakerRequest', path, method, message}),
    subscribe(targets, listener) {
      let active = true;
      let timer;
      // Copy the caller's target list so a later edit cannot change the subscription.
      const fixedTargets = targets.map(target => ({...target}));
      async function poll() {
        let delay = 3000;
        let event;
        try {
          event = {transport: 'polling', values: await read(fixedTargets)};
        } catch (error) {
          delay = 15000;
          event = {transport: 'polling', error: error.message};
        }
        if (active) {
          timer = timers.setTimeout(poll, delay);
          try {listener(event);} catch (error) {
            // Report plugin exceptions to the JS error handler; never label them as a device outage.
            timers.setTimeout(() => {throw error;}, 0);
          }
        }
      }
      poll();
      return {remove() {active = false; if (timer !== undefined) timers.clearTimeout(timer);}};
    },
  };
}
module.exports = {createDeviceBridge};
