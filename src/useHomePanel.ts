import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, demo, native, message } from './bridge';
import { LatestWriteQueue } from './lib/queue';
import { defaults, type Bootstrap, type Device, type Property, type JsonValue, type Preferences, type Appearance, type LoginStatus } from './types';

const initial: Bootstrap = { authenticated: false, preferences: defaults, homes: [], devices: [], login: { stage: '', qr: null, expiresAt: null, message: '' }, selected: null, inlineDetail: false, appearance: { glassAvailable: false, reduceMotion: false }, version: '0.1.1', visible: true };
export function useHomePanel(detailWindow: boolean) {
  const [data, setData] = useState<Bootstrap>(initial);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [settings, setSettings] = useState(false);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState<Set<string>>(new Set());
  const [readErrors, setReadErrors] = useState<Record<string, string>>({});
  const pendingReads = useRef(new Map<string, Promise<void>>());
  const writers = useRef(new Map<string, LatestWriteQueue<JsonValue>>());
  const alive = useRef(true);
  const current = useRef(data); current.current = data;
  const notify = useCallback((e: unknown) => {
    setError(message(e));
    if (String(e).includes('AUTH_REQUIRED')) setData(d => ({ ...d, authenticated: false, selected: null }));
  }, []);
  const patch = useCallback((device: Device) => { if (alive.current) setData(d => ({ ...d, devices: d.devices.map(x => x.id === device.id ? device : x) })); }, []);
  const refresh = useCallback(async () => {
    setRefreshing(true);
    try { const next = await api.refresh(); if (alive.current) { setData(next); setReadErrors({}); setError(''); } }
    catch (e) { notify(e); } finally { if (alive.current) setRefreshing(false); }
  }, [notify]);
  const read = useCallback((id: string): Promise<void> => {
    const existing = pendingReads.current.get(id); if (existing) return existing;
    const promise = api.read(id).then(d => { patch(d); setReadErrors(e => { const next = { ...e }; delete next[id]; return next; }); }).catch(e => { if (alive.current) setReadErrors(errors => ({ ...errors, [id]: message(e) })); if (String(e).includes('AUTH_REQUIRED')) notify(e); }).finally(() => pendingReads.current.delete(id));
    pendingReads.current.set(id, promise); return promise;
  }, [patch, notify]);
  const write = useCallback(async (id: string, p: Property, value: JsonValue) => {
    const key = `${id}:${p.siid}:${p.piid}`;
    let writer = writers.current.get(key);
    if (!writer) {
      writer = new LatestWriteQueue<JsonValue>(async v => { const device = await api.write(id, p.siid, p.piid, v); patch(device); }, pending => { if (alive.current) setBusy(previous => { const next = new Set(previous); if (pending) next.add(key); else next.delete(key); return next; }); });
      writers.current.set(key, writer);
    }
    try { await writer.enqueue(value); }
    catch (e) { notify(e); throw e; }
  }, [patch, notify]);
  const open = useCallback(async (id: string) => {
    try { const inline = await api.open(id); setData(d => ({ ...d, selected: id, inlineDetail: demo && !native ? innerWidth < 900 : inline })); void read(id); }
    catch (e) { notify(e); }
  }, [read, notify]);
  const close = useCallback(async () => { try { await api.close(); if (demo) setData(d => ({ ...d, selected: null, inlineDetail: false })); } catch (e) { notify(e); } }, [notify]);
  const save = useCallback(async (preferences: Preferences) => {
    const next = await api.save(preferences); setData(d => ({ ...d, preferences: next }));
    if (next.region !== current.current.preferences.region) await refresh();
  }, [refresh]);
  const startLogin = useCallback(async () => { try { const login = await api.login(); setData(d => ({ ...d, login })); setError(''); } catch (e) { notify(e); } }, [notify]);
  const logout = useCallback(async () => { await api.logout(); setSettings(false); writers.current.clear(); setData(d => ({ ...initial, preferences: d.preferences, appearance: d.appearance })); }, []);
  const action = useCallback(async (id: string, siid: number, aiid: number) => { try { patch(await api.action(id, siid, aiid)); } catch (e) { notify(e); } }, [patch, notify]);

  useEffect(() => {
    alive.current = true;
    void api.bootstrap().then(next => {
      if (!alive.current) return; setData({ ...next, visible: next.visible ?? true });
      if (next.authenticated && !detailWindow) void refresh();
      if (detailWindow && next.selected) void read(next.selected);
    }).catch(notify).finally(() => { if (alive.current) setLoading(false); });
    const unlisteners: (() => void)[] = [];
    function subscribe<T>(event: string, callback: (payload: T) => void) {
      if (!native) return;
      void listen<T>(event, e => { if (alive.current) callback(e.payload); }).then(unlisten => { if (alive.current) unlisteners.push(unlisten); else unlisten(); }).catch(notify);
    }
    subscribe<Device>('device-updated', patch);
    subscribe<Device[]>('devices-changed', devices => setData(d => ({ ...d, devices })));
    subscribe<Preferences>('preferences-changed', preferences => setData(d => ({ ...d, preferences })));
    subscribe<Appearance>('appearance-changed', appearance => setData(d => ({ ...d, appearance })));
    subscribe<{ id: string | null; inline: boolean }>('selection-changed', selection => { setData(d => ({ ...d, selected: selection.id, inlineDetail: selection.inline })); if (selection.id) void read(selection.id); });
    subscribe<boolean>('panel-visibility', visible => { setData(d => ({ ...d, visible })); if (visible) setError(''); });
    subscribe<LoginStatus>('auth-status', login => { setData(d => ({ ...d, login, authenticated: login.stage === 'success' ? true : d.authenticated })); if (login.stage === 'success' && !detailWindow) void refresh(); });
    subscribe('signed-out', () => { writers.current.clear(); setData(d => ({ ...initial, preferences: d.preferences, appearance: d.appearance })); });
    subscribe('open-settings', () => setSettings(true));
    return () => { alive.current = false; unlisteners.forEach(fn => fn()); };
  }, [detailWindow, notify, patch, read, refresh]);

  useEffect(() => {
    const media = matchMedia('(prefers-color-scheme: dark)');
    const apply = () => { document.documentElement.dataset.theme = data.preferences.theme === 'system' ? (media.matches ? 'dark' : 'light') : data.preferences.theme; };
    apply(); media.addEventListener('change', apply); return () => media.removeEventListener('change', apply);
  }, [data.preferences.theme]);
  useEffect(() => {
    if (!error || error.includes('登录')) return;
    const timer = setTimeout(() => setError(''), 6500); return () => clearTimeout(timer);
  }, [error]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      if (settings) setSettings(false);
      else if (demo && !native) { if (current.current.selected) void close(); }
      else void api.escape().catch(notify);
    };
    addEventListener('keydown', key); return () => removeEventListener('keydown', key);
  }, [settings, close, notify]);
  return { data, loading, refreshing, settings, setSettings, error, setError, busy, readErrors, refresh, read, write, open, close, save, startLogin, logout, action, notify };
}
export type Controller = ReturnType<typeof useHomePanel>;
