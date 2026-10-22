// HomePanelPluginHost.cpp : Defines the entry point for the application.
//

#include "pch.h"
#include "HomePanelPluginHost.h"

#include "AutolinkedNativeModules.g.h"

#include "NativeModules.h"
#include "HomePanelHostModule.h"
#include <shellapi.h>
#pragma comment(lib, "shell32.lib")

// A PackageProvider containing any turbo modules you define within this app project
struct CompReactPackageProvider
    : winrt::implements<CompReactPackageProvider, winrt::Microsoft::ReactNative::IReactPackageProvider> {
 public: // IReactPackageProvider
  void CreatePackage(winrt::Microsoft::ReactNative::IReactPackageBuilder const &packageBuilder) noexcept {
    AddAttributedModules(packageBuilder, true);
  }
};

// The entry point of the Win32 application
_Use_decl_annotations_ int CALLBACK WinMain(HINSTANCE, HINSTANCE, PSTR commandLine, int) {
  // Initialize WinRT
  winrt::init_apartment(winrt::apartment_type::single_threaded);

  // Enable per monitor DPI scaling
  SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

  // Find the path hosting the app exe file
  WCHAR appDirectory[MAX_PATH];
  GetModuleFileNameW(NULL, appDirectory, MAX_PATH);
  PathCchRemoveFileSpec(appDirectory, MAX_PATH);
  HomePanelSmoke::officialPluginTest = strcmp(commandLine, "--official-plugin-self-test") == 0;
  HomePanelSmoke::brokerSelfTest = strcmp(commandLine, "--broker-self-test") == 0;
  int argumentCount=0;
  auto arguments=CommandLineToArgvW(GetCommandLineW(), &argumentCount);
  if (arguments && argumentCount==3 && wcscmp(arguments[1], L"--official-plugin")==0) {
    HomePanelSmoke::officialPluginRuntime=true;
    HomePanelSmoke::pluginDirectory=std::filesystem::path(arguments[2]);
  }
  if (arguments) LocalFree(arguments);
  HomePanelSmoke::brokerEnabled = HomePanelSmoke::officialPluginRuntime || HomePanelSmoke::officialPluginTest || HomePanelSmoke::brokerSelfTest || strcmp(commandLine, "--broker") == 0;
  HomePanelSmoke::enabled = HomePanelSmoke::officialPluginTest || HomePanelSmoke::brokerSelfTest || strcmp(commandLine, "--self-test") == 0;
  HomePanelSmoke::report = std::filesystem::path(appDirectory) / "host-self-test.json";

  // Create a ReactNativeWin32App with the ReactNativeAppBuilder
  auto reactNativeWin32App{winrt::Microsoft::ReactNative::ReactNativeAppBuilder().Build()};

  // Configure the initial InstanceSettings for the app's ReactNativeHost
  auto settings{reactNativeWin32App.ReactNativeHost().InstanceSettings()};
  // Register any autolinked native modules
  RegisterAutolinkedNativeModulePackages(settings.PackageProviders());
  // Register any native modules defined within this app project
  settings.PackageProviders().Append(winrt::make<CompReactPackageProvider>());

#if BUNDLE
  // Load the JS bundle from a file (not Metro):
  // Set the path (on disk) where the .bundle file is located
  settings.BundleRootPath(std::wstring(L"file://").append(appDirectory).append(L"\\Bundle\\").c_str());
  // Set the name of the bundle file (without the .bundle extension)
  settings.JavaScriptBundleFile(L"index.windows");
  // Disable hot reload
  settings.UseFastRefresh(false);
#else
  // Load the JS bundle from Metro
  settings.JavaScriptBundleFile(L"index");
  // Enable hot reload
  settings.UseFastRefresh(true);
#endif
#if _DEBUG
  // For Debug builds
  // Enable Direct Debugging of JS
  settings.UseDirectDebugger(true);
  // Enable the Developer Menu
  settings.UseDeveloperSupport(true);
#else
  // For Release builds:
  // Disable Direct Debugging of JS
  settings.UseDirectDebugger(false);
  // Disable the Developer Menu
  settings.UseDeveloperSupport(false);
#endif

  // Get the AppWindow so we can configure its initial title and size
  auto appWindow{reactNativeWin32App.AppWindow()};
  appWindow.Title(HomePanelSmoke::officialPluginRuntime ? L"HomePanel · 官方设备插件" : L"HomePanel · 宿主自检");
  appWindow.Resize({388, 540});
  HomePanelSmoke::window = winrt::Microsoft::UI::GetWindowFromWindowId(appWindow.Id());
  WCHAR parentId[32]{};
  if (HomePanelSmoke::officialPluginRuntime) {
    if (!GetEnvironmentVariableW(L"HOMEPANEL_PARENT_PID",parentId,32)) return 3;
    HomePanelSmoke::parentProcess=OpenProcess(SYNCHRONIZE,FALSE,wcstoul(parentId,nullptr,10));
    if (!HomePanelSmoke::parentProcess) return 3;
    SetTimer(nullptr,0,500,HomePanelSmoke::CheckParent);
  }
  if (HomePanelSmoke::enabled) {
    HomePanelSmoke::WriteReport();
    SetTimer(nullptr, 0, 100, HomePanelSmoke::Check);
  }

  // Get the ReactViewOptions so we can set the initial RN component to load
  auto viewOptions{reactNativeWin32App.ReactViewOptions()};
  viewOptions.ComponentName(L"HomePanelPluginHost");

  // Start the app
  reactNativeWin32App.Start();
  if (HomePanelSmoke::parentProcess) CloseHandle(HomePanelSmoke::parentProcess);
  return HomePanelSmoke::enabled && !HomePanelSmoke::passed ? 2 : 0;
}
