function parseRuntimeManifest(source) {
  if (typeof source !== 'string' || source.length > 16 * 1024) throw new Error('INVALID_RUNTIME_MANIFEST');
  const value = JSON.parse(source);
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('INVALID_RUNTIME_MANIFEST');
  if (Object.keys(value).length === 0) return {kind: 'legacy-plain', package: 'miot.plugin.spec', version: '1.1.10', sdkApiLevel: 10110, entryModule: '1006883-10001'};
  if (value.schema !== 1 || value.kind !== 'downloaded-rn' ||
      !/^[a-zA-Z0-9._-]{1,128}$/.test(value.package || '') || !/^[a-zA-Z0-9._-]{1,64}$/.test(value.version || '') ||
      !Number.isSafeInteger(value.sdkApiLevel) || value.sdkApiLevel < 10000 || value.sdkApiLevel > 99999 ||
      !Number.isSafeInteger(value.entryModule) || value.entryModule < 0 || value.entryModule > 250000) throw new Error('INVALID_RUNTIME_MANIFEST');
  return value;
}
module.exports = {parseRuntimeManifest};
