# 米家设备插件在 Windows 运行的可行性

**0.4.11 插件管理与宿主更新（2026-10-21）：** 已实现完整的下载记录列表、按所选记录准备/运行的入口、SDK 10116 查询下载、本机 APK 公开根导入、Windows CryptoAPI 验签和逐文件运行资源核对。入口为“设置 → 设备面板 → 已下载插件”，设备信息页另有“校验并打开 · 只读”；允许控制须独立明确点击。旧隔离缓存可见，未知型号或未核对版本不会自动获得运行权限。

当前准备矩阵只包含 `xiaomi.wifispeaker.l05c` v25 / 包内 1.0.72、`chuangmi.plug.212a01` v31 / 包内 1.0.85，以及精确 SDK 完整摘要。已验证两个包及 SDK 的本机验签、篡改拒绝、运行资源准备与修复，并实际执行 RAM 入口。**真实 Windows 界面渲染仍在修复验证，尚不能声称正式官方插件界面可用。** 最终逐包运行范围以本次交付验证记录为准。

验签根来自用户持有且完整摘要匹配的 APK；没有独立认证 APK 的发行身份，也未查询证书吊销。签名前缀、完整包摘要、Windows ABI 和真实设备功能是不同检查：签名通过不会扩张宿主能力。源码和安装器不含小米闭源插件、SDK 包、根证书或提取图片；这也不是通用 Android 转 Windows 或安全沙箱。当前用法及校验细节见 [PLUGIN_DOWNLOAD.md](PLUGIN_DOWNLOAD.md)。

以下保留 0.4.10 及以前的历史记录，未实现或未执行的结论对应各段标注的版本与时间。

**0.4.10 查询下载更新（2026-10-21）：** 已接入按设备型号查询米家目录和从核准官方 HTTPS CDN 下载，入口在设备详情“信息”页。音箱与插座两个公开型号的真实下载、声明大小和 ZIP CRC 检查通过，包格式均为 RN RAM bundle。本版仅隔离缓存，尚未认证官方签名、自动下载 SDK 或适配该 RAM 格式运行；不会自动执行或放宽旧宿主白名单。详见 [PLUGIN_DOWNLOAD.md](PLUGIN_DOWNLOAD.md)。

以下为 0.4.3 及以前的历史记录；其中“没有自动下载器”等结论对应当时状态。

**0.4.3 集成测试更新：** 可选 Windows 宿主已接入安装器和原生设备信息页。需要用户自行导入精确版本的本机 APK；支持候选限定为 `miot.plugin.spec` 1.1.10 与单路插座。可只读打开，也可由用户显式启用控制；资源名适配与应用目录内运行库已加入。真实设备、官方按钮和最终像素尚未认证，其他格式/厂商插件未开放。下方 0.4.2/静态调查文字均为对应时间的历史记录。

**0.4.2 实验更新（2026-10-20）：** Windows RN 宿主已加载并稳定挂载本机 `miot.plugin.spec` 1.1.10 通用插座样本。旧 Metro、SVG Fabric、safe-area 与部分 SDK ABI 已适配，SDK 测试插座完成读取、修改、独立回读、轮询通知和恢复初值。未验收最终像素、官方按钮、图片资源、真实账号/设备、另外两个 RAM bundle 或第三方厂商专属插件。正式程序未加入 RN 或自动下载器。用法和逐包记录见 [RN 宿主记录](experiments/rnw-host/README.md)。下面保留 2026-10-19 的原始静态核查；其中的未执行结论描述当时状态。

核查日期：2026-10-19。范围是用户提供 APK 的静态分析、公开 SDK 和 Windows 官方文档。没有运行 APK/插件，没有查询账号、下载云端设备插件或控制设备。本次没有修改 HomePanel 运行时代码。

## 结论

可以研究“按型号获取插件、检查兼容性、在设备详情窗打开”的流程，但不能将其承诺为通用的 Android 插件自动转 Windows 功能。下载、插件格式解析、界面运行、米家宿主接口、真实设备通信和分发许可是不同的问题。

保留官方原布局有两条技术路线：为部分 React Native 插件独立实现 Windows 兼容宿主；或者让原版米家在 Android 环境运行，再将画面和输入集成到桌面。前者必须逐项适配，后者仍依赖 Android 环境。初始静态核查时，两条路线均未在本项目验证；最新单包测试范围见上方更新。

