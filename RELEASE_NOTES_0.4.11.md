# HomePanel 0.4.11

HomePanel 是一个面向 Windows 的原生米家设备控制器。它常驻系统托盘，支持家庭和房间筛选、设备状态、开关控制、详情设置，以及受边界约束的 Xiaomi 智能音箱状态和播放控制。

## 本版内容

- Rust + `eframe/egui` 原生 Windows 托盘与设备面板。
- 设备状态优先直读，失败时保留“待确认/缓存”标识，不把旧数据显示成实时状态。
- 米家家庭、房间、搜索、收藏、隐藏和详情设置。
- 音箱播放状态、歌曲、音量、循环和进度读取，以及经过身份、能力和回读校验的受限控制。
- 插件下载记录、包摘要、证书链和运行文件检查；插件宿主按需启动并与账号会话隔离。
- Windows x64 安装器，支持 SHA-256 校验文件。

## 安装

下载 GitHub Release 中的 `HomePanel-Setup-0.4.11.exe`，运行后从开始菜单启动。首次启动需要使用米家扫码登录。安装器会检查同一目录中的旧版 HomePanel；账号会话和个人设置留在当前 Windows 用户目录中。

如需校验下载文件，在 PowerShell 中运行：

```powershell
Get-FileHash .\HomePanel-Setup-0.4.11.exe -Algorithm SHA256
```

将输出的摘要与同名 `.sha256` 文件比较。

## 当前边界

HomePanel 不是小米官方客户端，也不承诺全部米家设备或官方插件兼容。官方插件、SDK、APK、用户导入的验签根、图片和账号数据不会进入源码仓库或安装包。插件下载成功不等于官方界面已经兼容；实际能力取决于公开 MIoT SPEC、设备型号和服务区。

项目的完整范围、隐私说明、协议来源和第三方许可见 [README.md](README.md)、[PLUGIN_DOWNLOAD.md](PLUGIN_DOWNLOAD.md)、[PLUGIN_COMPATIBILITY.md](PLUGIN_COMPATIBILITY.md)、[MIJIA_API_COVERAGE.md](MIJIA_API_COVERAGE.md) 和 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

## 验证

- `cargo test --locked --manifest-path src-tauri/Cargo.toml`: 191 passed, 2 ignored, 0 failed。
- `cargo build --locked --release --manifest-path src-tauri/Cargo.toml`: passed。
- Windows x64 安装器已生成，附件中的校验文件对应实际安装器。
