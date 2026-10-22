export type JsonValue = boolean | number | string | null;
export interface PropertyOption { value: JsonValue; name: string }
export interface Property {
  siid: number; piid: number; key: string; name: string; service: string; format: string;
  unit: string; readable: boolean; writable: boolean; range: number[] | null;
  options: PropertyOption[]; value: JsonValue; error: string | null;
}
export interface DeviceAction { siid: number; aiid: number; key: string; name: string }
export interface Device {
  id: string; name: string; model: string; homeId: string; room: string; online: boolean;
  kind: string; properties: Property[]; actions: DeviceAction[]; specStatus: string;
  updatedAt: number | null; image: string | null;
}
export interface Home { id: string; name: string; owner: string; rooms: { id: string; name: string; dids: string[] }[] }
export interface ApprovedImage { model: string; url: string; permissionUrl: string }
export interface Preferences { region: string; theme: string; glass: boolean; autostart: boolean; favorites: string[]; approvedImages: ApprovedImage[] }
export interface Appearance { glassAvailable: boolean; reduceMotion: boolean }
export interface LoginStatus { stage: string; qr: string | null; expiresAt: number | null; message: string }
export interface Bootstrap {
  authenticated: boolean; preferences: Preferences; homes: Home[]; devices: Device[];
  login: LoginStatus; selected: string | null; inlineDetail: boolean; appearance: Appearance;
  version: string; visible: boolean;
}
export const defaults: Preferences = { region: 'cn', theme: 'system', glass: true, autostart: false, favorites: [], approvedImages: [] };
