import { invoke } from '@tauri-apps/api/core';
import type { Bootstrap, Device, Preferences, LoginStatus, JsonValue } from './types';

export const native = '__TAURI_INTERNALS__' in window;
export const demo = import.meta.env.DEV && new URLSearchParams(location.search).get('demo') === '1';
type Demo = typeof import('./demo');
let modulePromise: Promise<Demo> | undefined;
async function call<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
  if (import.meta.env.DEV && demo) {
    modulePromise ??= import('./demo');
    return (await modulePromise).demoCommand(name, args) as Promise<T>;
  }
  if (!native) throw new Error('请在 HomePanel 桌面程序中使用真实设备连接。');
  return invoke<T>(name, args);
}
export const api = {
  bootstrap: () => call<Bootstrap>('bootstrap'),
  refresh: () => call<Bootstrap>('refresh_devices'),
  read: (id: string) => call<Device>('read_device', { id }),
  write: (id: string, siid: number, piid: number, value: JsonValue) => call<Device>('write_property', { id, siid, piid, value }),
  action: (id: string, siid: number, aiid: number) => call<Device>('run_action', { id, siid, aiid }),
  login: () => call<LoginStatus>('start_login'),
  cancelLogin: () => call<void>('cancel_login'),
  logout: () => call<void>('logout'),
  save: (preferences: Preferences) => call<Preferences>('save_preferences', { preferences }),
  open: (id: string) => demo && native ? invoke<boolean>('open_device', { id }) : call<boolean>('open_device', { id }),
  close: () => demo && native ? invoke<void>('close_device') : call<void>('close_device'),
  hide: () => demo && native ? invoke<void>('hide_panel') : call<void>('hide_panel'),
  escape: () => demo && native ? invoke<void>('escape_panel') : call<void>('escape_panel'),
  image: (id: string) => call<string | null>('load_device_image', { id }),
};
export function message(error: unknown) { return (error instanceof Error ? error.message : String(error)).replace(/^(AUTH_REQUIRED|UNCERTAIN):\s*/, ''); }
