# Windows RN 宿主试验

## 0.4.3 集成测试入口

本目录继续保留宿主源码。0.4.3 可将其作为独立组件装进集成安装器；下文“只留实验区、不进安装包”等描述属于 0.4.2 的历史状态。主程序仍是原生 Rust，不需要启动浏览器。

安装后从设置导入本机 APK，再从单路插座详情的信息页打开官方插件。初次用只读；另一个按钮允许修改这台设备，Rust 继续校验并确认。真实模式不运行 fixture 的自动写入诊断，也不把设备异常详情写入试验报告。用户关闭插件或退出账号/主程序时关闭子进程，异常父进程退出也会触发宿主退出。

本机资源按 Android Metro 命名转换为 file URI，仅来自精确 ZIP 哈希匹配的光栅图片。当前实现/单元测试不代表全部官方图标已目视验收。专用手机 API 缺失仍会拒绝。闭源 JS、图片、原包不随安装器或源码分发。

集成构建：先用 `Build-Host.ps1` 构建宿主，再用 `scripts/Prepare-VcRuntime.ps1` 只读取微软签名安装档中的 CAB，使用 `scripts/Stage-PluginHost.ps1` 生成许可齐全的运行时目录。将目录传给 `Build-Installer.ps1 -PluginHostDirectory <目录>`，再以相同参数运行 `Test-Installer.ps1`。CI 的手动运行勾选 `include_plugin_host` 会启用此路径；本机通过不等于已在 GitHub Runner 上运行。

本目录是开发试验入口，不进入原生 EXE 或安装包。独立 React Native Windows 环境采用 0.79.5 / React Native 0.79.5 / React 19.0.0；米家样本内置 RN 元数据为 0.61.0，不能视为版本兼容。选择新版本是为了验证独立 Win32 Composition 宿主的可构建性，不是绕过官方插件的宿主依赖。

`index.js` 和 `DevicePanel.js` 是自有开发界面。未连接 broker 时显示“未连接真实设备”；自检模式使用明确标注的内存测试插座；真实开发预览仅绑定命令行选定的一台设备。独立的 `OfficialPluginLab.js` 仅在显式的本机官方插件自检中执行用户提供的精确版本样本，拒绝真实设备。插件原包及 SDK bundle 均不放进本目录；未实现的手机接口明确拒绝。

## 本机试验结果，2026-10-20

1. 在工作区 `work/HomePanel-rnw-experiment/` 完成锁定 npm 安装和官方 `cpp-app` 工程生成，自动链接命令成功。
2. 自有测试屏幕通过 Metro 的 Windows Release JavaScript 打包。**只证明 JS 能打包，不证明原生窗口能运行。**
3. 工作区补齐 C++ MSBuild targets、Roslyn 4.14.0、Windows SDK Contracts 10.0.26100.1、NuGet、Windows App SDK 及 PRI 构建任务后，Release x64 原生构建成功。没有修改系统 Visual Studio 安装、虚拟化或服务。上游 Hermes 对可选全局变量及 C# LIB 路径仍有构建警告，不能将此记录写成零警告构建。
4. 真正启动 Win32 Composition 窗口，原生 View 布局、JS → C++、Promise 和原生事件往返自检通过，并自动退出。没有用屏幕自动化操作用户界面。
5. 增加原创 `HomePanelBroker` 匿名管道传输和 Rust 单设备 broker。测试数据端到端完成属性读取、写入确认、独立重读和轮询订阅回调。83 项 Rust 测试、4 项 JS 订阅测试通过；真实账号和物理设备均未访问。
6. 本机通用插座候选 `miot.plugin.spec` 1.1.10 已完成旧 Metro 模块加载、React 兼容层、SVG Fabric 映射及 Windows client-area safe-area 适配。其 React 树挂载后通过稳定性检查，没有捕获到渲染异常。没有验收 GPU 最终像素、图片资源、全部菜单或官方按钮点击路径。
7. 在该插件会话中，经提供给插件的 `Service.spec` 接口完成测试插座读取、修改、独立回读、`DeviceEvent` 轮询通知和恢复初始值。事件采用 `Map` 与带秒级观察时间的原始记录。通知约每 3 秒轮询；不是云端推送。JS 回归测试覆盖旧 SVG 颜色格式、safe-area 原生 ref、事件格式和订阅异常。真实账号和物理设备均未访问。
8. 本机 Hermes bytecode version 为 96，APK 内小插件为 89；未做字节码转换，不能混用。两个 RAM-bundle 候选尚未执行。逐包范围见 `PLUGIN_TEST_MATRIX.json`；当前结果只覆盖这一份 plain bundle 和测试设备。

原始安装、生成、Metro 和 MSBuild 日志留在上述工作区目录，不打包到开源源码或安装器。

## 原生构建与自检

本目录现在保留原生 C++ 源码，不需要再次初始化工程。开发环境需要 Node.js 22+、PowerShell 7、VS 2022 C++ 工具和 Windows SDK。构建目录应放在源码之外：

```powershell
npm ci --ignore-scripts --no-audit --no-fund
node --test
./Build-Host.ps1 -OutputDirectory C:/HomePanelBuild/rnw
```

