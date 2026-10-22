// Original adapter between the inspected legacy SVG ABI and RNW Fabric.
// The native components and generated view configs come from react-native-svg (MIT).
const supported = new Set(['RNSVGCircle', 'RNSVGClipPath', 'RNSVGDefs', 'RNSVGEllipse',
  'RNSVGGroup', 'RNSVGImage', 'RNSVGLine', 'RNSVGLinearGradient', 'RNSVGPath',
  'RNSVGRadialGradient', 'RNSVGRect', 'RNSVGUse', 'RNSVGSvgView']);

function normalizeBrush(value) {
  if (!Array.isArray(value)) return value;
  if (value[0] === 0 && value.length === 2 && Number.isFinite(value[1])) {
    return {type: 0, payload: value[1]};
  }
  if (value[0] === 1 && value.length === 2 && typeof value[1] === 'string') {
    return {type: 1, brushRef: value[1]};
  }
  if (value[0] === 2 && value.length === 1) return {type: 2};
  throw new Error('UNSUPPORTED_LEGACY_SVG_BRUSH');
}

function createWindowsSvgComponents({React, nativeComponents, onCompatibility = () => {}}) {
  const components = new Map();
  return name => {
    if (typeof name !== 'string' || !name.startsWith('RNSVG')) return undefined;
    if (components.has(name)) return components.get(name);
    const nativeName = name === 'RNSVGSvgView' ? 'RNSVGSvgIOS' : name;
    const NativeComponent = supported.has(name) && nativeComponents?.[nativeName];
    const Component = React.forwardRef((props, ref) => {
      if (!NativeComponent) throw new Error(`UNSUPPORTED_WINDOWS_SVG_COMPONENT:${name}`);
      const next = {...props, ref};
      for (const key of ['fill', 'stroke']) {
        if (Object.prototype.hasOwnProperty.call(props, key)) next[key] = normalizeBrush(props[key]);
      }
      return React.createElement(NativeComponent, next);
    });
    Component.displayName = `WindowsCompat${name}`;
    components.set(name, Component);
    onCompatibility({stage: 'legacy-svg-component-mapped', svgComponent: name, svgSupported: Boolean(NativeComponent)});
    return Component;
  };
}
module.exports = {createWindowsSvgComponents, normalizeBrush};
