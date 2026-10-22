/** Serializes writes and coalesces intermediate slider values. It never retries
 * a write: only a newer explicit user intent can produce another command. */
export class LatestWriteQueue<T> {
  private active = false;
  private next: { value: T; waiters: { resolve: () => void; reject: (e: unknown) => void }[] } | undefined;
  constructor(private perform: (value: T) => Promise<void>, private busy: (value: boolean) => void = () => {}) {}
  enqueue(value: T): Promise<void> {
    return new Promise((resolve, reject) => {
      if (this.next) { this.next.value = value; this.next.waiters.push({ resolve, reject }); }
      else this.next = { value, waiters: [{ resolve, reject }] };
      if (!this.active) void this.drain();
    });
  }
  private async drain() {
    this.active = true; this.busy(true);
    try {
      while (this.next) {
        const item = this.next; this.next = undefined;
        try { await this.perform(item.value); item.waiters.forEach(w => w.resolve()); }
        catch (error) { item.waiters.forEach(w => w.reject(error)); }
      }
    } finally { this.active = false; this.busy(false); }
  }
}

export function powerProperty(device: { properties: import('../types').Property[] }) {
  return device.properties.find(p => p.key === 'on' && p.format === 'bool' && p.writable);
}
export function propertyValue(p: import('../types').Property): string {
  if (p.error || p.value === null || p.value === undefined) return '暂未获取';
  const option = p.options.find(o => o.value === p.value);
  if (option) return option.name;
  if (typeof p.value === 'boolean') return p.value ? '已开启' : '已关闭';
  const unit: Record<string, string> = { percentage: '%', celsius: '°C', kelvin: ' K', 'μg/m3': ' μg/m³', ppm: ' ppm', seconds: ' 秒', minutes: ' 分钟', none: '' };
  return `${p.value}${unit[p.unit] ?? (p.unit ? ` ${p.unit}` : '')}`;
}
