const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createLocalAssets, resourceName} = require('./localAssets');
test('Metro drawable naming matches Android paths without loading network resources', () => {
  const asset = {httpServerLocation:'/assets/projects/miot.plugin.spec/assets/images',name:'plug_on',type:'png',width:80,height:80};
  const key = resourceName(asset);
  assert.equal(key, 'projects_miotpluginspec_assets_images_plug_on.png');
  const images=createLocalAssets({base:'C:/本机 目录/images',files:{[key]:{name:key,scale:2}}});
  const id=images.registerAsset(asset);
  assert.equal(images.resolve(id).uri,encodeURI('file:///C:/本机 目录/images/'+key));
  assert.equal(images.resolve(id).width,80);
  assert.equal(images.resolve(id).scale,2);
  assert.equal(images.resolve('https://example.com/a.png'),'https://example.com/a.png');
});
test('missing, unsafe or unsupported image definitions cannot escape the local image root', () => {
  const asset={httpServerLocation:'/assets/foo',name:'bar',type:'png'};
  const images=createLocalAssets({base:'C:/images',files:{foo_bar:{name:'../escape.png',scale:1}}});
  assert.equal(images.resolve(images.registerAsset(asset)).uri,'');
  assert.equal(resourceName({...asset,type:'svg'}),null);
});
