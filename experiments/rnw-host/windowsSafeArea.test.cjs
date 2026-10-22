const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createWindowsSafeArea} = require('./windowsSafeArea');
const React = {forwardRef: render => render,
  createElement: (type, props, ...children) => ({type, props, children})};

test('desktop safe area forwards native refs and layout callbacks without a phone measurement', () => {
  const api = createWindowsSafeArea(React, 'View');
  const onLayout = () => {}, style = {flex: 1, paddingTop: 9};
  const result = api.default({forceInset: {top: 'always'}, isLandscape: true, onLayout, style}, 'native-ref');
  assert.equal(result.type, 'View');
  assert.equal(result.props.ref, 'native-ref');
  assert.equal(result.props.onLayout, onLayout);
  assert.equal(result.props.style, style);
  assert.equal('forceInset' in result.props, false);
  assert.equal('isLandscape' in result.props, false);
  assert.deepEqual(api.default({forceInset: {horizontal: 8, top: 12}}, null).props.style,
    [undefined, {paddingTop: 12, paddingLeft: 8, paddingRight: 8}]);
});

test('safe area wrapper retains route configuration and forwards the child ref', () => {
  const api = createWindowsSafeArea(React, 'View');
  function Screen() {}
  Screen.navigationOptions = {title: '设备'};
  const Wrapped = api.withSafeArea({top: 'never'})(Screen);
  const result = Wrapped({device: 'fixture'}, 'ref');
  assert.equal(Wrapped.navigationOptions, Screen.navigationOptions);
  assert.equal(result.children[0].props.ref, 'ref');
  assert.equal(result.children[0].props.device, 'fixture');
});
