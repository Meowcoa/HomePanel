const {createLocalAssets} = require('./localAssets');
const {channel} = require('./phoneSdkAdapter');
const {createWindowsNetInfo} = require('./windowsNetInfo');
const {createLegacyAnimated, createWindowsViewPager} = require('./windowsViewPager');
const {createLegacyLinking} = require('./legacyLinking');
const {createWindowsSvgComponents} = require('./windowsSvg');
const {createWindowsSafeArea} = require('./windowsSafeArea');
const {createLegacyText} = require('./windowsLegacyText');
const {createWindowsArt} = require('./windowsArt');
function createLegacyOverrides({React, RN, phone, SvgFabric, Svg, Renderer = {findNodeHandle: RN.findNodeHandle}, onRegister = () => {}, onCompatibility = () => {}, localAssets = {}}) {
  const sdk = phone.sdk;
  let sharedRequire;
  const assets = createLocalAssets(localAssets);
  const properties = new WeakMap();
  const nativeEvents = new Map();
  const svgComponent = createWindowsSvgComponents({React, nativeComponents: SvgFabric, onCompatibility});
  const requireNativeComponent = name => svgComponent(name) ?? RN.requireNativeComponent(name);
  class EventEmitter {
    addListener(name, fn) {if (!nativeEvents.has(name)) nativeEvents.set(name, channel()); return nativeEvents.get(name).addListener(fn);}
    emit(name, ...args) {nativeEvents.get(name)?.emit(...args);}
    removeAllListeners() {nativeEvents.clear();}
  }
  const emitter = new EventEmitter();
  const netInfo = createWindowsNetInfo(RN.NativeModules.HomePanelHost);
  // Babel's old wildcard helper copies own enumerable keys, not prototype keys.
  const legacyRN = {};
  for (const [key, descriptor] of Object.entries(Object.getOwnPropertyDescriptors(RN))) {
    Object.defineProperty(legacyRN, key, {...descriptor, configurable: true});
  }
  const legacyAnimated = createLegacyAnimated(RN.Animated);
  const viewPagerAndroid = createWindowsViewPager(React, RN);
  const legacyLinking = createLegacyLinking(RN.Linking);
  onCompatibility({stage: 'pager-shim-created', viewPagerType: typeof viewPagerAndroid,
    viewPagerDisplayName: viewPagerAndroid?.displayName || null,
    animatedFactoryType: typeof legacyAnimated?.createAnimatedComponent});
  if (typeof legacyAnimated?.createAnimatedComponent === 'function') {
    const createAnimatedComponent = legacyAnimated.createAnimatedComponent;
    legacyAnimated.createAnimatedComponent = Component => {
      onCompatibility({stage: 'animated-component-request', componentType: typeof Component,
        componentDisplayName: Component?.displayName || Component?.name || null});
      if (Component == null) throw new Error('WINDOWS_COMPAT_ANIMATED_COMPONENT_UNDEFINED');
      return createAnimatedComponent(Component);
    };
  }
  const appRegistry = Object.create(RN.AppRegistry || {});
  appRegistry.registerComponent = (name, provider) => {onRegister(name, provider()); return name;};
  appRegistry.setWrapperComponentProvider = provider => {phone.wrapperProvider = provider;};
  const native = {
    ...RN.NativeModules,
    language: 'zh',
    MIOTHost: {...sdk.Host, language: 'zh_CN', isDarkMode: false, buildType: 'debug'},
    MIOTPackage: {...sdk.Package, basePath: '', localFilePath: '', plugPath: '', apiReport() {}},
    MIOTDevice: sdk.Device,
    MIOTRPC: {nativeCall(path, args, callback) {
      phone.nativeCall(path, args).then(reply => callback(true, reply), error => callback(false, {code: error.message}));
    }, standardCall(path, args, callback) {
      phone.nativeStandardCall(path, args).then(reply => callback(true, reply), error => callback(false, {code: error.message}));
    }},
    MIOTService: {addLog() {}, getServerName: callback => phone.nativeModules.getServerName(callback)},
    LocalCache: {bluetoothDevices: new Map(), clear() {this.bluetoothDevices.clear();}},
    RNGestureHandlerModule: {
      Direction: {RIGHT: 1, LEFT: 2, UP: 4, DOWN: 8},
      State: {UNDETERMINED: 0, FAILED: 1, BEGAN: 2, CANCELLED: 3, ACTIVE: 4, END: 5},
      createGestureHandler() {throw new Error('UNSUPPORTED_PHONE_VIEW:NativeGestureHandler');},
      attachGestureHandler() {throw new Error('UNSUPPORTED_PHONE_VIEW:NativeGestureHandler');},
    },
  };
  Object.defineProperties(legacyRN, {
    AppRegistry: {value: appRegistry}, NativeModules: {value: native},
    requireNativeComponent: {value: requireNativeComponent, enumerable: true},
    DeviceEventEmitter: {value: emitter}, NativeEventEmitter: {value: EventEmitter},
    Animated: {value: legacyAnimated}, ViewPagerAndroid: {value: viewPagerAndroid, enumerable: true},
    Linking: {value: legacyLinking, enumerable: true},
    ViewPropTypes: {value: {}}, TextPropTypes: {value: {}}, ImagePropTypes: {value: {}},
    ART: {value: createWindowsArt(React, Svg), enumerable: true},
    NetInfo: {value: netInfo.default, enumerable: true},
  });
  const react = {...React};
  react.createFactory = type => React.createElement.bind(null, type);
  const nativeIndex = {
    __esModule: true, default: native, isAndroid: false, isIOS: true,
    NativeType: 'ios', ANDROID: 'android', IOS: 'ios', RELEASE: 'release', DEBUG: 'debug',
    SetTimeout: setTimeout, ClearTimeout: clearTimeout,
    Properties: {init(target, values) {properties.set(target, values); return target;}, of(target) {return properties.get(target) || {}; }},
    getSystemLanguage: () => 'zh', MIOTEventEmitter: emitter,
    createEventManager: def => Object.fromEntries(Object.keys(def).map(key => [key, channel()])),
    buildEvents(def) {for (const key of Object.keys(def)) def[key] = channel();},
    PackageExitAction: {register() {}, unregister() {}},
    NativeTimer: {addListener(fn, delay) {const id = setInterval(fn, delay || 5000); return {remove: () => clearInterval(id)};}},
    Utils: {getStandardLanguageName: () => 'zh', isEmptyObject: obj => !obj || !Object.keys(obj).length},
    createI18n(settings, defaultLang = 'en') {
      if (sharedRequire?.named) {
        const module = sharedRequire.named('miot-sdk/native/common/i18n.js');
        const I18n = module.default || module;
        return new I18n(settings, defaultLang);
      }
      const strings = {...(settings?.[defaultLang] || {}), ...(settings?.zh || {})};
      return {...strings, strings, language: 'zh', get: key => strings[key] || key,
        formatString: (text, ...args) => String(text).replace(/\{(\d+)\}/g, (_, n) => args[Number(n)] ?? '')};
    },
    // Phone plugins may import System from `miot-sdk/native` instead of the
    // top-level SDK entry. Expose the same adapter from both paths.
    System: sdk.System,
  };
  sdk.Resources.createI18n = nativeIndex.createI18n;
  sdk.Utils = nativeIndex.Utils;
  class UnsupportedBluetoothDevice {constructor() {throw new Error('UNSUPPORTED_PHONE_API:BluetoothDevice');}}
  const explicit = new Map([
    ['node_modules/react-native-safe-area-view/index.js', createWindowsSafeArea(React, RN.View)],
    ['node_modules/react-native/Libraries/Image/AssetRegistry.js', assets],
    ['node_modules/@react-native/assets/registry.js', assets],
    ['node_modules/react-native/Libraries/Image/resolveAssetSource.js', assets.resolve],
    ['miot-sdk/index.js', sdk], ['miot-sdk/native/index.js', nativeIndex],
    ['miot-sdk/Device.js', {__esModule: true, default: sdk.Device, DeviceEvent: sdk.DeviceEvent, DarkMode: sdk.DarkMode}],
    ['miot-sdk/device/BasicDevice.js', {__esModule: true, default: sdk.Device, DeviceEvent: sdk.DeviceEvent}],
    ['miot-sdk/Host.js', {__esModule: true, default: sdk.Host, HostEvent: sdk.HostEvent}],
    ['miot-sdk/host/locale.js', {__esModule: true, default: sdk.Host.locale}],
    ['miot-sdk/host/storage.js', {__esModule: true, default: sdk.Host.storage}],
    ['miot-sdk/Package.js', {__esModule: true, default: sdk.Package, PackageEvent: sdk.PackageEvent, Entrance: sdk.Entrance}],
    ['miot-sdk/service/spec.js', {__esModule: true, default: sdk.Service.spec}],
    ['node_modules/react-native/Libraries/Core/InitializeCore.js', {}],
    ['node_modules/react-native/Libraries/react-native/react-native-implementation.js', legacyRN],
    ['node_modules/react-native/Libraries/BatchedBridge/NativeModules.js', native],
    ['node_modules/react-native/Libraries/EventEmitter/NativeEventEmitter.js', EventEmitter],
    ['node_modules/react-native/Libraries/EventEmitter/RCTDeviceEventEmitter.js', emitter],
    ['node_modules/react-native/Libraries/ReactNative/AppRegistry.js', appRegistry],
    ['node_modules/react-native/Libraries/Utilities/Platform.android.js', RN.Platform],
    ['node_modules/react-native/Libraries/Utilities/Dimensions.js', RN.Dimensions],
    ['node_modules/react-native/Libraries/StyleSheet/StyleSheet.js', RN.StyleSheet],
    ['node_modules/react-native/Libraries/ReactNative/requireNativeComponent.js', requireNativeComponent],
    ['node_modules/react-native/Libraries/ReactNative/UIManager.js', RN.UIManager],
    ['node_modules/react-native/Libraries/ReactNative/NativeUIManager.js', RN.UIManager],
    ['node_modules/react-native/Libraries/LayoutAnimation/LayoutAnimation.js', RN.LayoutAnimation],
    ['node_modules/react-native/Libraries/Components/Keyboard/Keyboard.js', RN.Keyboard],
    ['node_modules/react-native/Libraries/Renderer/shims/ReactNative.js', Renderer],
    ['node_modules/react-native/Libraries/Linking/Linking.js', {__esModule: true, default: legacyLinking, Linking: legacyLinking}],
    ['node_modules/react/index.js', react],
    ['node_modules/@react-native-community/netinfo/src/index.ts', netInfo],
    ['miot-sdk/device/bluetooth/BluetoothDevice.js', {__esModule: true,
      default: UnsupportedBluetoothDevice, IBluetooth: UnsupportedBluetoothDevice,
      BluetoothEvent: sdk.DeviceEvent}],
  ]);
  const components = ['View','Text','Image','ScrollView','FlatList','SectionList','TextInput','Switch','Modal','ActivityIndicator',
    'TouchableOpacity','TouchableHighlight','TouchableWithoutFeedback','TouchableNativeFeedback','Pressable','SafeAreaView','KeyboardAvoidingView'];
  let metadataInstalled = false;
  return ({id, name, require: legacyRequire}) => {
    if (name === 'miot-sdk/index.js' || name === 'miot-sdk/native/index.js') sharedRequire = legacyRequire;
    if (name === 'node_modules/intl/index.js' || name === 'node_modules/intl/lib/core.js' || name?.startsWith('node_modules/intl/locale-data/')) {
      if (typeof Intl?.DateTimeFormat !== 'function' || typeof Intl?.NumberFormat !== 'function') throw new Error('UNSUPPORTED_PHONE_API:Intl');
      // Windows Hermes supplies native Intl; phone locale registration is unnecessary.
      return name.includes('/locale-data/') ? {} : Intl;
    }
    if (name === 'node_modules/react-native/Libraries/react-native/react-native-implementation.js') {
      onCompatibility({stage: 'react-native-implementation-intercepted', moduleId: id,
        viewPagerType: typeof legacyRN.ViewPagerAndroid,
        viewPagerDisplayName: legacyRN.ViewPagerAndroid?.displayName || null});
    }
    if (!metadataInstalled && name === 'node_modules/react-native/Libraries/react-native/react-native-implementation.js') {
      metadataInstalled = true;
      const PropTypes = legacyRequire.named('node_modules/prop-types/index.js');
      for (const key of ['Text','View','Image']) {
        const Component = RN[key];
        const wrapped = key === 'Text' ? createLegacyText(React, Component) : React.forwardRef((props, ref) => React.createElement(Component, {...props, ...(key === 'Image' ? {source:assets.resolve(props.source)} : {}), ref}));
        if (key === 'Image') wrapped.resolveAssetSource=assets.resolve;
        wrapped.displayName = `WindowsCompat${key}`;
        wrapped.propTypes = {style: PropTypes.oneOfType([PropTypes.object, PropTypes.array, PropTypes.number]),
          children: PropTypes.node, source: PropTypes.oneOfType([PropTypes.object, PropTypes.array, PropTypes.number])};
        Object.defineProperty(legacyRN, key, {value: wrapped, enumerable: true, configurable: true});
      }
    }
    if (explicit.has(name)) return explicit.get(name);
    if (name?.startsWith('node_modules/react-native/Libraries/')) {
      for (const key of components) if (name.endsWith(`/${key}.js`)) return legacyRN[key];
    }
    return undefined;
  };
}
module.exports = {createLegacyOverrides};