对于独立原生 HomePanel，更可控的方案是自动下载本项目维护的型号适配包。它能扩充布局、中文、单位及已验证协议映射，但不是自动转换官方插件，也不能凭型号表生成厂商未公开的功能。

## 本次实际找到的内容

样本为用户提供的 `com.xiaomi.smarthome.apk`，SHA-256：

`73335d3c17dff3e73097db7e2f86a373b7eb2c67611dc82749883ab2da3f0b6e`

没有验证发行方签名真实性。以下仅描述这个文件，不代表全部版本或全部设备。

| 内容 | 静态核查结果 | 意义与边界 |
| --- | --- | --- |
| 中国区设备目录 | 11,549 条设备记录 | 型号/分类等元数据，不是 11,549 份插件，更不是兼容认证 |
| 内置 RN 插件包 | `assets/rn_plugins/` 下 3 个 ZIP | 已复制到工作区本机研究目录并核对 SHA-256；没有执行 |
| 插件基础包 | `assets/plugin_rn_sdk/sdk.bundle` | JavaScript 文本；带有手机宿主调用，不是可独立使用的 Windows SDK |
| React Native 元数据 | 随包 `react-native/package.json` 标记 0.61.0 | 是内置包版本线索，不足以证明全部插件都用同一运行时 |
| 小插件字节码 | 7 个 `.hbc` 文件，Hermes bytecode version 89 | 与 RN RAM bundle 格式不同，需要匹配的执行引擎及宿主 |
| 原生库 | 137 个 `.so`，均在 `arm64-v8a` 下 | 是 Android ARM64 库，不能直接作为 Windows DLL 加载 |
| 宿主引用 | SDK 文本静态匹配到 52 个不同的 `NativeModules.X` 名称 | 包含设备、蓝牙、音箱、摄像头等；不是每个插件都依赖全部模块，匹配也不穷尽动态调用 |
| 插件服务路径线索 | DEX 中存在 `/v2/plugin/fetch_plugin`、`/v2/plugin/fetch_sdk`、`/v2/plugin/get_config_info_new` | 只确认字符串存在；请求参数、响应、授权、验签和当前服务可用性均未验证，未调用 |

三个内置包的格式：

| APK 中的包名 | manifest 中的包标识 | 包版本 | SDK API level | 主 bundle 格式 |
| --- | --- | --- | --- | --- |
| `miot.clock.mijia.zip` | `com.smarthome.spec.scene` | 1.1.8 | 10108 | indexed RAM bundle |
| `miot.switch.spec.zip` | `miot.plugin.spec` | 1.1.10 | 10110 | JavaScript plain bundle |
| `xiaomi.router.rmo15.zip` | `com.xiaomi.routerinitial` | 1.1.9 | 10109 | indexed RAM bundle |

这些名称与标识不能直接证明第三方插座的专属插件已包含其中。`miot.plugin.spec` 是通用包标识，不能据此认定覆盖所有设备。

插件内有签名文件；本次只确认文件存在，未验证签名链。manifest 中 `publicPlugin=1` 等业务字段也不是开源许可证或再分发授权。

## 四种方案会带来什么体验

| 方案 | 用户看到什么 | 是否保留官方插件界面 | 主要代价 |
| --- | --- | --- | --- |
| 本项目型号适配包 | 打开设备时下载适配描述，在原生详情窗显示控件 | 本项目独立实现布局 | 需要维护型号、协议映射与兼容记录；私有能力逐项确认 |
| Windows RN 兼容宿主 | 支持列表内的插件在详情窗运行；不支持的回到基础控制页 | 有条件保留原 JS 布局 | RN/字节码版本、原生控件、资源解析和米家服务桥接均要适配；不能承诺所有包 |
| Android 运行时 + 原版米家 | 由米家登录、下载和运行设备插件，桌面显示其画面 | 最接近原版，有待实测 | Android 系统镜像、ARM 兼容、图形/输入和可能的虚拟化配置；不是把插件转成 Windows |
| 安卓手机画面桥接 | 手机米家运行插件，电脑小窗显示并操作 | 保留手机实际界面 | 依赖已连接并授权的 Android 手机；指定设备页面跳转和小窗集成仍需适配 |

