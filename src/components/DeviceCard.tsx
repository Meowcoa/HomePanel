import { memo, useEffect, useRef, useState } from 'react';
import type { Device, Property } from '../types';
import { listen } from '@tauri-apps/api/event';
import { api, native } from '../bridge';
import { powerProperty, propertyValue } from '../lib/queue';
import { Icon } from './Icon';

export function DevicePicture({ device, size = 44 }: { device: Device; size?: number }) {
  const [source, setSource] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    const reload = () => { setSource(null); void api.image(device.id).then(uri => { if (active) setSource(uri); }).catch(() => {}); };
    reload();
    const subscription = native ? listen('preferences-changed', reload) : null;
    return () => { active = false; if (subscription) void subscription.then(unlisten => unlisten()); };
  }, [device.id]);
  return source ? <img className="device-picture" src={source} width={size} height={size} alt="" onError={() => setSource(null)} /> : <Icon name={device.kind} size={size} className="device-picture" />;
}
function summary(device: Device) {
  if (!device.online) return '设备离线';
  if (device.specStatus === 'unloaded') return '正在加载功能…';
  if (device.specStatus === 'unsupported') return '暂未支持此型号';
  const power = powerProperty(device);
  if (power?.value === false && !power.error) return '已关闭';
  const readable = device.properties.filter(p => !p.error && p.value !== null && p.value !== undefined);
  if (device.kind === 'sensor') return readable.filter(p => ['temperature', 'relative-humidity'].includes(p.key)).map(propertyValue).join(' · ') || '暂未获取状态';
  const primary = readable.find(p => ['brightness', 'mode', 'fan-level', 'speed-level'].includes(p.key));
  if (primary) return `${primary.key === 'brightness' ? '亮度 ' : ''}${propertyValue(primary)}${primary.key === 'fan-level' && !primary.unit ? ' 档' : ''}`;
  return power?.value === true && !power.error ? '已开启' : '暂未获取状态';
}
export const DeviceCard = memo(function DeviceCard({ device, selected, pending, readError, onOpen, onPower, onVisible }: { device: Device; selected: boolean; pending: boolean; readError?: string; onOpen: (id: string) => void; onPower: (id: string, p: Property, value: boolean) => Promise<void>; onVisible: (id: string) => Promise<void> }) {
  const ref = useRef<HTMLElement>(null);
  const power = powerProperty(device);
  const enabled = power?.value === true && !power.error;
  useEffect(() => {
    if (!device.online || device.specStatus !== 'unloaded' || !ref.current) return;
    const observer = new IntersectionObserver(entries => { if (entries.some(e => e.isIntersecting)) { observer.disconnect(); void onVisible(device.id); } });
    observer.observe(ref.current); return () => observer.disconnect();
  }, [device.id, device.online, device.specStatus, onVisible]);
  const compact = device.kind === 'fan' || device.kind === 'sensor';
  return <article ref={ref} className={`device-card ${selected ? 'selected' : ''} ${enabled ? 'is-on' : ''} ${compact ? 'compact' : ''} ${!device.online ? 'offline' : ''}`} data-device={device.id} data-kind={device.kind}>
    <button className="card-body" onClick={() => onOpen(device.id)} aria-label={`查看 ${device.name}`}>
      <span className="card-room"><span className="card-room-dot" />{device.room || '未分配空间'}</span>
      <span className="device-art" aria-hidden="true"><span className="device-art-ring" /><DevicePicture device={device} size={52} /></span>
      <span className="card-copy"><strong>{device.name}</strong><span className={`device-status ${readError ? 'status-warning' : ''}`} title={readError}>{readError ? '状态读取失败' : summary(device)}</span></span>
    </button>
    {power ? <button className={`power-button ${enabled ? 'active' : ''}`} disabled={!device.online || power.value == null || !!power.error || pending} aria-label={`${enabled ? '关闭' : '打开'} ${device.name}`} aria-pressed={enabled} onClick={() => { void onPower(device.id, power, !enabled).catch(() => {}); }}>{pending ? <span className="spinner" /> : <Icon name="power" size={20} />}</button> : null}
  </article>;
});
