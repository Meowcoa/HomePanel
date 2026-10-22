// Original indexed Metro RAM parser. Parsing never executes bundle source.
const MAX_BYTES = 60 * 1024 * 1024;
const MAX_SLOTS = 250000;
function invalid(detail) {throw new Error(`INVALID_RAM_BUNDLE:${detail}`);}
function decodeBase64(text) {
  if (typeof text !== 'string' || text.length > Math.ceil(MAX_BYTES / 3) * 4 || text.length % 4) invalid('BASE64');
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  const padding = text.endsWith('==') ? 2 : text.endsWith('=') ? 1 : 0;
  const bytes = new Uint8Array(text.length / 4 * 3 - padding);
  if (bytes.length > MAX_BYTES) invalid('SIZE');
  for (let i = 0, n = 0; i < text.length; i += 4) {
    const a = alphabet.indexOf(text[i]), b = alphabet.indexOf(text[i + 1]);
    const c = text[i + 2] === '=' ? 0 : alphabet.indexOf(text[i + 2]);
    const d = text[i + 3] === '=' ? 0 : alphabet.indexOf(text[i + 3]);
    if (a < 0 || b < 0 || c < 0 || d < 0 ||
        (text[i + 2] === '=' && (i + 4 !== text.length || text[i + 3] !== '=')) ||
        (text[i + 3] === '=' && i + 4 !== text.length)) invalid('BASE64');
    if ((text[i + 2] === '=' && (b & 15)) || (text[i + 3] === '=' && (c & 3))) invalid('BASE64_PADDING');
    const word = a * 262144 + b * 4096 + c * 64 + d;
    bytes[n++] = word >>> 16;
    if (n < bytes.length) bytes[n++] = word >>> 8;
    if (n < bytes.length) bytes[n++] = word;
  }
  return bytes;
}
// Hermes does not require a global TextDecoder. Reject malformed UTF-8 ourselves.
function decodeUtf8(bytes) {
  const parts = [], units = [];
  for (let i = 0; i < bytes.length;) {
    const a = bytes[i++];
    let value, count, minimum;
    if (a < 128) {value = a; count = 0; minimum = 0;}
    else if (a >= 194 && a <= 223) {value = a & 31; count = 1; minimum = 128;}
    else if (a >= 224 && a <= 239) {value = a & 15; count = 2; minimum = 2048;}
    else if (a >= 240 && a <= 244) {value = a & 7; count = 3; minimum = 65536;}
    else invalid('UTF8');
    if (i + count > bytes.length) invalid('UTF8_TRUNCATED');
    for (let j = 0; j < count; j++) {
      const b = bytes[i++];
      if ((b & 192) !== 128) invalid('UTF8_CONTINUATION');
      value = value * 64 + (b & 63);
    }
    if (value < minimum || value > 0x10ffff || (value >= 0xd800 && value <= 0xdfff)) invalid('UTF8_RANGE');
    if (value > 65535) {value -= 65536; units.push(0xd800 + (value >>> 10), 0xdc00 + (value & 1023));}
    else units.push(value);
    if (units.length >= 4096) {parts.push(String.fromCharCode(...units)); units.length = 0;}
  }
  if (units.length) parts.push(String.fromCharCode(...units));
  return parts.join('');
}
function parseRamBundle(input) {
  const bytes = input instanceof Uint8Array ? input : input instanceof ArrayBuffer ? new Uint8Array(input) : null;
  if (!bytes || bytes.byteLength < 12 || bytes.byteLength > MAX_BYTES) invalid('SIZE');
  const data = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (data.getUint32(0, true) !== 0xfb0bd1e5) invalid('MAGIC');
  const slots = data.getUint32(4, true), startupLength = data.getUint32(8, true);
  if (!slots || slots > MAX_SLOTS) invalid('SLOTS');
  const base = 12 + slots * 8;
  if (base > bytes.length || !startupLength || startupLength > bytes.length - base) invalid('STARTUP_BOUNDS');
  const entries = new Map(), intervals = [];
  for (let id = 0; id < slots; id++) {
    const offset = data.getUint32(12 + id * 8, true), length = data.getUint32(16 + id * 8, true);
    if (!length) {if (offset) invalid('EMPTY_RECORD'); continue;}
    if (offset < startupLength || offset > bytes.length - base || length > bytes.length - base - offset) invalid('MODULE_BOUNDS');
    entries.set(id, {offset: base + offset, length});
    intervals.push([offset, offset + length]);
  }
  intervals.sort((a, b) => a[0] - b[0]);
  for (let i = 1; i < intervals.length; i++) if (intervals[i][0] < intervals[i - 1][1]) invalid('MODULE_OVERLAP');
  function source(offset, length) {
    if (bytes[offset + length - 1] !== 0) invalid('MISSING_TERMINATOR');
    const value = decodeUtf8(bytes.subarray(offset, offset + length - 1));
    if (value.includes('\0')) invalid('EMBEDDED_NUL');
    return value;
  }
  // Validate all source encodings before executing any factory, but keep decoding lazy thereafter.
  const startup = source(base, startupLength);
  for (const row of entries.values()) source(row.offset, row.length);
  return {startup, ids: [...entries.keys()], slots, has: id => entries.has(id),
    source(id) {const row = entries.get(id); if (!row) invalid('MISSING_MODULE'); return source(row.offset, row.length);}};
}
module.exports = {parseRamBundle, decodeBase64, decodeUtf8};
