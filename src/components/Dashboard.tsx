import { useEffect, useMemo, useState } from 'react';
import type { Controller } from '../useHomePanel';
import { demo } from '../bridge';
import { DeviceCard } from './DeviceCard';
import { Icon } from './Icon';
import { powerProperty } from '../lib/queue';

export function Dashboard({ controller: c }: { controller: Controller }) {
  const [homeId, setHomeId] = useState('');
  const [room, setRoom] = useState('全部');
  const homes = c.data.homes;
  const selectedHome = homes.find(h => h.id === homeId) ?? homes[0];
  const homeDevices = useMemo(() => c.data.devices.filter(d => !selectedHome || d.homeId === selectedHome.id), [c.data.devices, selectedHome]);
  const rooms = useMemo(() => ['全部', ...(c.data.preferences.favorites.length ? ['常用'] : []), ...new Set(homeDevices.map(d => d.room))], [homeDevices, c.data.preferences.favorites.length]);
  const selectedRoom = rooms.includes(room) ? room : '全部';
  const devices = useMemo(() => homeDevices.filter(d => selectedRoom === '全部' || selectedRoom === '常用' && c.data.preferences.favorites.includes(d.id) || d.room === selectedRoom), [homeDevices, selectedRoom, c.data.preferences.favorites]);
  const ids = devices.map(d => d.id).join('|');
  useEffect(() => {
    if (!c.data.visible) return;
    const timer = setInterval(() => { void Promise.all(devices.filter(d => d.online).slice(0, 12).map(d => c.read(d.id))); }, 15_000);
    return () => clearInterval(timer);
  }, [ids, c.data.visible, c.read]);
  const latest = Math.max(0, ...devices.map(d => d.updatedAt ?? 0));
  const onlineCount = homeDevices.filter(d => d.online).length;
  const runningCount = homeDevices.filter(d => { const power = powerProperty(d); return d.online && power?.value === true && !power.error; }).length;
  const offlineCount = homeDevices.length - onlineCount;
  const roomCount = new Set(homeDevices.map(d => d.room).filter(Boolean)).size;
  const roomSize = (name: string) => name === '全部' ? homeDevices.length : name === '常用' ? homeDevices.filter(d => c.data.preferences.favorites.includes(d.id)).length : homeDevices.filter(d => d.room === name).length;
  const overviewTitle = offlineCount ? `${offlineCount} 台设备暂时离线` : homeDevices.length ? '一切尽在掌控' : '连接你的家';
  const overviewSummary = homeDevices.length ? `${runningCount} 台设备正在运行 · ${onlineCount}/${homeDevices.length} 台在线` : '连接米家账号后，设备会汇集在这里';
  return <section className="dashboard">
    <header className="panel-header dashboard-header"><div className="dashboard-brand"><span className="brand-icon"><Icon name="home" size={19} /></span><div className="dashboard-brand-copy"><span className="brand-label">HOME PANEL</span><div className="home-select"><h1>{selectedHome?.name || '我的家'}</h1><Icon name="chevron" size={15} /><select aria-label="选择家庭" value={selectedHome?.id ?? ''} onChange={e => { setHomeId(e.target.value); setRoom('全部'); }}>{homes.length ? homes.map(h => <option key={h.id} value={h.id}>{h.name}</option>) : <option value="">我的家</option>}</select></div></div></div><div className="header-actions"><button className={`icon-button ${c.refreshing ? 'refreshing' : ''}`} aria-label="刷新设备" disabled={c.refreshing} onClick={() => { void c.refresh(); }}><Icon name="refresh" size={19} /></button><button className="icon-button" aria-label="设置" onClick={() => c.setSettings(true)}><Icon name="settings" size={20} /></button></div></header>
    <section className="overview-card" aria-label="家庭状态概览"><div className="overview-kicker"><span className="live-dot" /> HOME SYSTEM <span className="overview-kicker-separator">/</span> 实时概览</div><div className="overview-copy"><h2>{overviewTitle}</h2><p>{overviewSummary}</p></div><div className="overview-art" aria-hidden="true"><span className="overview-orbit orbit-one" /><span className="overview-orbit orbit-two" /><span className="overview-art-core"><Icon name="home" size={33} /></span><span className="overview-node"><Icon name="power" size={13} /></span></div><div className="overview-metrics"><span><strong>{onlineCount}</strong><small>在线设备</small></span><i /><span><strong>{roomCount}</strong><small>空间</small></span><span className={`overview-health ${offlineCount ? 'has-offline' : ''}`}><span />{offlineCount ? '需要留意' : '连接正常'}</span></div></section>
    <nav className="room-tabs" aria-label="房间筛选">{rooms.map(r => <button key={r} className={selectedRoom === r ? 'active' : ''} aria-current={selectedRoom === r ? 'page' : undefined} onClick={() => setRoom(r)}><span>{r}</span><span className="room-count">{roomSize(r)}</span></button>)}</nav>
    <div className="devices-scroll"><div className="device-grid">{devices.map(d => <DeviceCard key={d.id} device={d} selected={c.data.selected === d.id} pending={Array.from(c.busy).some(k => k.startsWith(`${d.id}:`))} readError={c.readErrors[d.id]} onOpen={c.open} onPower={c.write} onVisible={c.read} />)}</div>{!devices.length ? <div className="empty-state"><Icon name="home" size={38} /><h2>{c.refreshing ? '正在查找设备' : selectedRoom === '常用' ? '还没有收藏设备' : '这里还没有设备'}</h2><p>{c.refreshing ? '正在读取当前米家账号的家庭与设备' : '请确认服务地区正确，设备已在米家 App 中添加。'}</p>{!c.refreshing ? <button className="secondary-button" onClick={() => c.setSettings(true)}>检查服务地区</button> : null}</div> : null}</div>
    <footer className={`panel-footer ${demo ? 'demo-footer' : ''}`}><Icon name={c.refreshing ? 'refresh' : demo ? 'warning' : 'refresh'} size={15} /><span>{demo ? '演示模式 · 未连接真实设备' : c.refreshing ? '正在同步设备' : latest ? `最近更新 ${new Date(latest).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}` : '等待获取设备状态'}</span></footer>
  </section>;
}
