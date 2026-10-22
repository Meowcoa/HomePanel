// Original development screen. No Xiaomi plugin or account credentials are loaded.
import React, {useEffect, useState} from 'react';
import {AppRegistry, DeviceEventEmitter, NativeModules, Pressable, Text, View} from 'react-native';
import DevicePanel from './DevicePanel';
import OfficialPluginLab from './OfficialPluginLab';
const {createDeviceBridge} = require('./deviceBridge');

const host = NativeModules.HomePanelHost;

function HostExperiment() {
  const [enabled, setEnabled] = useState(false);
  const [status, setStatus] = useState('正在检查原生桥接…');
  const [connected, setConnected] = useState(false);
  const [officialLab, setOfficialLab] = useState(false);
  const [runtimeMode, setRuntimeMode] = useState(false);
  useEffect(() => {
    if (!host) { setStatus('原生模块不可用'); return; }
    const subscription = DeviceEventEmitter.addListener('onHostDiagnostic', value => {
      if (value === 'homepanel-host-test-v1') {
        host.reportStage('event');
        setStatus('原生调用与事件往返通过');
      }
    });
    host.getCapabilities().then(raw => {
      const caps = JSON.parse(raw);
      if (caps.protocol !== 1 || caps.officialPluginCompatible !== false) {
        throw new Error('Unsupported host contract');
      }
      host.reportStage('promise');
      host.requestDiagnosticEvent();
      setConnected(caps.deviceBrokerConnected);
      setOfficialLab(caps.localPluginTest === true || caps.localPluginRuntime === true);
      setRuntimeMode(caps.localPluginRuntime === true);
      if (caps.brokerSelfTest) {
        const bridge = createDeviceBridge(NativeModules.HomePanelBroker);
        const target = {siid: 2, piid: 1};
        return bridge.getDevice().then(async device => {
          if (!device.testData || device.model !== 'homepanel.test.plug') throw new Error('Invalid test fixture');
          const initial = await bridge.read([target]);
          if (initial[0].value !== false) throw new Error('Initial state missing');
          const changed = await bridge.setValue(target, true);
          if (!changed.available || changed.value !== true) throw new Error('Write not confirmed');
          // A subscription performs a separate read, and explicitly reports polling.
          const subscription = bridge.subscribe([target], event => {
            if (event.transport === 'polling' && event.values?.[0]?.value === true) {
              host.reportStage('broker'); subscription.remove();
            }
          });
        });
      }
    }).catch(() => setStatus('原生桥接检查失败'));
    return () => subscription.remove();
  }, []);
  if (officialLab) return <OfficialPluginLab runtimeMode={runtimeMode} />;
  return (
    <View onLayout={() => host?.reportStage('layout')}
      style={{padding: 24, backgroundColor: '#edf4f0', flex: 1}}>
      <Text style={{fontSize: 22, color: '#243b31'}}>HomePanel 宿主试验</Text>
      <Text style={{marginVertical: 16}}>{connected ? '单设备接口开发预览' : '自有测试界面 · 未连接真实设备'}</Text>
      <Text style={{marginBottom: 16, color: '#52665d'}}>{status}</Text>
      {connected && <DevicePanel />}
      <Pressable accessibilityRole="button" onPress={() => setEnabled(value => !value)}
        style={{padding: 18, borderRadius: 16, backgroundColor: enabled ? '#c5ead5' : '#ffffff'}}>
        <Text>{enabled ? '测试开关：开启' : '测试开关：关闭'}</Text>
      </Pressable>
    </View>
  );
}

AppRegistry.registerComponent('HomePanelPluginHost', () => HostExperiment);