Android Emulator 的硬件加速要求与 CPU/系统镜像架构有关。当前 APK 仅带 ARM64 库，不能假设直接安装到普通 x64 AVD 就能工作。其他运行时的 ARM 翻译、应用服务与设备能力也必须实测。

“只交付一个 EXE”可以作为安装器或自解压启动器的外观，但不能免除运行时文件、后台进程和硬件要求。是否无需用户安装开发环境，与是否没有运行时依赖，是两个不同的交付指标。

## 最小可验证试验

如果要验证原插件兼容，先选择一个有合法使用来源、依赖较少的具体插件；本机内置的 plain bundle 可作为格式研究候选，但不是开源发布素材。不能先宣称支持全部第三方厂商。

1. 核准插件来源与本机试验条件，识别 manifest、资源、签名、bundle 类型和所需宿主 API。保留原包不改写，不以删除签名或绕过校验解决兼容问题。
2. 用自有测试页面验证 Windows 宿主的文字、图片、布局、导航、生命周期及固定尺寸窗口。不能用空壳桥接或模拟状态冒充真实插件已兼容。
3. 隔离运行候选插件，缺失的原生模块或服务明确报“不支持”，不要静默返回成功。账号留在受控宿主内，通过按设备/能力限制的桥接调用，不把会话交给第三方包。
4. 先验证真实属性读取和状态更新。设备写入另按项目规则验证规格、目标、受理及回读；本次没有执行此步骤。
5. 验证布局、中文、滑杆、断网、会话过期、关闭和退出。通过一个型号只代表该型号/插件版本，不能外推全部设备。

自动下载管理器应在已有可运行插件和明确下载授权后接入，包含来源验证、版本兼容检查、可验证的包完整性/签名、失败回退和本地缓存。不应先上线“下载成功”按钮，让用户误以为功能已经适配。

## 本机研究文件与开源边界

工作区相对路径 `work/HomePanel-plugin-research/` 保存独立标准库检查脚本、`plugin-inventory.json` 和 `local-only-bundles/` 中的三份原包。没有提取开发者目录中的密钥字段，也没有读取私人会话。报告只导出格式、版本、计数、哈希和少量公开协议路径线索。

闭源包、图片、字体和字节码留在本机研究目录，不进入源码或安装包。公开 SDK 文档可以帮助理解接口，但不能由“仓库公开”“下载地址可访问”或 SDK 的许可推导出第三方厂商插件的再分发许可。开源发布只包含独立实现以及确有授权的内容。

最初阶段完成了静态清点、三个包的本地提取及哈希校验。后续 Windows RN 单包测试记录见本文开头；自动下载器和 Android 集成尚未实现。

## 公开资料

- [米家官方插件 SDK](https://github.com/MiEcosystem/miot-plugin-sdk)：README 明确面向 React Native 手机插件，调试依赖米家宿主。
- [固定版本的依赖声明](https://github.com/MiEcosystem/miot-plugin-sdk/blob/a48234baf2f3e7faba09c6c7bd18efe7197841bf/package.json)：React Native 0.61.0 及原生扩展依赖，不能视为完整 Windows 宿主。
- [固定版本 Package 接口](https://github.com/MiEcosystem/miot-plugin-sdk/blob/a48234baf2f3e7faba09c6c7bd18efe7197841bf/miot-sdk/Package.js)：插件初始化数据、SDK 版本和生命周期约定；公开接口本身不补齐手机原生实现。
- [Microsoft RN Windows 0.61 Native Modules](https://microsoft.github.io/react-native-windows/v1/docs/0.61/native-modules)：平台能力需要 Windows 原生模块实现并注册。
- [React Native 0.61 indexed RAM bundle 读取器](https://github.com/facebook/react-native/blob/v0.61.0/ReactCommon/cxxreact/JSIndexedRAMBundle.cpp)：用于区分 bundle 格式；未引入其代码。
- [Hermes 字节码文件格式](https://github.com/facebook/hermes/blob/main/include/hermes/BCGen/HBC/BytecodeFileFormat.h)：用于识别 magic 与版本字段；不是插件兼容测试。
- [Android Emulator 加速与架构要求](https://developer.android.com/studio/run/emulator-acceleration)：虚拟化、CPU 和系统镜像限制。
- [scrcpy 项目](https://github.com/Genymobile/scrcpy)：Android 画面和输入桥接的候选基础，未集成或安装；普通控制模式需要手机调试授权。
