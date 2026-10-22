Unicode true
!include "${NSISDIR}\Include\MUI2.nsh"
!ifdef HOST_INCLUDE
!include "${HOST_INCLUDE}"
!endif

!ifndef VERSION
!define VERSION "0.3.7"
!endif
!ifndef VERSION_NUM
!define VERSION_NUM "0.3.7.0"
!endif
!ifndef OUTPUT_DIR
!error "Pass OUTPUT_DIR to makensis."
!endif
!ifndef RELEASE_EXE
!error "Pass RELEASE_EXE to makensis."
!endif
!ifndef SOURCE_ROOT
!error "Pass SOURCE_ROOT to makensis."
!endif
!ifdef TEST_MODE
!define TEST_SUFFIX "-test"
!ifndef TEST_START_MENU
!error "Pass TEST_START_MENU to makensis in test mode."
!endif
!define APP_REG_KEY "Software\HomePanelInstallerTest\Install"
!define UNINSTALL_REG_KEY "Software\HomePanelInstallerTest\Uninstall\HomePanel"
!define START_MENU_DIR "${TEST_START_MENU}"
!else
!define TEST_SUFFIX ""
!define APP_REG_KEY "Software\HomePanel"
!define UNINSTALL_REG_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\HomePanel"
!define START_MENU_DIR "$SMPROGRAMS\HomePanel"
!endif

Name "HomePanel"
Caption "HomePanel 桌面安装程序"
OutFile "${OUTPUT_DIR}\HomePanel-Setup-${VERSION}${TEST_SUFFIX}.exe"
InstallDir "$LOCALAPPDATA\Programs\HomePanel"
InstallDirRegKey HKCU "${APP_REG_KEY}" "InstallDir"
RequestExecutionLevel user
SetCompressor /FINAL lzma
CRCCheck force
; A skipped executable must never produce a successful-looking upgrade.
AllowSkipFiles off
FileErrorText "无法更新文件：$\r$\n$0$\r$\n$\r$\n请退出 HomePanel 并检查安装目录权限，然后点重试；或取消本次安装。" "无法更新文件：$\r$\n$0$\r$\n$\r$\n请退出 HomePanel 并检查安装目录权限，然后点重试；或取消本次安装。"
ShowInstDetails show
ShowUnInstDetails show
BrandingText "HomePanel · MIoT 桌面客户端"
VIProductVersion "${VERSION_NUM}"

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\HomePanel.exe"
!define MUI_FINISHPAGE_RUN_TEXT "安装完成后启动 HomePanel"
!insertmacro MUI_PAGE_WELCOME
!ifdef HOST_INCLUDE
!define MUI_LICENSEPAGE_TEXT_TOP "集成版包含可再分发的 Microsoft Windows App SDK 和 VC++ 运行库。HomePanel 自有代码仍为 MIT；第三方组件保留各自许可。"
!insertmacro MUI_PAGE_LICENSE "${SOURCE_ROOT}\packaging\RuntimeTerms.txt"
!insertmacro MUI_PAGE_LICENSE "${SOURCE_ROOT}\LICENSES\Microsoft-VC-runtime.rtf"
!endif
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"

!macro AppFileGuard PREFIX
Function ${PREFIX}EnsureAppFileWritable
    Push $0
    Push $1
retry:
    IfFileExists "$INSTDIR\HomePanel.exe" 0 ready
    ; OPEN_EXISTING + GENERIC_WRITE only probes access. It never creates,
    ; truncates, deletes or modifies the executable. Allow all sharing modes.
    System::Call 'kernel32::CreateFileW(w "$INSTDIR\HomePanel.exe", i 0x40000000, i 7, p 0, i 3, i 0x80, p 0) p .r0 ?e'
    Pop $1
    IntPtrCmp $0 -1 blocked
    System::Call 'kernel32::CloseHandle(p r0)'
    Goto ready
blocked:
    DetailPrint "HomePanel 程序文件不可写，已停止更新（Windows 错误 $1）。"
    IfSilent cancelled
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
        "HomePanel 程序文件暂时无法更新。$\r$\n$\r$\n如果程序仍在运行，请右键右下角的 HomePanel 托盘图标，选择“退出”，然后点击“重试”。关闭面板不会退出托盘程序。$\r$\n$\r$\n如果已经退出，请检查安装目录是否只读或没有写入权限。$\r$\nWindows 错误：$1$\r$\n$\r$\n取消会结束本次操作，并保留现有程序。" \
        /SD IDCANCEL IDRETRY retry
