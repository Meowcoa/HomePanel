// Original Android Metro asset-name mapping. Only user-imported raster files are used.
function resourceName(asset) {
  if (!asset || !['png', 'jpg', 'jpeg', 'webp'].includes(asset.type)) return null;
  return `${asset.httpServerLocation}/${asset.name}`.replace(/^\/assets\//, '')
    .replace(/\//g, '_').replace(/[^a-zA-Z0-9_]/g, '').toLowerCase() + `.${asset.type}`;
}
function createLocalAssets({base = '', files = {}} = {}) {
  const registry = new Map(); let next = 1000000000;
  function resolve(source) {
    if (Array.isArray(source)) return source.map(resolve);
    if (!registry.has(source)) return source;
    const asset = registry.get(source), key = resourceName(asset), item = files[key];
    if (!item || !/^[a-z0-9_]+\.(png|jpg|jpeg|webp)$/.test(item.name) || !(item.scale > 0)) return {uri: '', width: asset.width, height: asset.height};
    return {uri: encodeURI(`file:///${base.replace(/\\/g, '/')}/${item.name}`), scale: item.scale,
      width: asset.width, height: asset.height};
  }
  return {registerAsset(asset) {const id = next++;registry.set(id, asset);return id;},
    getAssetByID: id => registry.get(id), resolve};
}
module.exports = {resourceName, createLocalAssets};
