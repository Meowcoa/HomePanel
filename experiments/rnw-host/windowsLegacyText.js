// Some legacy phone plugins customize Text.prototype.render. Modern RN Text is a forwardRef.
// Keep that customization local to this legacy class, never mutate Windows RN.Text.
function createLegacyText(React, NativeText) {
  return class WindowsLegacyText extends React.Component {
    setNativeProps(props) {this.native?.setNativeProps?.(props);}
    measure(...args) {this.native?.measure?.(...args);}
    measureInWindow(...args) {this.native?.measureInWindow?.(...args);}
    measureLayout(...args) {this.native?.measureLayout?.(...args);}
    focus() {this.native?.focus?.();}
    blur() {this.native?.blur?.();}
    render() {return React.createElement(NativeText, {...this.props, ref: node => {this.native = node;}});}
  };
}
module.exports = {createLegacyText};
