function createWindowsNetInfo(nativeHost) {
  async function fetch() {
    if (!nativeHost?.getNetworkStatus) throw new Error('NETWORK_STATUS_UNAVAILABLE');
    return JSON.parse(await nativeHost.getNetworkStatus());
  }
  function addEventListener(listener) {
    let active = true, timer, previous;
    async function poll() {
      try {
        const state = await fetch(), serialized = JSON.stringify(state);
        if (active && previous !== serialized) {previous = serialized; listener(state);}
      } catch (_) {
        if (active && previous !== 'unknown') {previous = 'unknown'; listener({type: 'unknown', isConnected: null, isInternetReachable: null, details: null});}
      }
      if (active) timer = setTimeout(poll, 5000);
    }
    poll();
    return () => {active = false; if (timer !== undefined) clearTimeout(timer);};
  }
  const api = {fetch, addEventListener,
    NetInfoStateType: {unknown: 'unknown', none: 'none', wifi: 'wifi', ethernet: 'ethernet', cellular: 'cellular', other: 'other'}};
  return {__esModule: true, default: api, ...api};
}
module.exports = {createWindowsNetInfo};
