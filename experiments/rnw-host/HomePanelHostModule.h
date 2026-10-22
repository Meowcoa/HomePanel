#pragma once
// Original smoke test and scoped broker transport. Account credentials stay in Rust.
#include "NativeModules.h"
#include <atomic>
#include <filesystem>
#include <fstream>
#include <mutex>
#include <winrt/Windows.Networking.Connectivity.h>

namespace HomePanelSmoke {
inline std::atomic<unsigned> stages{0};
inline bool enabled = false;
inline HWND window = nullptr;
inline std::filesystem::path report;
inline unsigned ticks = 0;
inline unsigned completeTicks = 0;
inline constexpr unsigned pluginStabilityTicks = 200; // 20 seconds of fixture rendering.
inline bool passed = false;
inline std::atomic<bool> brokerEnabled{false};
inline bool brokerSelfTest = false;
inline bool officialPluginTest = false;
inline bool officialPluginRuntime = false;
inline std::filesystem::path pluginDirectory;
inline HANDLE parentProcess = nullptr;
inline void CALLBACK CheckParent(HWND, UINT, UINT_PTR timer, DWORD) noexcept {
  if (parentProcess && WaitForSingleObject(parentProcess, 0) == WAIT_OBJECT_0) {
    KillTimer(nullptr, timer);
    if (window) PostMessageW(window, WM_CLOSE, 0, 0);
  }
}
inline std::atomic<bool> pluginFailed{false};
inline std::mutex brokerMutex;

inline void WriteReport() noexcept {
  if (!enabled) return;
  try {
    std::ofstream out(report, std::ios::trunc);
    auto current = stages.load();
    out << "{\"protocol\":1,\"selfTest\":true,\"nativeWindow\":"
        << (window && IsWindow(window) ? "true" : "false")
        << ",\"nativeModule\":" << ((current & 1) ? "true" : "false")
        << ",\"promiseRoundTrip\":" << ((current & 2) ? "true" : "false")
        << ",\"nativeViewLayout\":" << ((current & 4) ? "true" : "false")
        << ",\"nativeEventRoundTrip\":" << ((current & 8) ? "true" : "false")
        << ",\"brokerRoundTrip\":" << ((current & 16) ? "true" : "false")
        << ",\"passed\":" << (passed && !pluginFailed ? "true" : "false")
        << ",\"officialPluginStable\":" << (officialPluginTest && completeTicks >= pluginStabilityTicks ? "true" : "false")
        << ",\"realDeviceAccess\":false,\"officialPluginLoaded\":" << ((current & 32) ? "true" : "false")
        << ",\"officialPluginMounted\":" << ((current & 64) ? "true" : "false")
        << ",\"officialPluginFailed\":" << (pluginFailed ? "true" : "false") << "}\n";
  } catch (...) { /* The test runner treats an absent report as failure. */ }
}

inline void CALLBACK Check(HWND, UINT, UINT_PTR timer, DWORD) noexcept {
  ++ticks;
  auto expected = officialPluginTest ? 127u : brokerSelfTest ? 31u : 15u;
  const bool complete = stages.load() == expected;
  completeTicks = complete ? completeTicks + 1 : 0;
  const bool settled = !officialPluginTest || completeTicks >= pluginStabilityTicks;
  if ((complete && settled) || ticks >= (officialPluginTest ? 900u : 150u) || pluginFailed) {
    passed = !pluginFailed && complete && settled && window && IsWindow(window);
    WriteReport();
    KillTimer(nullptr, timer);
    if (window) PostMessageW(window, WM_CLOSE, 0, 0);
  }
}
}

