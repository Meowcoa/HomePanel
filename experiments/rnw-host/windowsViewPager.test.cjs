const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createLegacyAnimated, createWindowsViewPager, mapPagerScrollOffset} = require('./windowsViewPager');

test('maps Windows scroll offsets to Android pager position and fraction', () => {
  assert.deepEqual(mapPagerScrollOffset(125, 100, 3), {position: 1, offset: 0.25});
  assert.deepEqual(mapPagerScrollOffset(-20, 100, 3), {position: 0, offset: 0});
  assert.deepEqual(mapPagerScrollOffset(450, 100, 3), {position: 2, offset: 0});
  assert.deepEqual(mapPagerScrollOffset(80, 0, 3), {position: 0, offset: 0});
});

test('legacy animated events always use the JavaScript driver on Windows', () => {
  const calls = [];
  const animated = {event: (mapping, options) => {calls.push({mapping, options}); return 'handler';}};
  const legacy = createLegacyAnimated(animated);
  const pagerMapping = [{nativeEvent: {position: {}, offset: {}}}];
  const regularMapping = [{nativeEvent: {contentOffset: {x: {}}}}];
  assert.equal(legacy.event(pagerMapping, {useNativeDriver: true, listener() {} }), 'handler');
  assert.equal(calls[0].options.useNativeDriver, false);
  assert.equal(calls[0].options.listener instanceof Function, true);
  legacy.event(regularMapping, {useNativeDriver: true});
  assert.equal(calls[1].options.useNativeDriver, false);
});

test('legacy timing and spring animations are forced off the incompatible native driver', () => {
  const calls = [];
  const animated = {
    event: () => {},
    timing: (value, config) => {calls.push(['timing', value, config]); return 'timing';},
    spring: (value, config) => {calls.push(['spring', value, config]); return 'spring';},
  };
  const legacy = createLegacyAnimated(animated);
  assert.equal(legacy.timing('x', {useNativeDriver: true, duration: 100}), 'timing');
  assert.equal(legacy.spring('y', {useNativeDriver: true}), 'spring');
  assert.deepEqual(calls.map(row => row[2].useNativeDriver), [false, false]);
});

test('Windows pager exposes its Android compatibility name and translates scroll callbacks', () => {
  const React = {
    Children: {count: children => Array.isArray(children) ? children.length : children == null ? 0 : 1},
    createElement: (type, props, children) => ({type, props, children}),
    forwardRef: render => render,
    useImperativeHandle: (ref, create) => {ref.current = create();},
    useRef: current => ({current}),
  };
  const RN = {Dimensions: {get: () => ({width: 100})}, ScrollView: 'ScrollView'};
  const Pager = createWindowsViewPager(React, RN);
  const ref = {current: null}, scrollEvents = [], selected = [];
  const element = Pager({children: ['one', 'two', 'three'], initialPage: 2, onPageScroll: event => scrollEvents.push(event),
    onPageSelected: event => selected.push(event)}, ref);
  const nativeCalls = [];
  element.props.ref.current = {scrollTo: value => nativeCalls.push(value)};
  element.props.onLayout({nativeEvent: {layout: {width: 100}}});
  element.props.onScroll({nativeEvent: {contentOffset: {x: 125}}});
  element.props.onMomentumScrollEnd({nativeEvent: {contentOffset: {x: 225}}});
  assert.equal(Pager.displayName, 'WindowsCompatViewPagerAndroid');
  assert.deepEqual(nativeCalls, [{x: 200, y: 0, animated: false}]);
  assert.deepEqual(scrollEvents, [{nativeEvent: {position: 1, offset: 0.25}}]);
  assert.deepEqual(selected, [{nativeEvent: {position: 2}}]);
  assert.equal(ref.current.getNode().setPageWithoutAnimation instanceof Function, true);
});
