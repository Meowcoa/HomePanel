// Exact local candidate. Self-test writes fixture data only; product mode never runs diagnostics that write values.
import React, {useEffect, useState} from 'react';
import * as RN from 'react-native';
import * as WindowsRenderer from 'react-native/Libraries/ReactNative/RendererProxy';
import * as SvgFabric from 'react-native-svg/src/fabric';
import * as Svg from 'react-native-svg';
const {createLegacyMetro} = require('./legacyMetro');
const {createLegacyOverrides} = require('./legacyOverrides');
const {createPhoneSdk} = require('./phoneSdkAdapter');
const {createDeviceBridge} = require('./deviceBridge');
const {verifyPhoneSdkBridge} = require('./phoneSdkRoundTrip');
const {decodeBase64} = require('./ramBundle');
const {parseRuntimeManifest} = require('./runtimeManifest');
const host = RN.NativeModules.HomePanelHost;

function diagnostic(error, loader, phase = 'error', compatibility = {}, runtimeMode = false) {
  if (runtimeMode) return;
  const normalized = error instanceof Error ? error : new Error(String(error));
  host.reportStage('official-error');
  host.reportPluginDiagnostic(JSON.stringify({phase, message: String(normalized.message || error).slice(0, 1500),
    stack: String(normalized.stack || '').split('\n').slice(0, 8).join('\n'),
    chain: normalized.legacyModuleChain?.map(id => loader?.describe(id)), stats: loader?.stats(),
    compatibility}), true);
}
class PluginBoundary extends React.Component {
  state = {failed: false};
  static getDerivedStateFromError() {return {failed: true};}
  componentDidCatch(error) {diagnostic(error, this.props.loader, 'render-error', {}, this.props.runtimeMode);}
  render() {return this.state.failed ? <RN.Text style={{padding: 20}}>此页面暂不兼容，请关闭插件并使用原生控制面板。</RN.Text> : this.props.children;}
}
function MountedPlugin({Component, verify}) {
  useEffect(() => {host.reportStage('official-mounted'); verify();}, [verify]);
  return <Component />;
}
export default function OfficialPluginLab({runtimeMode = false}) {
  const [notice, setNotice] = useState('');
  const [failure, setFailure] = useState('');
  const [writable, setWritable] = useState(false);
  const [state, setState] = useState({Component: null, loader: null});
  useEffect(() => {
    let active = true, phone, loader;
    const compatibility = {};
    function fail(error) {
      // Display only bounded protocol/compatibility error codes, never source or private data.
      const code = String(error?.message || '').match(/(?:UNSUPPORTED|INVALID|MISSING|PUBLIC_SPEC|READ_ONLY|CROSS_DEVICE|DIRECT_STATE|BROKER|LOCAL_PLUGIN|PLUGIN_MANIFEST|RAM_RECORD|REAL_SESSION|PRIVACY)[A-Z0-9_:.\/-]*/)?.[0];
      if (active) setFailure(code?.slice(0, 160) || 'PLUGIN_RUNTIME_ERROR');
    }
    const previousHandler = global.ErrorUtils?.getGlobalHandler?.();
    if (previousHandler) global.ErrorUtils.setGlobalHandler(error =>
      {fail(error);diagnostic(error, loader, 'uncaught-error', {...compatibility}, runtimeMode);});
    (async () => {
      const bridge = createDeviceBridge(RN.NativeModules.HomePanelBroker);
      const device = await bridge.getDevice();
      if (runtimeMode && device.testData) throw new Error('REAL_SESSION_REQUIRED');
      const manifest = parseRuntimeManifest(await host.getLocalPluginManifest());
      setWritable(device.writesAllowed === true);
      phone = createPhoneSdk({device, bridge, manifest, onNotice: notice => {
        if (runtimeMode && active) setNotice('该手机专用功能暂不可用，请使用原生面板');
        compatibility.sdkNotices = [...(compatibility.sdkNotices || []), String(notice).slice(0, 200)].slice(-6);
      }, onEntry(Component) {
        const Wrapper = phone.wrapperProvider?.();
        const Root = Wrapper ? () => <Wrapper><Component /></Wrapper> : Component;
        if (active) setState({Component: Root, loader, verify: () => {
          if (device.model === 'xiaomi.wifispeaker.l05c') phone.startPrivacyLifecycle().catch(error=>{
            fail(error);diagnostic(error,loader,'privacy-initialization-error',{},runtimeMode);
          });
          if (runtimeMode) return; // No automatic write probe on a real device.
          const check = manifest.kind === 'downloaded-rn'
            ? bridge.read([{siid: 2, piid: 1}]).then(() => ({readOnlyProbe: true, writeProbe: false}))
            : verifyPhoneSdkBridge(phone.sdk, device.testData);
          check.then(result => {
            if (!active) return;
            host.reportPluginDiagnostic(JSON.stringify({phase: 'sdk-bridge-confirmed', result, testData: true,
              officialButtonPressTested: false}), false);
            host.reportStage('broker');
          }).catch(error => diagnostic(error, loader, 'broker-error', {...compatibility}));
        }});
        host.reportStage('official-entry');
      }});
      let localAssets = {};
      localAssets = {files: JSON.parse(await host.getLocalPluginAssets()), base: await host.getLocalPluginAssetRoot()};
      const overrides = createLegacyOverrides({React, RN, phone, Renderer: WindowsRenderer, SvgFabric, Svg, localAssets,
        onCompatibility: info => {
          Object.assign(compatibility, info);
          host.reportPluginDiagnostic(JSON.stringify({phase: 'compatibility-probe', compatibility}), false);
        }});
      const shared = createLegacyMetro({override: overrides, react: React});
      const sdkSource = await host.getLocalPluginSource(0);
      shared.register(sdkSource);
      if (manifest.kind === 'downloaded-rn') {
        loader = createLegacyMetro({override: overrides, fallback: shared.require, react: React});
        loader.registerRam(decodeBase64(await host.getLocalPluginBytes(1)));
        for (const id of loader.stats().startupRequests) if (id !== manifest.entryModule) loader.require(id);
      } else {
        loader = shared;
        loader.register(await host.getLocalPluginSource(1));
      }
      loader.require(manifest.entryModule);
      phone.startLifecycle();
      host.reportPluginDiagnostic(JSON.stringify({phase: 'entry-executed', stats: loader.stats(),
        package: manifest.package, version: manifest.version, testData: device.testData === true}), false);
    })().catch(error => {fail(error);diagnostic(error, loader, 'initialization-error', {...compatibility}, runtimeMode);});
    return () => {
      active = false;
      if (previousHandler) global.ErrorUtils.setGlobalHandler(previousHandler);
      phone?.stopLifecycle();
    };
  }, [runtimeMode]);
  return <RN.View onLayout={() => host.reportStage('layout')} style={{flex: 1, backgroundColor: '#f7f7f7'}}>
    <RN.Text style={{fontSize: 11, color: '#974d31', textAlign: 'center', padding: 4}}>{runtimeMode ? `官方插件测试 · ${writable ? '允许控制' : '只读'} · 每约 3 秒同步` : '本机官方插件兼容试验 · 测试数据'}</RN.Text>
    {!!notice && <RN.Text style={{padding: 8, color: '#974d31'}}>{notice}</RN.Text>}
    <RN.Pressable onPress={() => host.closeWindow()} style={{padding: 8, alignSelf: 'flex-end'}}><RN.Text>关闭插件</RN.Text></RN.Pressable>
    {failure ? <RN.Text selectable style={{padding: 20}}>{failure.startsWith('PRIVACY')
      ? '无法确认该设备已有的米家隐私同意记录。请先在手机米家打开此设备并检查授权，然后重试。程序不会替你同意或修改云端记录。'
      : `插件启动失败或状态无法读取。兼容信息：${failure}。请关闭并使用原生控制。`}</RN.Text> : state.Component
      ? <PluginBoundary loader={state.loader} runtimeMode={runtimeMode}><MountedPlugin Component={state.Component} verify={state.verify}/></PluginBoundary>
      : <RN.Text style={{padding: 20}}>正在启动本机官方插件…</RN.Text>}
  </RN.View>;
}