REACT_MODULE(HomePanelHost)
struct HomePanelHost {
  REACT_METHOD(getNetworkStatus)
  void getNetworkStatus(winrt::Microsoft::ReactNative::ReactPromise<std::string> const &promise) noexcept {
    try {
      using namespace winrt::Windows::Networking::Connectivity;
      auto profile = NetworkInformation::GetInternetConnectionProfile();
      auto level = profile ? profile.GetNetworkConnectivityLevel() : NetworkConnectivityLevel::None;
      std::string type = !profile ? "none" : profile.IsWlanConnectionProfile() ? "wifi" : profile.IsWwanConnectionProfile() ? "cellular" : "ethernet";
      std::string reply = "{\"type\":\"" + type + "\",\"isConnected\":";
      reply += level != NetworkConnectivityLevel::None ? "true" : "false";
      reply += ",\"isInternetReachable\":";
      reply += level == NetworkConnectivityLevel::InternetAccess ? "true" : "false";
      reply += ",\"details\":null}";
      promise.Resolve(reply);
    } catch (...) {promise.Reject("NETWORK_STATUS_UNAVAILABLE");}
  }
  REACT_METHOD(getCapabilities)
  void getCapabilities(winrt::Microsoft::ReactNative::ReactPromise<std::string> const &promise) noexcept {
    HomePanelSmoke::stages.fetch_or(1);
    std::string result = "{\"protocol\":1,\"rnWindows\":\"0.79.5\",\"officialPluginCompatible\":false,\"deviceBrokerConnected\":";
    result += HomePanelSmoke::brokerEnabled ? "true" : "false";
    result += ",\"brokerSelfTest\":";
    result += HomePanelSmoke::brokerSelfTest ? "true" : "false";
    result += ",\"localPluginTest\":";
    result += HomePanelSmoke::officialPluginTest ? "true" : "false";
    result += ",\"localPluginRuntime\":";
    result += HomePanelSmoke::officialPluginRuntime ? "true}" : "false}";
    promise.Resolve(result);
  }

  REACT_EVENT(onHostDiagnostic)
  std::function<void(std::string)> onHostDiagnostic;

  REACT_METHOD(requestDiagnosticEvent)
  void requestDiagnosticEvent() noexcept {
    if (onHostDiagnostic) onHostDiagnostic("homepanel-host-test-v1");
  }

  REACT_METHOD(reportStage)
  void reportStage(std::string const &stage) noexcept {
    if (stage == "promise") HomePanelSmoke::stages.fetch_or(2);
    else if (stage == "layout") HomePanelSmoke::stages.fetch_or(4);
    else if (stage == "event") HomePanelSmoke::stages.fetch_or(8);
    else if (stage == "broker" && (HomePanelSmoke::brokerSelfTest || HomePanelSmoke::officialPluginTest)) HomePanelSmoke::stages.fetch_or(16);
    else if (stage == "official-entry" && HomePanelSmoke::officialPluginTest) HomePanelSmoke::stages.fetch_or(32);
    else if (stage == "official-mounted" && HomePanelSmoke::officialPluginTest) HomePanelSmoke::stages.fetch_or(64);
    else if (stage == "official-error" && HomePanelSmoke::officialPluginTest) {
      HomePanelSmoke::pluginFailed = true;
    }
  }

  REACT_METHOD(getLocalPluginSource)
  void getLocalPluginSource(int part, winrt::Microsoft::ReactNative::ReactPromise<std::string> const &promise) noexcept {
    if ((!HomePanelSmoke::officialPluginTest && !HomePanelSmoke::officialPluginRuntime) || (part != 0 && part != 1)) {promise.Reject("LOCAL_PLUGIN_LAB_DISABLED"); return;}
    try {
      auto path = (HomePanelSmoke::officialPluginRuntime ? HomePanelSmoke::pluginDirectory : HomePanelSmoke::report.parent_path() / "LocalPlugin") / (part == 0 ? "sdk.bundle" : "switch.bundle");
      if (std::filesystem::file_size(path) > 60 * 1024 * 1024) {promise.Reject("LOCAL_PLUGIN_SOURCE_TOO_LARGE"); return;}
      std::ifstream input(path, std::ios::binary);
      std::string source((std::istreambuf_iterator<char>(input)), {});
      if (!input || source.empty()) {promise.Reject("LOCAL_PLUGIN_SOURCE_UNREADABLE"); return;}
      promise.Resolve(source);
    } catch (...) {promise.Reject("LOCAL_PLUGIN_SOURCE_UNAVAILABLE");}
  }

