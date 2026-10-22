import { describe, it, expect } from 'vitest';
import { LatestWriteQueue, powerProperty } from './queue';

describe('device write ordering', () => {
  it('serializes requests and sends only the latest pending slider position', async () => {
    const values: number[] = []; const releases: (() => void)[] = []; const busy: boolean[] = [];
    const q = new LatestWriteQueue<number>(v => { values.push(v); return new Promise<void>(r => releases.push(r)); }, b => busy.push(b));
    const first = q.enqueue(10); const intermediate = q.enqueue(20); const last = q.enqueue(60);
    expect(values).toEqual([10]); releases.shift()!(); await first; await Promise.resolve();
    expect(values).toEqual([10, 60]); releases.shift()!(); await Promise.all([intermediate, last]);
    expect(busy).toEqual([true, false]);
  });
  it('does not retry a failed physical-device write', async () => {
    let calls = 0; const q = new LatestWriteQueue(async () => { calls++; throw new Error('offline'); });
    await expect(q.enqueue(true)).rejects.toThrow('offline'); expect(calls).toBe(1);
  });
  it('does not invent a power button for a read-only sensor', () => {
    expect(powerProperty({ properties: [] })).toBeUndefined();
  });
});
