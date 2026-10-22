import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { demo, api, native } from './bridge';
import { useHomePanel } from './useHomePanel';
import { Dashboard } from './components/Dashboard';
import { DeviceDetail } from './components/DeviceDetail';
import { Settings } from './components/Settings';
import { Login } from './components/Login';
import { Icon } from './components/Icon';

export default function App() {
  const detailWindow = new URLSearchParams(location.search).get('window') === 'detail';
  const c = useHomePanel(detailWindow);
  const selected = c.data.devices.find(d => d.id === c.data.selected);
  const glass = c.data.preferences.glass && c.data.appearance.glassAvailable;
  const classes = `${demo && !native ? 'preview-desktop' : 'native-desktop'} ${!glass ? 'solid-surfaces' : ''} ${c.data.appearance.reduceMotion ? 'reduce-motion' : ''}`;
  useEffect(() => {
    if (demo && new URLSearchParams(location.search).get('stage') === 'detail' && c.data.devices.length && !c.data.selected) void c.open('demo-light');
  }, [c.data.devices.length]);
  useEffect(() => {
    if (c.data.appearance.reduceMotion || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const nodes = document.querySelectorAll('.main-panel, .native-desktop > .detail-panel');
    const animations = Array.from(nodes, node => node.animate(c.data.visible ? [{ opacity: 0, transform: 'translateY(8px) scale(.985)' }, { opacity: 1, transform: 'translateY(0) scale(1)' }] : [{ opacity: 1, transform: 'translateY(0)' }, { opacity: 0, transform: 'translateY(6px)' }], { duration: c.data.visible ? 190 : 175, easing: 'cubic-bezier(.16,1,.3,1)', fill: 'forwards' }));
    return () => animations.forEach(a => a.cancel());
  }, [c.data.visible, c.data.appearance.reduceMotion]);
  useEffect(() => {
    if (!native) return;
    let cancelled = false;
    const promise = listen('detail-closing', () => {
      if (!c.data.appearance.reduceMotion && !matchMedia('(prefers-reduced-motion: reduce)').matches) document.querySelector('.detail-panel')?.animate([{ opacity: 1, transform: 'translateX(0)' }, { opacity: 0, transform: 'translateX(6px)' }], { duration: 175, easing: 'ease-out' });
    });
    void promise.then(unlisten => { if (cancelled) unlisten(); });
    return () => { cancelled = true; void promise.then(unlisten => unlisten()); };
  }, [c.data.appearance.reduceMotion]);
  useEffect(() => {
    if (!c.data.selected || c.data.appearance.reduceMotion || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    document.querySelector('.detail-panel')?.animate([{ opacity: 0, transform: 'translateX(6px) scale(.985)' }, { opacity: 1, transform: 'translateX(0) scale(1)' }], { duration: 190, easing: 'cubic-bezier(.16,1,.3,1)' });
  }, [c.data.selected, c.data.appearance.reduceMotion]);
  return <main className={classes}>
    {detailWindow ? <DeviceDetail device={selected} controller={c} /> : <>
      <section className="main-panel glass" aria-label="HomePanel 设备面板">
        {c.loading ? <div className="startup"><span className="brand-icon"><Icon name="home" size={32} /></span><strong>HomePanel</strong><span className="spinner" /></div> : c.settings ? <Settings controller={c} /> : c.data.authenticated ? <Dashboard controller={c} /> : <Login controller={c} />}
        {selected && c.data.inlineDetail && !c.settings ? <div className="inline-detail"><DeviceDetail device={selected} controller={c} /></div> : null}
        {c.error ? <div className="error-toast" role="alert"><Icon name="warning" size={18} /><span>{c.error}</span><button className="icon-button" aria-label="关闭提示" onClick={() => c.setError('')}><Icon name="close" size={15} /></button></div> : null}
      </section>
      {demo && !native && selected && !c.data.inlineDetail && !c.settings ? <div className="preview-detail"><DeviceDetail device={selected} controller={c} /></div> : null}
      {demo && !native ? <div className="preview-taskbar" aria-hidden="true"><div className="preview-start"><span /><span /><span /><span /></div><span className="preview-taskbar-spacer" /><span>⌃</span><button tabIndex={-1} title="演示托盘入口" onClick={() => { void api.hide(); }}><Icon name="home" size={20} /></button><span className="preview-time">{new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span></div> : null}
    </>}
    {detailWindow && c.error ? <div className="error-toast" role="alert"><span>{c.error}</span><button className="icon-button" aria-label="关闭提示" onClick={() => c.setError('')}><Icon name="close" size={15} /></button></div> : null}
  </main>;
}
