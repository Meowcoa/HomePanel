const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createWindowsSvgComponents, normalizeBrush} = require('./windowsSvg');

test('legacy solid, referenced and current-color brushes retain their meaning', () => {
  assert.deepEqual(normalizeBrush([0, -16711936]), {type: 0, payload: -16711936});
  assert.deepEqual(normalizeBrush([1, 'gradient']), {type: 1, brushRef: 'gradient'});
  assert.deepEqual(normalizeBrush([2]), {type: 2});
  assert.equal(normalizeBrush(null), null);
  assert.throws(() => normalizeBrush([3]), /UNSUPPORTED_LEGACY_SVG_BRUSH/);
});

test('legacy components use generated Fabric components and preserve refs and path props', () => {
  const React = {forwardRef: render => render, createElement: (type, props) => ({type, props})};
  const resolve = createWindowsSvgComponents({React, nativeComponents: {RNSVGPath: 'FabricPath', RNSVGSvgIOS: 'FabricSvg'}});
  const Path = resolve('RNSVGPath');
  const original = {d: 'M0 0L10 10', fill: [0, 123], stroke: null};
  const result = Path(original, 'ref');
  assert.equal(result.type, 'FabricPath');
  assert.deepEqual(result.props, {...original, fill: {type: 0, payload: 123}, ref: 'ref'});
  assert.deepEqual(original.fill, [0, 123]);
  assert.equal(resolve('RNSVGPath'), Path);
  assert.equal(resolve('RNSVGSvgView')({}, null).type, 'FabricSvg');
  assert.equal(resolve('RCTView'), undefined);
  assert.throws(() => resolve('RNSVGText')({}, null), /UNSUPPORTED_WINDOWS_SVG_COMPONENT/);
});
