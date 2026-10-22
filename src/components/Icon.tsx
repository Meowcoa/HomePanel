import type { CSSProperties } from 'react';
// Original, code-native line icons. No vendor/device artwork is bundled.
const paths: Record<string, React.ReactNode> = {
  light: <><path d="M9 18h6m-5 3h4M8.4 14.9A7 7 0 1 1 15.6 15c-1 .7-1.1 1.5-1.1 3h-5c0-1.5-.1-2.3-1.1-3.1Z" /></>,
  lamp: <><path d="M7 3h10l2 9H5l2-9Zm5 9v6M5 18h14v4M5 18v4M6 21h12" /></>,
  plug: <><rect x="4" y="3" width="16" height="15" rx="3" /><path d="M9 8v4m6-4v4M8 18v3m8-3v3" /></>,
  purifier: <><rect x="3" y="3" width="11" height="19" rx="3" /><path d="M7 7h3m-3 3h3M18 6c2-2 3 2 5 0m-5 5c2-2 3 2 5 0m-5 5c2-2 3 2 5 0" /></>,
  fan: <><circle cx="12" cy="12" r="1.8" /><path d="M10.5 10.6C1 6 8-3 12 4c1 2 0 4-1.5 6.6Zm3-.1C18-1 27 8 19 12c-2 .7-4-.3-5.5-1.5Zm.4 3C25 19 17 27 12 20c-1-2-.5-4.5 1.9-6.5Zm-3.2.2C5 25-3 17 4 12c2-1 4.5-.5 6.7 1.7Z" /></>,
  sensor: <><path d="M7 14.3V5a2 2 0 0 1 4 0v9.3a4 4 0 1 1-4 0Z" /><path d="M9 8v10m8-3 3-5 3 5a3.5 3.5 0 1 1-6 0Z" /></>,
  climate: <><rect x="2" y="4" width="20" height="10" rx="2" /><path d="M5 11h14M7 17v4m5-4v2m5-2v4" /></>,
  humidifier: <><path d="M5 14h14l1 7H4l1-7Zm5-3c-4-3 4-4 0-7m5 7c-4-3 4-4 0-7" /></>,
  vacuum: <><circle cx="12" cy="13" r="9" /><path d="M6 7h12M7 18h10" /><circle cx="12" cy="5" r="2" /></>,
  camera: <><rect x="3" y="4" width="18" height="14" rx="6" /><circle cx="12" cy="11" r="3" /><path d="M8 22h8m-4-4v4" /></>,
  speaker: <><rect x="5" y="2" width="14" height="20" rx="5" /><circle cx="12" cy="14" r="4" /><path d="M10 6h4" /></>,
  device: <><rect x="5" y="2" width="14" height="20" rx="4" /><path d="M10 6h4m-3 12h2" /></>,
  power: <><path d="M12 2v9M6.1 5a9 9 0 1 0 11.8 0" /></>,
  refresh: <><path d="M21 9a9 9 0 0 0-15-6L3 6m0-5v5h5M3 15a9 9 0 0 0 15 6l3-3m0 5v-5h-5" /></>,
  settings: <><path d="m9 3 1-2h4l1 2 2 1 2-.2 2 3-1 2v3l1 2-2 3-2-.2-2 1-1 2h-4l-1-2-2-1-2 .2-2-3 1-2V9L2 7l2-3 2 .2 3-1.2Z" transform="translate(0 1)" /><circle cx="12" cy="11" r="3" /></>,
  chevron: <path d="m6 9 6 6 6-6" />,
  close: <path d="m5 5 14 14M19 5 5 19" />,
  back: <path d="m14 5-7 7 7 7" />,
  home: <><path d="m2 11 10-9 10 9M5 9v13h14V9m-10 13v-8h6v8" /></>,
  star: <path d="m12 2 3.1 6.3L22 9.4l-5 4.9 1.2 6.9-6.2-3.3-6.2 3.3L7 14.3 2 9.4l6.9-1.1L12 2Z" />,
  check: <path d="m4 12 5 5L20 6" />,
  warning: <><path d="m12 3 10 18H2L12 3Z" /><path d="M12 9v5m0 3v.2" /></>,
  qr: <><path d="M3 9V3h6M15 3h6v6M21 15v6h-6M9 21H3v-6" /><path d="M6 6h1v1H6Zm11 0h1v1h-1ZM6 17h1v1H6Zm8-5h4v3h-4Zm-4-1v4m2 2h2v3m4-2h2" /></>,
};
export function Icon({ name, size = 24, className = '', style }: { name: string; size?: number; className?: string; style?: CSSProperties }) {
  return <svg className={`icon ${className}`} width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={style}>{paths[name] ?? paths.device}</svg>;
}
