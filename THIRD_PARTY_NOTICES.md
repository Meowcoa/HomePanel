# Third-party notices

HomePanel's original application code is MIT licensed. Version 0.4.9 independently corrects scoped XiaoAI protocol context, unifies native playback controls and separates status freshness; no extra runtime dependency is added. Device artwork and Xiaomi trademarks are not covered by this license. No Xiaomi device artwork is bundled.

## Protocol references adapted into Rust

- **daviddwlee84/mihome-ctl** — Copyright (c) 2026 Da-Wei Lee. MIT. <https://github.com/daviddwlee84/mihome-ctl>
  The QR login flow, encrypted cloud request construction, home enumeration, and MIoT operation payloads informed `cloud.rs` and `crypto.rs`.
- That connector is itself derived from **PiotrMachowski/Xiaomi-cloud-tokens-extractor**, Copyright (c) 2020 Piotr Machowski. MIT. <https://github.com/PiotrMachowski/Xiaomi-cloud-tokens-extractor>
- **Yonsm/MiService** — Copyright (c) 2021–2026 Yonsm. MIT. <https://github.com/Yonsm/MiService>
  MIoT get/set/action request shapes and the STS clientSign handshake informed `cloud.rs`. The 0.4.5 login fix independently implements the nonce/ssecurity SHA-1 protocol fact; no upstream Python runtime is bundled.

Copies of the upstream MIT licenses are in `LICENSES/`. Exact reference revisions are recorded in `references.json`. No Python code or runtime is bundled.

## Public device metadata and measurement facts

Chinese labels are fetched at runtime from the public MIoT `instance/v2/multiLanguage` endpoint using the credential-free client, with a local cache and original offline fallbacks. No Xiaomi SDK is linked or distributed.

The exact `chuangmi.plug.212a01` SPEC family (versions 1–3) and its public power comment document 0.01 W reporting. Public device reports provide the missing mA/V units and identify energy as mWh. HomePanel independently formats these hardware measurement facts for display only; it does not import code or configuration from the restricted Xiaomi Home SDK. Sources and limitations are recorded in `references.json` and the README. Hardware verification remains outstanding.

## Android APK behavior research (0.3.5)

The user-provided `com.xiaomi.smarthome` APK, version 11.7.709 (versionCode 110717091), was statically inspected for protocol interoperability. Its publisher signature was not authenticated. An observed nested property-set result code of 0 or 1 is treated as acceptance only; HomePanel independently matches the target and requires fresh readback before confirming state. The public MiEcosystem plugin API documentation describes datasource 2 as direct device RPC. This documentation informed request semantics only; no SDK implementation is used.

No APK executable code, proprietary SDK, device plugins, fonts, artwork, extracted configuration or complete model database is included in the source or installer. Device categories use public MIoT standard type facts; the new switch, speaker and mosquito-dispeller artwork is original vector code. APK research does not grant redistribution or Xiaomi service rights. Source identifiers are in `references.json`; the findings and unverified areas are in `APK_ANALYSIS.md`.

## Visual references and system fonts

MiMonitorLightTray's public documentation informed tray usability only. MiWuX was viewed as a layout reference only. Folia's Bravais guide informed the content wall and focus treatment. No code or artwork from these projects is redistributed. The generated concept image is a design reference, not a runtime asset.

Apple's public Liquid Glass guidance informed the 0.3.1 material hierarchy and simple controls. No Apple framework, code, symbols, fonts or artwork is bundled. Desktop blur uses Windows DWM acrylic, not Apple's Liquid Glass renderer.

HomePanel 0.3.10 embeds the unmodified Noto Sans CJK SC Regular font from the notofonts/noto-cjk project under SIL Open Font License 1.1. This font provides Chinese glyphs without installing fonts or loading Windows font files. The complete license is in LICENSES/NotoSansCJK-OFL.txt, is embedded in the executable, and can be read in Settings or with --font-license. No Apple or Microsoft font is bundled. Font copyright and reserved names remain governed by the font license, separately from HomePanel's MIT code.

## Runtime and build dependencies

The current Windows executable is built from the Rust dependencies pinned in `src-tauri/Cargo.lock`; direct package versions and declared SPDX licenses are listed in `LICENSES/DEPENDENCY_LICENSES.md`. Transitive dependencies retain their own upstream license terms.

The native executable does not embed React, Tauri, WebView2, or a browser runtime. Windows APIs for the tray, DWM backdrop, DPAPI, and registry access are provided by Windows. NSIS is a build-time packaging tool and is not included in the installed app.

HomePanel uses zip 4.6.1 (MIT) to inspect local archives and import an exact user-supplied candidate. Execution is delegated to the optional host; no Xiaomi source is distributed. Its complete license is in LICENSES/zip-MIT.txt and the native Settings license section. Data-only panel definitions under adapters/ are original HomePanel MIT content; they do not include proprietary vendor plugins or expand MIoT permissions. The optional host under experiments/rnw-host/ is built separately and can be included in the 0.4.3 integrated installer. It is not embedded into the native Rust executable and starts only when requested. No extracted plugin ZIP, Xiaomi SDK bundle, APK model database or research dump is included in source releases.

This software is an independent community client, not affiliated with Xiaomi. Open-source code licenses do not grant access to Xiaomi services or rights to Xiaomi trademarks and device images. Community cloud interfaces may change independently of this application.

The 0.4.2 RN development experiment retains generated C++ project/template files under experiments/rnw-host/windows from react-native-windows 0.79.5 (Microsoft and contributors, MIT); LICENSE-RNW.txt is retained. HomePanelHostModule.h, deviceBridge.js and the scoped Rust broker are original. Public MiEcosystem service/spec.js documentation informed interface semantics only; no Xiaomi implementation was copied. Microsoft build tools and NuGet archives are not included in source archives or installers. The integrated installer distributes only its staged app-local runtime files, their exact manifest and third-party licenses; it does not distribute development tools.