cancelled:
    SetErrorLevel 2
    Quit
ready:
    !ifdef HOST_INCLUDE
        !insertmacro HomePanelHostGuard
    !endif
    Pop $1
    Pop $0
FunctionEnd
!macroend

!insertmacro AppFileGuard ""
!insertmacro AppFileGuard "un."

Function StopAppForUpgrade
    Push $0
    InitPluginsDir
    File /oname=$PLUGINSDIR\HomePanel-Update.exe "${RELEASE_EXE}"
retry_stop:
    DetailPrint "正在退出目标安装目录中的 HomePanel…"
    ClearErrors
    ExecWait '$\"$PLUGINSDIR\HomePanel-Update.exe$\" --close-for-update $\"$INSTDIR\HomePanel.exe$\"' $0
    IfErrors stop_failed
    StrCmp $0 0 stopped
    StrCmp $0 3 stopped_legacy
stop_failed:
    IfSilent stop_cancelled
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "无法退出目标目录的 HomePanel，请检查权限后重试。$\r$\n现有安装尚未修改。" /SD IDCANCEL IDRETRY retry_stop
stop_cancelled:
    SetErrorLevel 2
    Quit
stopped_legacy:
    DetailPrint "旧版未响应退出通知，已结束目标安装目录中的进程。"
stopped:
    Pop $0
FunctionEnd

Section "HomePanel" SEC_MAIN
    Call StopAppForUpgrade
    Call EnsureAppFileWritable
    SetOutPath "$INSTDIR"
    SetOverwrite on
    File /oname=HomePanel.exe "${RELEASE_EXE}"
    File /oname=LICENSE "${SOURCE_ROOT}\LICENSE"
    File /oname=THIRD_PARTY_NOTICES.md "${SOURCE_ROOT}\THIRD_PARTY_NOTICES.md"
    CreateDirectory "$INSTDIR\LICENSES"
    SetOutPath "$INSTDIR\LICENSES"
    File /oname=DEPENDENCY_LICENSES.md "${SOURCE_ROOT}\LICENSES\DEPENDENCY_LICENSES.md"
    File /oname=NotoSansCJK-OFL.txt "${SOURCE_ROOT}\LICENSES\NotoSansCJK-OFL.txt"
    File /oname=zip-MIT.txt "${SOURCE_ROOT}\LICENSES\zip-MIT.txt"

    !ifdef HOST_INCLUDE
        !insertmacro HomePanelHostInstall
    !endif
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    CreateDirectory "${START_MENU_DIR}"
    CreateShortCut "${START_MENU_DIR}\HomePanel.lnk" "$INSTDIR\HomePanel.exe"
    CreateShortCut "${START_MENU_DIR}\Uninstall HomePanel.lnk" "$INSTDIR\Uninstall.exe"

    WriteRegStr HKCU "${APP_REG_KEY}" "InstallDir" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_REG_KEY}" "DisplayName" "HomePanel"
    WriteRegStr HKCU "${UNINSTALL_REG_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNINSTALL_REG_KEY}" "Publisher" "HomePanel contributors"
    WriteRegStr HKCU "${UNINSTALL_REG_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_REG_KEY}" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
    WriteRegDWORD HKCU "${UNINSTALL_REG_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_REG_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
    Call un.EnsureAppFileWritable
    Delete "${START_MENU_DIR}\HomePanel.lnk"
    Delete "${START_MENU_DIR}\Uninstall HomePanel.lnk"
    RMDir "${START_MENU_DIR}"

    Delete "$INSTDIR\LICENSES\DEPENDENCY_LICENSES.md"
    Delete "$INSTDIR\LICENSES\NotoSansCJK-OFL.txt"
    Delete "$INSTDIR\LICENSES\zip-MIT.txt"
    RMDir "$INSTDIR\LICENSES"
    Delete "$INSTDIR\THIRD_PARTY_NOTICES.md"
    Delete "$INSTDIR\LICENSE"
    !ifdef HOST_INCLUDE
        !insertmacro HomePanelHostDelete
    !endif
    Delete "$INSTDIR\HomePanel.exe"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir "$INSTDIR"

    DeleteRegKey HKCU "${UNINSTALL_REG_KEY}"
    DeleteRegKey HKCU "${APP_REG_KEY}"
    !ifdef TEST_MODE
        DeleteRegKey HKCU "Software\HomePanelInstallerTest"
    !endif
SectionEnd
