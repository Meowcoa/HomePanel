// Fixture-only end-to-end check through the same SDK adapter exposed to the plugin.
// This exercises SDK calls, not an official plugin button press or a physical device.
async function verifyPhoneSdkBridge(sdk, testData) {
  if (testData !== true || sdk.Device.deviceID !== 'homepanel-test-device') {
    throw new Error('SDK_SELF_TEST_REQUIRES_FIXTURE');
  }
  const target = {did: sdk.Device.deviceID, siid: 2, piid: 1};
  const spec = sdk.Service.spec;
  const read = async expected => {
    const rows = await spec.getPropertiesValue([target]);
    if (rows.length !== 1 || rows[0].code !== 0 || rows[0].value !== expected) {
      throw new Error('SDK_SELF_TEST_READBACK_FAILED');
    }
  };
  const write = async value => {
    const rows = await spec.setPropertiesValue([{...target, value}]);
    if (rows.length !== 1 || rows[0].code !== 0) throw new Error('SDK_SELF_TEST_WRITE_UNCONFIRMED');
  };
  let subscription, listener, timeout, changed = false;
  try {
    await read(false);
    const notified = new Promise((resolve, reject) => {
      timeout = setTimeout(() => reject(new Error('SDK_SELF_TEST_POLL_TIMEOUT')), 10000);
      listener = sdk.DeviceEvent.deviceReceivedMessages.addListener((device, values, records) => {
        if (device.deviceID === target.did && values instanceof Map && values.get('prop.2.1')?.[0] === true
            && records.some(row => row.key === 'prop.2.1' && Number.isFinite(row.time))) resolve();
      });
    });
    // Attach a rejection handler before asynchronous native work to avoid an unhandled timeout.
    notified.catch(() => {});
    subscription = await sdk.Device.subscribeMessages('prop.2.1');
    changed = true;
    await write(true);
    await read(true);
    await notified;
    return {read: true, writeConfirmed: true, independentReadback: true, subscriptionTransport: 'polling', fixtureRestored: true};
  } finally {
    clearTimeout(timeout);
    subscription?.remove();
    listener?.remove();
    if (changed) {await write(false); await read(false);}
  }
}
module.exports = {verifyPhoneSdkBridge};