每个原生命令后检查 `$LASTEXITCODE`，非零时停止；PowerShell 脚本自身通过异常报告失败。正常开发依赖见 [RN Windows 0.79 官方说明](https://microsoft.github.io/react-native-windows/v1/docs/0.79/rnw-dependencies)。本机验证用的是 `work/HomePanel-rnw-experiment/` 下的工作区工具配置；没有在干净虚拟机重测常规 VS 构建入口。宿主使用 `WindowsPackageType=None` 和 Windows App SDK self-contained 输出，开发目录包含多个运行时 DLL；不能把这个 EXE 单独复制出去声称可用。

`prepareSvgFabric.cjs` 只接受 `react-native-svg` 15.12.1 的两个精确 SHA-256 上游文件，为 Fabric 临时启用静态库、Hybrid CRT、Windows 自动链接和各工程独立的中间目录。主机使用 C++20。原文件先备份到外部构建目录，`Build-Host.ps1` 在成功或失败后恢复其原始字节；并发修改时停止恢复并保留备份。意外终止后可用 `node ./prepareSvgFabric.cjs restore <备份目录>` 恢复。不会修改系统 VS 或 npm 共享存储。

Rust Debug 编译后执行：

```powershell
./Test-Host.ps1 -HostExe C:/HomePanelBuild/rnw/HomePanelPluginHost.exe -DebugHomePanelExe C:/HomePanelBuild/rust/debug/homepanel.exe -EvidenceDirectory C:/HomePanelBuild/evidence-001
```

自检不创建 HomePanel 账号目录，固定使用测试数据；Windows 自己可能创建空的系统缓存目录。已有报告会先保留，只有新生成且全部检查通过的报告才算成功。测试窗口自动关闭。它不验证官方插件、GPU 最终像素、全部设备或真实云端接口。

## 本机官方插座插件自检

仅在本机研究输出目录的 `LocalPlugin/` 放入用户合法持有样本的 `sdk.bundle`、`switch.bundle`，运行 `Test-OfficialPlugin.ps1`。精确源文件哈希固定在脚本中；没有自动下载或绕过签名处理，也不将原包加入 GitHub、安装器或演示素材。

```powershell
./Test-OfficialPlugin.ps1 -HostExe C:/HomePanelBuild/rnw/HomePanelPluginHost.exe -DebugHomePanelExe C:/HomePanelBuild/rust/debug/homepanel.exe -EvidenceDirectory C:/HomePanelBuild/official-evidence-001
```

测试同时要求原生桥接、插件入口与挂载、SDK broker 往返及完整条件满足后的稳定期。任何捕获到的渲染/订阅异常均使检查失败，诊断文件中的错误也单独判失败。证据包含本次 EXE、Hermes bundle 和本地插件源文件的哈希。SDK 自检通过接口调用验证测试设备，尚未自动点击官方按钮。

## 单设备桥接的边界

开发版 `homepanel.exe --plugin-host <宿主 EXE> --device-id <本机已登录账号中的设备 ID>` 才会初始化真实会话和查询设备。本轮没有执行此命令。默认只读；需要真实修改时必须额外显式传入 `--allow-device-writes`，再由用户点击控件。设备 ID 不要发到公开日志或 Issue。

- Rust 保留凭据，匿名管道只暴露当前设备的名称、型号、规格和指定属性结果，不提供家庭列表、token、任意 URL、RPC、动作、删除或 OTA。
- 支持 `getDevice`、`read`、单属性 `write`。未知字段、设备 ID 注入、重复目标、超过 12 个读取目标、SPEC v3 模块编号和不支持的权限均拒绝；修改继续复用原来的 SPEC 校验及直读回读。
- 真实预览以会话世代和型号约束设备；账号失效或设备变化后拒绝旧请求。直读失败/缓存回退明确返回 `available=false`，不把缓存伪装成当前值。
- `deviceBridge.js` 的 `subscribe` 是非重叠轮询：成功后约 3 秒，失败退到 15 秒，移除后取消定时及在途回调。不是小米云实时推送，notify-only 属性不能因此读取。
- `DevicePanel.js` 只提供布尔开关与最多 12 项只读显示。其他专用控件及完整手机插件 SDK 适配仍未完成；这不是发布版设备详情的替换界面。
- 原生管道断开或 20 秒未得到完整回复时拒绝；超时后关闭桥接，避免把迟到回复配给下一次调用。Rust 限制帧长、请求频率和单次会话数量。

以上 broker、fixture 与开发 CLI 通过 `cfg(debug_assertions)` 排除出正式 Rust EXE。正常启动不创建 RN 进程，安装器也不加入 RN 运行时。没有将匿名管道称为不可信 JS 的操作系统沙箱。

后续仍需逐包资源解析、手机专用控件及私有 API、官方按钮交互、真实设备与逐版本验收，然后集成窗口组。`RNSVGText`、Mask、Pattern 等未实现的 Fabric 控件在实际渲染时明确失败；导入过某组件不等于已使用或验证它。正式程序继续使用原生面板，实验通过不改变正式版的兼容标识。

React Native Windows 为 Microsoft 及贡献者的 MIT 项目；`react-native-svg` 15.12.1 为 Horcrux 及贡献者的 MIT 项目，许可见 `LICENSE-SVG.txt`。此试验只在本目录声明开发依赖，不随原生产品分发 npm 模块或 Xiaomi 插件。
