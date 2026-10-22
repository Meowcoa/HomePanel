// A Win32 client area has no phone notch or phone status-bar inset.
// Preserve normal layout and explicit numeric padding without old Animated private refs.
function createWindowsSafeArea(React, View) {
  const SafeAreaView = React.forwardRef(({forceInset, isLandscape, style, ...props}, ref) => {
    const padding = {};
    for (const [edge, axis] of [['Top', 'vertical'], ['Bottom', 'vertical'], ['Left', 'horizontal'], ['Right', 'horizontal']]) {
      const value = forceInset?.[edge.toLowerCase()] ?? forceInset?.[axis];
      if (typeof value === 'number' && Number.isFinite(value) && value >= 0) padding[`padding${edge}`] = value;
    }
    return React.createElement(View, {...props, ref, pointerEvents: props.pointerEvents ?? 'box-none',
      style: Object.keys(padding).length ? [style, padding] : style});
  });
  SafeAreaView.displayName = 'WindowsSafeAreaView';
  SafeAreaView.setStatusBarHeight = () => {}; // Phone status-bar hints do not change a Win32 client area.
  const withSafeArea = forceInset => Wrapped => {
    const Wrapper = React.forwardRef((props, ref) => React.createElement(SafeAreaView,
      {style: {flex: 1}, forceInset}, React.createElement(Wrapped, {...props, ref})));
    Wrapper.displayName = `WindowsSafeArea(${Wrapped.displayName || Wrapped.name || 'Component'})`;
    for (const key of ['navigationOptions', 'router']) {
      if (key in Wrapped) Object.defineProperty(Wrapper, key, Object.getOwnPropertyDescriptor(Wrapped, key) || {value: Wrapped[key]});
    }
    return Wrapper;
  };
  return {__esModule: true, default: SafeAreaView, withSafeArea};
}
module.exports = {createWindowsSafeArea};
