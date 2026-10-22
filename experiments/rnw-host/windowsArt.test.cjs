const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createWindowsArt, WindowsArtPath, WindowsArtLinearGradient} = require('./windowsArt');
const React = {forwardRef: render => render, createElement: (type, props, ...children) => ({type, props, children})};
const Svg = Object.fromEntries(['Svg','G','Path','Defs','LinearGradient','Stop','Text','ClipPath','Rect'].map(name => [name, name]));
test('ART ring paths retain arc directions and relative coordinates', () => {
  const ring = new WindowsArtPath().move(0, 125).arc(250, 0, 125).arc(-250, 0, 125)
    .move(15, 0).counterArc(220, 0, 110).counterArc(-220, 0, 110).close();
  assert.equal(ring.toString(), 'm0 125 a125 125 0 0 1 250 0 a125 125 0 0 1 -250 0 m15 0 a110 110 0 0 0 220 0 a110 110 0 0 0 -220 0 z');
  assert.throws(() => new WindowsArtPath().move(NaN, 0), /INVALID_ART_COORDINATE/);
});
test('ART shapes produce SVG paths with real brushes and transforms', () => {
  const art = createWindowsArt(React, Svg);
  const shape = art.Shape({d: new art.Path().moveTo(0, 0).line(5, 5), fill: '#0890ec', x: 4, y: 6}, 'ref');
  assert.equal(shape.type, 'Path');
  assert.equal(shape.props.d, 'M0 0 l5 5');
  assert.equal(shape.props.fill, '#0890ec');
  assert.equal(shape.props.transform, 'translate(4 6)');
  assert.equal(shape.props.ref, 'ref');
  assert.equal(art.Surface({width: 250, height: 250, children: shape}).type, 'Svg');
});
test('ART gradients create SVG definitions and are referenced by the rendered shape', () => {
  const art = createWindowsArt(React, Svg);
  const shape = art.Shape({d: 'M0 0 L5 5', fill: new WindowsArtLinearGradient({'0': 'purple', '1': 'yellow'}, 0, 0, 250, 0)});
  assert.equal(shape.type, 'G');
  assert.equal(shape.children[0].type, 'Defs');
  assert.equal(shape.children[0].children[0].type, 'LinearGradient');
  assert.match(shape.children[1].props.fill, /^url\(#homepanel-art-/);
});
