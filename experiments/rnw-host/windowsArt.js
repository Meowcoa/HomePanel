// Original ART-to-SVG adapter. Draw real vectors with the MIT react-native-svg
// implementation; unsupported data fails explicitly instead of drawing blanks.
function numbers(...values) {
  if (values.some(value => !Number.isFinite(Number(value)))) throw new Error('INVALID_ART_COORDINATE');
  return values.map(Number).join(' ');
}
class WindowsArtPath {
  constructor(source = '') {
    if (source instanceof WindowsArtPath) source = source.toString();
    if (typeof source !== 'string') throw new Error('UNSUPPORTED_ART_PATH');
    this.commands = source ? [source] : [];
  }
  command(name, ...values) {this.commands.push(`${name}${numbers(...values)}`); return this;}
  move(x, y) {return this.command('m', x, y);}
  moveTo(x, y) {return this.command('M', x, y);}
  line(x, y) {return this.command('l', x, y);}
  lineTo(x, y) {return this.command('L', x, y);}
  curve(...values) {if (values.length !== 6) throw new Error('UNSUPPORTED_ART_CURVE'); return this.command('c', ...values);}
  curveTo(...values) {if (values.length !== 6) throw new Error('UNSUPPORTED_ART_CURVE'); return this.command('C', ...values);}
  arc(x, y, rx, ry = rx, outer = false) {return this.command('a', Math.abs(rx), Math.abs(ry), 0, outer ? 1 : 0, 1, x, y);}
  counterArc(x, y, rx, ry = rx, outer = false) {return this.command('a', Math.abs(rx), Math.abs(ry), 0, outer ? 1 : 0, 0, x, y);}
  close() {this.commands.push('z'); return this;}
  reset() {this.commands = []; return this;}
  toString() {return this.commands.join(' ');}
}
class WindowsArtLinearGradient {
  constructor(stops, x1 = 0, y1 = 0, x2 = 1, y2 = 0) {
    if (!stops || typeof stops !== 'object') throw new Error('INVALID_ART_GRADIENT');
    this.stops = Array.isArray(stops) ? stops.map((color, i) => [stops.length <= 1 ? 0 : i / (stops.length - 1), color]) : Object.entries(stops);
    this.coordinates = {x1, y1, x2, y2};
  }
}
function createWindowsArt(React, Svg) {
  let nextId = 0;
  const surface = Svg.Svg || Svg.default;
  function transform(props) {
    const values = [];
    if (props.x != null || props.y != null) values.push(`translate(${numbers(props.x ?? 0, props.y ?? 0)})`);
    if (props.rotation != null) values.push(`rotate(${numbers(props.rotation, props.originX ?? 0, props.originY ?? 0)})`);
    if (props.scale != null || props.scaleX != null || props.scaleY != null) values.push(`scale(${numbers(props.scaleX ?? props.scale ?? 1, props.scaleY ?? props.scale ?? 1)})`);
    if (props.transform != null) {
      const matrix = props.transform;
      if (Array.isArray(matrix) && matrix.length === 6) values.push(`matrix(${numbers(...matrix)})`);
      else if (typeof matrix === 'string') values.push(matrix);
      else throw new Error('UNSUPPORTED_ART_TRANSFORM');
    }
    return values.join(' ') || undefined;
  }
  const Surface = React.forwardRef(({width, height, children, style, ...rest}, ref) =>
    React.createElement(surface, {...rest, ref, width, height, style}, children));
  const Group = React.forwardRef(({children, ...props}, ref) =>
    React.createElement(Svg.G, {ref, opacity: props.opacity, transform: transform(props)}, children));
  const Shape = React.forwardRef((props, ref) => {
    const definitions = [];
    function brush(value) {
      if (!(value instanceof WindowsArtLinearGradient)) return value ?? 'none';
      const id = `homepanel-art-${++nextId}`;
      definitions.push(React.createElement(Svg.LinearGradient, {key: id, id, gradientUnits: 'userSpaceOnUse', ...value.coordinates},
        value.stops.map(([offset, color], i) => React.createElement(Svg.Stop, {key: i, offset, stopColor: color}))));
      return `url(#${id})`;
    }
    const path = props.d instanceof WindowsArtPath ? props.d.toString() : props.d;
    if (typeof path !== 'string') throw new Error('UNSUPPORTED_ART_PATH');
    const node = React.createElement(Svg.Path, {ref, d: path, fill: brush(props.fill), stroke: brush(props.stroke),
      strokeWidth: props.strokeWidth, strokeLinecap: props.strokeCap, strokeLinejoin: props.strokeJoin,
      strokeDasharray: props.strokeDash, fillRule: 'nonzero', opacity: props.opacity, transform: transform(props),
      onPress: props.onPress, onPressIn: props.onPressIn, onPressOut: props.onPressOut});
    return definitions.length ? React.createElement(Svg.G, null, React.createElement(Svg.Defs, null, ...definitions), node) : node;
  });
  const Text = React.forwardRef(({children, font, alignment, ...props}, ref) => React.createElement(Svg.Text, {
    ...props, ref, ...(typeof font === 'object' ? {fontFamily: font.fontFamily, fontSize: font.fontSize, fontWeight: font.fontWeight} : {}),
    textAnchor: alignment === 'center' ? 'middle' : alignment === 'right' ? 'end' : 'start',
  }, children));
  const ClippingRectangle = React.forwardRef(({width, height, children, ...props}, ref) => {
    const id = `homepanel-art-clip-${++nextId}`;
    return React.createElement(Svg.G, {ref, transform: transform(props), clipPath: `url(#${id})`},
      React.createElement(Svg.Defs, null, React.createElement(Svg.ClipPath, {id}, React.createElement(Svg.Rect, {width, height}))), children);
  });
  return {Surface, Group, Shape, Text, ClippingRectangle, Path: WindowsArtPath, LinearGradient: WindowsArtLinearGradient};
}
module.exports = {createWindowsArt, WindowsArtPath, WindowsArtLinearGradient};