The local RN experiment additionally pins react-native-svg 15.12.1, Copyright (c) [2015-2016] [Horcrux], MIT; its complete license is retained in experiments/rnw-host/LICENSE-SVG.txt. Source: https://github.com/software-mansion/react-native-svg/tree/v15.12.1 . prepareSvgFabric.cjs temporarily adjusts that version's Windows build configuration and restores its exact upstream files after building. windowsSvg.js and windowsSafeArea.js are independent HomePanel compatibility adapters. React Native Codegen generates SVG view configurations from the installed MIT package. Local execution of one user-supplied Xiaomi plain bundle is an interoperability experiment only; no Xiaomi executable source, plugin assets or runtime payload is copied into this repository or the product installer.

## Optional integrated Windows runtime (0.4.3)

The staged installer includes Microsoft Windows App SDK 1.7.250401001, Microsoft React Native Windows 0.79.5, Hermes and app-local Microsoft VC++ redistributables. Microsoft binaries retain their own terms and are not relicensed under HomePanel MIT. The installer presents the Windows App SDK and VC runtime terms. Per-component license/notice files and the production Metro package license inventory are installed under PluginHost/LICENSES. The VC installer itself is not executed or redistributed; its Microsoft signature is verified before archive extraction. SDK source and build tools are excluded. The runtime-manifest.json records every staged file. User-imported Xiaomi JS and raster images stay local and are not part of this distribution.

## XiaoAI protocol facts (0.4.4)

The user-supplied Xiaomi Speaker 3.0.0 APK was analyzed in a separate task. Only endpoint names, declared fields, units and observed enum constants informed the independently authored Rust speaker_service.rs and native UI. No decompiled implementation, APK assets, proprietary signing key, account session or private device data is distributed. Runtime micoapi authorization remains separate from xiaomiio, with DPAPI and exact device matching. Since 0.4.8 selected private controls have separate verified protocol contracts, while the public SPEC is still checked and never modified to invent permissions. Runtime hardware compatibility remains unverified. See SPEAKER_SERVICE.md for the exact scope.


## XiaoAI response schema correction (0.4.6)

DeviceListResponseV3 declares deviceList and groupInfo; MinaResponse/BaseResponse declare data/result and code/status aliases. These protocol facts were rechecked against classes3.dex in the exact previously documented APK, and independently implemented in Rust with synthetic HTTP fixtures. No phone implementation, APK, real response data or account credentials are included.


## Model-based official plugin retrieval (0.4.10)

The plugin directory request, response fields and ZIP/signature distinctions are independently implemented protocol facts from the user-supplied Mi Home 11.7.709 APK. The older endpoint request is also documented by the al-one/hass-xiaomi-miot project README; no code from it is copied. No additional runtime dependency is added. The exact iot-cdn.io.mi.com origin was verified from the authenticated directory response and HTTPS transport, not from an assumed APK hardcoded allowlist. Downloads stay in the local user quarantine cache; source archives and installers contain no downloaded plugins, signed URLs, certificates or account data. Local size/CRC/SHA256 checks do not constitute publisher signature verification. See PLUGIN_DOWNLOAD.md and references.json.

## Independent downloaded-plugin host and local trust (0.4.11)

The model-based plugin/SDK queries, indexed RAM bundle loader, phone SDK compatibility adapter, selected-device capability/read/write/action protocol, local download catalog and preparation checks are independently authored HomePanel implementations. Public protocol semantics and the exact user-provided APK/package samples informed interoperability only; no decompiled Xiaomi implementation, restricted SDK source, account credential, proprietary signing key or model database is redistributed. Windows CryptoAPI provides ECDSA signature, certificate-chain and validity checks without installing a certificate into the Windows trust store.

The user must import the public root from the exact, full-SHA-256-pinned APK already documented above. This verifies a chain to that locally imported APK root; HomePanel has not independently authenticated that APK's publisher identity and does not perform certificate revocation checking. A successful signature may cover only the declared bundle prefix. Exact full-archive SHA-256 pins and launch-time reconstruction/checking of all expected runtime files separately bind the remaining bundle bytes, runtime manifest and raster resources. A rewritten local inventory does not grant trust. These checks and the separate process are not a general-purpose security sandbox or a grant of Android/iOS host privileges.

The compatibility matrix remains limited to exact model/package/SDK versions and hashes. Two downloaded packages and their SDK have passed local Windows cryptographic/tamper checks, preparation/recovery tests and RAM entry execution. Real Windows rendering and original device-control behavior are still undergoing validation; this notice does not certify an operational official UI or universal Xiaomi plugin compatibility. The bridge defaults to read-only, keeps account data in Rust, binds operations to the original account/device lease, validates property writes against public SPEC and requires direct readback. Supported zero-input/zero-output daily actions report acceptance separately from confirmed state; notifications are polling, not Xiaomi cloud push. Unsupported mobile/private services remain unsupported.

Downloaded Xiaomi plugins, SDK archives, extracted public root certificates and raster artwork remain only in the user's local data directory. None are included in GitHub source releases, source ZIPs or the HomePanel installer. Local import/download and signature verification do not grant redistribution, trademark or service authorization. No additional runtime dependency was added for this protocol or verifier; the existing optional Microsoft runtime retains its separate terms. See PLUGIN_DOWNLOAD.md and PLUGIN_COMPATIBILITY.md for current scope and verification limits.
