// Independent Metro factory registry for locally supplied phone plugins.
// The current Windows Metro registry and global __r/__d are never replaced.
const {parseRamBundle} = require('./ramBundle');
function createLegacyMetro({globalObject = globalThis, override = () => undefined, fallback, react = globalObject.React} = {}) {
  const modules = new Map(), cache = new Map(), starts = [], stack = [];
  const legacyGlobal = Object.create(globalObject);
  // Preserve old bare IntlPolyfill references without changing the host's Intl/global registry.
  const phoneIntl = typeof globalObject.Intl?.DateTimeFormat === 'function' ? Object.assign(Object.create(globalObject.Intl), {
    __addLocaleData() { /* Native Windows Intl already owns locale data. */ },
  }) : undefined;
  legacyGlobal.IntlPolyfill = phoneIntl;
  let loaded = 0;
  function define(factory, id, dependencies, name) {
    if (typeof factory !== 'function' || !Array.isArray(dependencies)) throw new Error('INVALID_METRO_DEFINITION');
    if (modules.has(id)) throw new Error('DUPLICATE_METRO_MODULE');
    modules.set(id, {factory, dependencies, name});
  }
  function requireModule(id) {
    if (cache.has(id)) return cache.get(id).exports;
    let record = modules.get(id);
    if (!record) {
      if (fallback) return fallback(id);
      throw new Error(`MISSING_METRO_MODULE:${id}`);
    }
    if (record.source) {
      let captured;
      // Signed local package execution, isolated from host/SDK module IDs, not a security sandbox.
      Function('global', '__d', '__r', '__DEV__', 'IntlPolyfill', 'React', record.source())(legacyGlobal,
        (factory, moduleId, dependencies, name) => {
          if (captured || moduleId !== id || typeof factory !== 'function' || !Array.isArray(dependencies)) throw new Error('INVALID_RAM_DEFINITION');
          captured = {factory, dependencies, name};
        }, () => {throw new Error('RAM_RECORD_STARTUP_DENIED');}, false, phoneIntl, react);
      if (!captured) throw new Error('MISSING_RAM_DEFINITION');
      modules.set(id, record = captured);
    }
    const replacement = override({id, name: record.name, require: requireModule});
    if (replacement !== undefined) {cache.set(id, {exports: replacement}); return replacement;}
    const module = {exports: {}};
    cache.set(id, module); stack.push(id);
    try {
      record.factory(legacyGlobal, requireModule, importDefault, importAll, module, module.exports, record.dependencies);
      loaded++; return module.exports;
    } catch (error) {
      cache.delete(id);
      if (!error.legacyModuleChain) error.legacyModuleChain = stack.slice();
      throw error;
    } finally {stack.pop();}
  }
  function importDefault(id) {
    const exports = requireModule(id);
    return exports && exports.__esModule ? exports.default : exports;
  }
  function importAll(id) {
    const exports = requireModule(id);
    if (exports && exports.__esModule) return exports;
    return {...exports, default: exports};
  }
  legacyGlobal.__r = requireModule;
  legacyGlobal.require = requireModule;
  requireModule.named = name => {
    for (const [id, record] of modules) if (record.name === name) return requireModule(id);
    throw new Error(`MISSING_METRO_NAMED_MODULE:${name}`);
  };
  return {
    register(source) {
      if (typeof source !== 'string' || source.length > 60 * 1024 * 1024) throw new Error('INVALID_PLUGIN_SOURCE');
      const start = source.indexOf('__d(function');
      if (start < 0) throw new Error('PLAIN_METRO_FACTORIES_REQUIRED');
      // Deliberate execution of the user-selected local source; this is not a sandbox.
      // Registration captures startup requests so adapters can be installed first.
      Function('global', '__d', '__r', '__DEV__', 'IntlPolyfill', 'React', source.slice(start))(
        legacyGlobal, define, id => starts.push(id), false, phoneIntl, react,
      );
    },
    registerRam(input) {
      const bank = parseRamBundle(input);
      for (const id of bank.ids) if (modules.has(id)) throw new Error('DUPLICATE_METRO_MODULE');
      // Indexed bundles use only numeric require calls plus source-map comments at startup.
      const startup = bank.startup.replace(/\/\/[^\n\r]*/g, '').trim();
      if (!/^(?:\s*__r\(\s*\d+\s*\)\s*;?\s*)+$/.test(startup)) throw new Error('UNSUPPORTED_RAM_STARTUP');
      for (const id of bank.ids) modules.set(id, {source: () => bank.source(id)});
      for (const match of startup.matchAll(/__r\(\s*(\d+)\s*\)/g)) starts.push(Number(match[1]));
    },
    require: requireModule,
    stats: () => ({definitions: modules.size, evaluated: loaded, startupRequests: starts.slice()}),
    describe: id => modules.get(id)?.name || String(id),
  };
}
module.exports = {createLegacyMetro};
