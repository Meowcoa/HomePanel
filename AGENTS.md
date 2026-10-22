# HomePanel development

- The shipped app is a native Rust Windows program under `src-tauri/`; its UI uses `eframe/egui`. React, Tauri, Vite, npm, and WebView2 files are legacy prototype material and are not part of the native build.
- Run `cargo test --locked --manifest-path src-tauri/Cargo.toml` and `cargo build --locked --release --manifest-path src-tauri/Cargo.toml` for relevant Rust changes. Package with `scripts/Build-Installer.ps1` and NSIS 3.13.
- Keep account credentials entirely in Rust; use DPAPI on Windows. Never log, print, export, screenshot into reports, or commit sessions, private account/device data, cookies, or QR login payloads.
- Real device writes must validate the MIoT SPEC and read back results. Do not invent supported capabilities or mark command acceptance as confirmed state. Never issue a cloud device-removal request unless its exact interface and behavior are verified.
- Demo data is allowed only in Debug builds through `--demo` or `--demo-detail`; it must not enter Release behavior. The preview must remain clearly labeled and must not access or change real account/device state.
- Preserve the approved native tray/card/detail design, system CJK font fallback, reduced-motion behavior, and Windows backdrop handling. Verify Windows window, DPI, tray, and focus behavior separately from static UI inspection.
- Use original assets or documented authorized sources. Do not copy code/art from MiWuX or restricted Xiaomi HA/Miloco SDKs. Do not bundle unlicensed Xiaomi images or Windows system fonts.
- Keep dependency licenses and protocol source records current. Do not publish to an external repository without the user's release instruction.
- Place scratch scripts, snapshots, traces, toolchains, release outputs, and build logs in the workspace `work/` directory outside this source tree.
