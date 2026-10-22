const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createLegacyText} = require('./windowsLegacyText');
test('phone Text render customizations never alter the native Text component', () => {
  class Component {constructor(props) {this.props = props;}}
  const React = {Component, createElement: (type, props) => ({type, props})};
  const NativeText = {};
  const Text = createLegacyText(React, NativeText);
  const render = Text.prototype.render;
  Text.prototype.render = function() {const result = render.call(this);return {...result, props: {...result.props, style: [result.props.style, {fontFamily: ''}]}};};
  const instance = new Text({children: '米家', style: {color: 'red'}});
  const result = instance.render();
  assert.equal(result.type, NativeText);
  assert.deepEqual(result.props.style, [{color: 'red'}, {fontFamily: ''}]);
  let measured = false;
  result.props.ref({measure: () => {measured = true;}});
  instance.measure();
  assert.equal(measured, true);
  assert.equal(NativeText.render, undefined);
});
