import { useEffect, useRef, useState } from 'react';
import type { Property, JsonValue } from '../types';
import { propertyValue } from '../lib/queue';
import { Icon } from './Icon';

export function PropertyControl({ property: p, disabled, onWrite }: { property: Property; disabled: boolean; onWrite: (value: JsonValue) => Promise<void> }) {
  const [draft, setDraft] = useState(p.value ?? '');
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const dragging = useRef(false);
  const lastSent = useRef<JsonValue | undefined>(undefined);
  const latest = useRef(draft); latest.current = draft;
  useEffect(() => { if (!dragging.current) { setDraft(p.value ?? ''); latest.current = p.value ?? ''; lastSent.current = undefined; } }, [p.value]);
  useEffect(() => () => { clearTimeout(timer.current); }, []);
  const commit = () => {
    clearTimeout(timer.current);
    const value = latest.current;
    if (value === '' || value === lastSent.current || value === p.value) return;
    lastSent.current = value;
    void onWrite(value).catch(() => { setDraft(p.value ?? ''); latest.current = p.value ?? ''; lastSent.current = undefined; });
  };
  if (!p.writable) return <div className="read-only-property"><span>{p.name}</span><strong title={p.error ?? ''}>{propertyValue(p)}</strong></div>;
  if (p.format === 'bool') return <div className="boolean-property"><span>{p.name}</span><button className={`toggle ${p.value === true ? 'checked' : ''}`} role="switch" aria-checked={p.value === true} aria-label={p.name} disabled={disabled || p.value == null} onClick={() => { void onWrite(p.value !== true).catch(() => {}); }}><span /></button></div>;
  if (p.options.length) return <label className="select-property"><span>{p.name}</span><select aria-label={p.name} value={p.value == null ? '' : JSON.stringify(p.value)} disabled={disabled} onChange={e => { void onWrite(JSON.parse(e.target.value) as JsonValue).catch(() => {}); }}><option value="" disabled>请选择</option>{p.options.map(o => <option key={JSON.stringify(o.value)} value={JSON.stringify(o.value)}>{o.name}</option>)}</select></label>;
  if (p.range && p.range.length >= 3 && p.range[1] - p.range[0] <= 100_000) {
    const [min, max, step] = p.range;
    const percent = typeof draft === 'number' ? Math.max(0, Math.min(100, ((draft - min) / (max - min)) * 100)) : 0;
    const label = p.key === 'brightness' ? `${draft === '' ? '—' : draft}%` : p.key === 'color-temperature' ? `${draft === '' ? '—' : draft} K` : `${draft === '' ? '—' : draft}${p.unit === 'percentage' ? '%' : ''}`;
    return <label className={`slider-property ${p.key === 'color-temperature' ? 'temperature-slider' : ''}`}><span className="property-label"><span>{p.name}</span><output>{label}</output></span><span className="range-track" style={{ '--filled': `${percent}%` } as React.CSSProperties}><input type="range" aria-label={p.name} min={min} max={max} step={step || 1} value={typeof draft === 'number' ? draft : min} disabled={disabled} onPointerDown={() => { dragging.current = true; }} onChange={e => { const value = Number(e.target.value); latest.current = value; setDraft(value); clearTimeout(timer.current); timer.current = setTimeout(commit, 250); }} onPointerUp={() => { dragging.current = false; commit(); }} onPointerCancel={() => { dragging.current = false; commit(); }} onKeyUp={commit} onBlur={() => { dragging.current = false; commit(); }} /></span></label>;
  }
  return <form className="text-property" onSubmit={e => { e.preventDefault(); commit(); }}><label>{p.name}<input aria-label={p.name} type={p.format === 'string' ? 'text' : 'number'} value={String(draft)} disabled={disabled} onChange={e => { const value = p.format === 'string' ? e.target.value : Number(e.target.value); latest.current = value; setDraft(value); }} /></label><button className="icon-button" type="submit" aria-label={`设置${p.name}`} disabled={disabled}><Icon name="check" /></button></form>;
}
