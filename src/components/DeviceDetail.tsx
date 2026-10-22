import type { Controller } from '../useHomePanel';
import type { Device } from '../types';
import { Icon } from './Icon';
import { DevicePicture } from './DeviceCard';
import { PropertyControl } from './PropertyControl';
import { powerProperty } from '../lib/queue';

export function DeviceDetail({ device, controller: c }: { device?: Device; controller: Controller }) {
  if (!device) return <section className="detail-panel glass"><div className="empty-state"><span className="spinner" />加载设备…</div></section>;
  const power = powerProperty(device);
  const pending = Array.from(c.busy).some(k => k.startsWith(`${device.id}:`));
  const favorite = c.data.preferences.favorites.includes(device.id);
  const controls = device.properties.filter(p => p !== power && p.writable);
  const readonly = device.properties.filter(p => p !== power && !p.writable && p.readable);
  return <section className={`detail-panel glass ${device.online ? '' : 'offline-detail'}`} role="dialog" aria-modal={c.data.inlineDetail || undefined} aria-labelledby="detail-title">
    <header className="detail-header"><div><h2 id="detail-title">{device.name}</h2><span className="room-name">{device.room}</span></div><div className="header-actions"><button className={`icon-button favorite-button ${favorite ? 'favorite' : ''}`} aria-label={favorite ? '取消收藏' : '收藏设备'} onClick={() => { const favorites = favorite ? c.data.preferences.favorites.filter(id => id !== device.id) : [...c.data.preferences.favorites, device.id]; void c.save({ ...c.data.preferences, favorites }).catch(c.notify); }}><Icon name="star" size={17} /></button><button className="icon-button" aria-label="关闭设备详情" onClick={() => { void c.close(); }}><Icon name="close" size={20} /></button></div></header>
    <div className="detail-scroll">
      <div className="device-hero"><div className="detail-art-stage"><span className="detail-art-orbit" /><span className="detail-art-orbit orbit-small" /><DevicePicture device={device} size={64} /></div>{power ? <button className={`large-power ${power.value === true ? 'active' : ''}`} disabled={!device.online || power.value == null || !!power.error || pending} aria-label={`${power.value === true ? '关闭' : '打开'} ${device.name}`} onClick={() => { void c.write(device.id, power, power.value !== true).catch(() => {}); }}>{pending ? <span className="spinner" /> : <Icon name="power" size={20} />}<span>{!device.online ? '设备离线' : power.value == null || power.error ? '状态未知' : power.value === true ? '已开启' : '已关闭'}</span></button> : <span className="sensor-note">{!device.online ? '设备离线' : device.specStatus === 'unsupported' ? '暂未支持此型号' : device.kind === 'sensor' ? '环境状态' : '设备状态'}</span>}</div>
      {c.readErrors[device.id] ? <p className="inline-error">{c.readErrors[device.id]}<button onClick={() => { void c.read(device.id); }}>重试</button></p> : null}
      {!device.online ? <p className="muted-note">设备当前离线，显示最近获取的状态。</p> : null}
      {device.specStatus === 'unloaded' ? <div className="empty-state small"><span className="spinner" />正在加载设备功能</div> : null}
      {controls.length ? <div className="detail-section-title"><strong>设备调节</strong><span>CONTROL</span></div> : null}
      <div className="detail-controls">{controls.map(p => <PropertyControl key={`${p.siid}:${p.piid}`} property={p} disabled={!device.online} onWrite={value => c.write(device.id, p, value)} />)}</div>
      {readonly.length ? <><div className="detail-section-title readings-title"><strong>实时读数</strong><span>LIVE</span></div><div className="readings">{readonly.map(p => <PropertyControl key={`${p.siid}:${p.piid}`} property={p} disabled onWrite={async () => {}} />)}</div></> : null}
      {device.actions.length ? <div className="device-actions">{device.actions.map(a => <button key={`${a.siid}:${a.aiid}`} className="secondary-button" disabled={!device.online} onClick={() => { void c.action(device.id, a.siid, a.aiid); }}>{a.name}</button>)}</div> : null}
      {device.specStatus === 'unsupported' ? <p className="muted-note">此型号没有可用的 MIoT 功能定义。设备信息仍可查看，暂不提供控制。</p> : null}
    </div>
  </section>;
}