  // Binary RAM bundles cannot pass through a UTF-8 string without corruption.
  // Only fixed package filenames are accepted; JS never supplies filesystem paths.
  REACT_METHOD(getLocalPluginBytes)
  void getLocalPluginBytes(int part, winrt::Microsoft::ReactNative::ReactPromise<std::string> const &promise) noexcept {
    if ((!HomePanelSmoke::officialPluginTest && !HomePanelSmoke::officialPluginRuntime) || (part != 0 && part != 1)) {promise.Reject("LOCAL_PLUGIN_LAB_DISABLED"); return;}
    try {
      auto path = (HomePanelSmoke::officialPluginRuntime ? HomePanelSmoke::pluginDirectory : HomePanelSmoke::report.parent_path() / "LocalPlugin") / (part == 0 ? "sdk.bundle" : "switch.bundle");
      if (std::filesystem::file_size(path) > 60 * 1024 * 1024) {promise.Reject("LOCAL_PLUGIN_SOURCE_TOO_LARGE"); return;}
      std::ifstream input(path, std::ios::binary);
      std::string bytes((std::istreambuf_iterator<char>(input)), {});
      if (!input || bytes.empty() || bytes.size() > 60 * 1024 * 1024) {promise.Reject("LOCAL_PLUGIN_SOURCE_UNREADABLE"); return;}
      static char const alphabet[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
      std::string encoded;
      encoded.reserve(((bytes.size() + 2) / 3) * 4);
      for (size_t i = 0; i < bytes.size(); i += 3) {
        unsigned word = static_cast<unsigned char>(bytes[i]) << 16;
        if (i + 1 < bytes.size()) word |= static_cast<unsigned char>(bytes[i + 1]) << 8;
        if (i + 2 < bytes.size()) word |= static_cast<unsigned char>(bytes[i + 2]);
        encoded.push_back(alphabet[(word >> 18) & 63]);
        encoded.push_back(alphabet[(word >> 12) & 63]);
        encoded.push_back(i + 1 < bytes.size() ? alphabet[(word >> 6) & 63] : '=');
        encoded.push_back(i + 2 < bytes.size() ? alphabet[word & 63] : '=');
      }
      promise.Resolve(encoded);
    } catch (...) {promise.Reject("LOCAL_PLUGIN_SOURCE_UNAVAILABLE");}
  }
  REACT_METHOD(getLocalPluginManifest)
  void getLocalPluginManifest(winrt::Microsoft::ReactNative::ReactPromise<std::string> const &promise) noexcept {
    if (!HomePanelSmoke::officialPluginTest && !HomePanelSmoke::officialPluginRuntime) {promise.Reject("LOCAL_PLUGIN_LAB_DISABLED");return;}
    try {
      auto path = (HomePanelSmoke::officialPluginRuntime ? HomePanelSmoke::pluginDirectory : HomePanelSmoke::report.parent_path() / "LocalPlugin") / "runtime-manifest.json";
      if (!std::filesystem::exists(path)) {promise.Resolve("{}");return;}
      if (std::filesystem::file_size(path) > 16 * 1024) {promise.Reject("PLUGIN_MANIFEST_TOO_LARGE");return;}
      std::ifstream input(path, std::ios::binary);
      std::string data((std::istreambuf_iterator<char>(input)), {});
      if (!input || data.empty()) {promise.Reject("PLUGIN_MANIFEST_UNREADABLE");return;}
      promise.Resolve(data);
    } catch (...) {promise.Reject("PLUGIN_MANIFEST_UNAVAILABLE");}
  }

  REACT_METHOD(getLocalPluginAssets)
  void getLocalPluginAssets(winrt::Microsoft::ReactNative::ReactPromise<std::string> const &promise) noexcept {
    if (!HomePanelSmoke::officialPluginRuntime && !HomePanelSmoke::officialPluginTest) {promise.Resolve("{}");return;}
    try {
      auto path=(HomePanelSmoke::officialPluginRuntime ? HomePanelSmoke::pluginDirectory : HomePanelSmoke::report.parent_path() / "LocalPlugin") / "local-assets.json";
      if (!std::filesystem::exists(path) && HomePanelSmoke::officialPluginTest) {promise.Resolve("{}");return;}
      if (std::filesystem::file_size(path)>256*1024) {promise.Reject("ASSET_MAP_TOO_LARGE");return;}
      std::ifstream input(path,std::ios::binary);
      std::string data((std::istreambuf_iterator<char>(input)),{});
      if (!input) {promise.Reject("ASSET_MAP_UNAVAILABLE");return;}
      promise.Resolve(data);
    } catch (...) {promise.Reject("ASSET_MAP_UNAVAILABLE");}
  }
  REACT_METHOD(getLocalPluginAssetRoot)
  void getLocalPluginAssetRoot(winrt::Microsoft::ReactNative::ReactPromise<std::string> const &promise) noexcept {
    if (!HomePanelSmoke::officialPluginRuntime && !HomePanelSmoke::officialPluginTest) {promise.Resolve("");return;}
    auto directory=HomePanelSmoke::officialPluginRuntime ? HomePanelSmoke::pluginDirectory : HomePanelSmoke::report.parent_path() / "LocalPlugin";
    promise.Resolve(winrt::to_string((directory / "images").generic_wstring()));
  }

  REACT_METHOD(closeWindow)
  void closeWindow() noexcept {if (HomePanelSmoke::window) PostMessageW(HomePanelSmoke::window, WM_CLOSE, 0, 0);}

  REACT_METHOD(reportPluginDiagnostic)
  void reportPluginDiagnostic(std::string const &detail, bool failed) noexcept {
    if (!HomePanelSmoke::officialPluginTest || detail.size() > 8192) return;
    if (HomePanelSmoke::pluginFailed && !failed) return;
    if (failed) HomePanelSmoke::pluginFailed = true;
    try {
      std::ofstream out(HomePanelSmoke::report.parent_path() / "official-plugin-diagnostic.json", std::ios::trunc);
      out << detail;
    } catch (...) {}

  }
};

// Uses inherited anonymous pipes only. No TCP listener, paths, token or arbitrary RPC.
REACT_MODULE(HomePanelBroker)
struct HomePanelBroker {
  REACT_METHOD(request)
  void request(std::string const &frame, winrt::Microsoft::ReactNative::ReactPromise<std::string> const &promise) noexcept {
    if (!HomePanelSmoke::brokerEnabled) { promise.Reject("BROKER_NOT_CONNECTED"); return; }
    if (frame.size() > 60 * 1024 || frame.find_first_of("\r\n") != std::string::npos) {
      promise.Reject("INVALID_FRAME"); return;
    }
    std::lock_guard<std::mutex> guard(HomePanelSmoke::brokerMutex);
    auto input = GetStdHandle(STD_INPUT_HANDLE);
    auto output = GetStdHandle(STD_OUTPUT_HANDLE);
    auto line = frame + "\n";
    DWORD written = 0;
    if (!WriteFile(output, line.data(), static_cast<DWORD>(line.size()), &written, nullptr) || written != line.size()) {
      promise.Reject("BROKER_WRITE_FAILED"); return;
    }
    std::string reply;
    auto deadline = GetTickCount64() + 20000;
    while (reply.size() < 64 * 1024 && GetTickCount64() < deadline) {
      DWORD available = 0;
      if (!PeekNamedPipe(input, nullptr, 0, nullptr, &available, nullptr)) { promise.Reject("BROKER_DISCONNECTED"); return; }
      if (!available) { Sleep(10); continue; }
      char byte = 0;
      DWORD received = 0;
      if (!ReadFile(input, &byte, 1, &received, nullptr) || received != 1) { promise.Reject("BROKER_READ_FAILED"); return; }
      if (byte == '\n') { promise.Resolve(reply); return; }
      reply.push_back(byte);
    }
    // Close the transport after a timeout: late replies must never match a new request.
    HomePanelSmoke::brokerEnabled = false;
    promise.Reject("BROKER_TIMEOUT_OR_OVERSIZE");
  }
};
