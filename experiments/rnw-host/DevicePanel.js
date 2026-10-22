// Original controls using the verified, scoped broker; no phone SDK or artwork.
import React, {useEffect, useState} from 'react';
import {NativeModules, Switch, Text, View} from 'react-native';
const {createDeviceBridge} = require('./deviceBridge');
const bridge = createDeviceBridge(NativeModules.HomePanelBroker);

export default function DevicePanel() {
  const [device, setDevice] = useState(null);
  const [values, setValues] = useState([]);
  const [error, setError] = useState('');
  const [pending, setPending] = useState(false);
  useEffect(() => {
    let active = true, subscription;
    bridge.getDevice().then(info => {
      if (!active) return;
      setDevice(info);
      const targets = info.properties.filter(p => p.readable).slice(0, 12).map(({siid, piid}) => ({siid, piid}));
      if (!targets.length) return;
      subscription = bridge.subscribe(targets, event => {
        if (!active) return;
        if (event.error) {setError(event.error === 'DEVICE_OFFLINE' ? '设备已离线' : '状态读取失败，稍后重试'); setValues([]);}
        else {setValues(event.values); setError('');}
      });
    }).catch(() => {if (active) setError('设备服务不可用');});
    return () => {active = false; subscription?.remove();};
  }, []);
  async function write(property, value) {
    if (pending) return;
    setPending(true); setError('');
    try {
      const confirmed = await bridge.setValue({siid: property.siid, piid: property.piid}, value);
      setValues(current => [...current.filter(p => p.siid !== property.siid || p.piid !== property.piid), confirmed]);
    } catch (_) {setError('未确认修改结果，请刷新设备状态');}
    finally {setPending(false);}
  }
  return <View style={{padding: 16, backgroundColor: '#ffffff', borderRadius: 20, marginBottom: 16}}>
    <Text style={{fontSize: 18, color: '#243b31'}}>{device?.name || '正在读取设备…'}</Text>
    <Text style={{fontSize: 12, marginVertical: 8, color: '#52665d'}}>
      {device?.testData ? '测试数据 · 未连接真实设备' : '单设备开发预览'} · 每约 3 秒轮询
    </Text>
    {device?.properties.slice(0, 12).map(property => {
      const state = values.find(p => p.siid === property.siid && p.piid === property.piid);
      return <View key={`${property.siid}.${property.piid}`} style={{flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginVertical: 8}}>
        <Text style={{color: '#243b31'}}>{property.name}</Text>
        {property.format === 'bool' && property.writable && state?.available
          ? <Switch value={state.value === true} disabled={pending || !device.writesAllowed} onValueChange={value => write(property, value)} />
          : <Text>{state?.available ? `${state.value} ${property.unit || ''}` : '状态待确认'}</Text>}
      </View>;
    })}
    {!!error && <Text style={{color: '#a34340'}}>{error}</Text>}
  </View>;
}
