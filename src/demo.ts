// Development-only presentation data. bridge.ts excludes this dynamic import
// from production builds. This module never contacts Xiaomi or real devices.
import type { Device, Property, Bootstrap } from './types';
import { defaults } from './types';
const prop = (key: string, name: string, value: Property['value'], overrides: Partial<Property> = {}): Property => ({ siid: 2, piid: 1, key, name, service: '日常控制', format: typeof value === 'boolean' ? 'bool' : 'uint16', unit: '', readable: true, writable: true, range: null, options: [], value, error: null, ...overrides });
const device = (id: string, name: string, room: string, kind: string, properties: Property[]): Device => ({ id, name, room, kind, model: `demo.${kind}.example`, homeId: 'demo-home', online: true, properties, actions: [], specStatus: 'ready', updatedAt: Date.now(), image: null });
const devices = [
  device('demo-light', '客厅灯', '客厅', 'light', [prop('on', '电源', true), prop('brightness', '亮度', 60, { piid: 2, unit: 'percentage', range: [1, 100, 1] }), prop('color-temperature', '色温', 4000, { piid: 3, unit: 'kelvin', range: [2700, 6500, 100] })]),
  device('demo-bedroom', '卧室灯', '卧室', 'lamp', [prop('on', '电源', false), prop('brightness', '亮度', 40, { piid: 2, unit: 'percentage', range: [1, 100, 1] })]),
  device('demo-plug', '智能插座', '客厅', 'plug', [prop('on', '电源', true)]),
  device('demo-purifier', '空气净化器', '书房', 'purifier', [prop('on', '电源', true), prop('mode', '模式', 0, { piid: 2, options: [{ value: 0, name: '自动模式' }, { value: 1, name: '睡眠模式' }, { value: 2, name: '手动模式' }] })]),
  device('demo-fan', '风扇', '客厅', 'fan', [prop('on', '电源', true), prop('fan-level', '风速', 2, { piid: 2, range: [1, 4, 1] })]),
  device('demo-sensor', '温湿度计', '卧室', 'sensor', [prop('temperature', '温度', 24, { writable: false, format: 'float', unit: 'celsius' }), prop('relative-humidity', '湿度', 55, { piid: 2, writable: false, unit: 'percentage' })]),
];
const params = new URLSearchParams(location.search);
if (params.get('offline') === '1') devices[2].online = false;
const state: Bootstrap = { authenticated: params.get('screen') !== 'login', preferences: { ...defaults }, homes: [{ id: 'demo-home', name: '我的家', owner: '', rooms: [] }], devices, login: { stage: '', qr: null, expiresAt: null, message: '' }, selected: null, inlineDetail: true, appearance: { glassAvailable: true, reduceMotion: false }, version: '0.1.1', visible: true };
export async function demoCommand(name: string, args: Record<string, unknown>) {
  if (name === 'bootstrap' || name === 'refresh_devices') return structuredClone(state);
  const d = state.devices.find(d => d.id === args.id);
  if (name === 'read_device') return structuredClone(d);
  if (name === 'load_device_image') { if (params.get('images') === 'broken') throw new Error('演示：图片加载失败'); return null; }
  if (name === 'write_property') {
    await new Promise(r => setTimeout(r, 280));
    if (params.get('fail') === 'write') throw new Error('演示：设备离线，操作未执行');
    if (!d?.online) throw new Error('演示：设备离线');
    const p = d.properties.find(p => p.siid === args.siid && p.piid === args.piid);
    if (!p?.writable) throw new Error('演示：属性只读');
    p.value = args.value as Property['value']; d.updatedAt = Date.now(); return structuredClone(d);
  }
  if (name === 'save_preferences') { state.preferences = args.preferences as Bootstrap['preferences']; return structuredClone(state.preferences); }
  if (name === 'logout') { state.authenticated = false; return; }
  if (name === 'start_login') throw new Error('演示模式无法登录真实账号，请打开桌面程序');
  if (name === 'open_device') return true;
}
