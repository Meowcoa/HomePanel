use eframe::CreationContext;
use crate::model::GlassStyle;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

/// Owned system picker; selecting a file alone does not execute it.
pub fn pick_panel_file(owner: Option<isize>, plugin: bool) -> Result<Option<std::path::PathBuf>, String> {
    pick_file(owner, if plugin {1} else {0})
}
pub fn pick_mijia_apk(owner: Option<isize>) -> Result<Option<std::path::PathBuf>, String> {pick_file(owner,2)}
fn pick_file(owner: Option<isize>, kind: u8) -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(windows)] {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::UI::Controls::Dialogs::{
            GetOpenFileNameW, CommDlgExtendedError, OPENFILENAMEW,
            OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OFN_NOCHANGEDIR,
            OFN_DONTADDTORECENT, OFN_HIDEREADONLY,
        };
        let mut buffer=vec![0u16;32768];
        let filter:Vec<u16>=if kind==2 {"本机米家 APK\0*.apk\0\0"} else if kind==1 {"米家插件包\0*.zip;*.mpkg;*.mpk\0所有文件\0*.*\0\0"}
            else {"HomePanel 适配包\0*.json\0\0"}.encode_utf16().collect();
        let title:Vec<u16>=if kind==2 {"导入本机持有的米家 APK · 不上传"}else if kind==1 {"选择插件包 · 仅做离线检测"}else{"导入 HomePanel 适配包"}.encode_utf16().chain(Some(0)).collect();
        let mut dialog:OPENFILENAMEW=unsafe{std::mem::zeroed()};
        dialog.lStructSize=std::mem::size_of::<OPENFILENAMEW>() as u32;
        dialog.hwndOwner=owner.map_or(std::ptr::null_mut(),|value|value as _);
        dialog.lpstrFilter=filter.as_ptr();dialog.nFilterIndex=1;
        dialog.lpstrFile=buffer.as_mut_ptr();dialog.nMaxFile=buffer.len() as u32;
        dialog.lpstrTitle=title.as_ptr();
        dialog.Flags=OFN_EXPLORER|OFN_FILEMUSTEXIST|OFN_PATHMUSTEXIST|OFN_NOCHANGEDIR|OFN_DONTADDTORECENT|OFN_HIDEREADONLY;
        if unsafe{GetOpenFileNameW(&mut dialog)}==0 {
            return if unsafe{CommDlgExtendedError()}==0 {Ok(None)}else{Err("无法打开文件选择窗口".into())};
        }
        let length=buffer.iter().position(|value|*value==0).ok_or("文件路径过长")?;
        Ok(Some(std::ffi::OsString::from_wide(&buffer[..length]).into()))
    }
    #[cfg(not(windows))] {let _=(owner,kind);Err("文件选择仅支持 Windows".into())}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub glass_available: bool,
    pub reduce_motion: bool,
    pub system_dark: bool,
}

pub fn appearance() -> Appearance {
    let mut result = Appearance {
        glass_available: true,
        reduce_motion: false,
        system_dark: false,
    };
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        use windows_sys::Win32::UI::Accessibility::{HIGHCONTRASTW, HCF_HIGHCONTRASTON};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETHIGHCONTRAST, SPI_GETCLIENTAREAANIMATION,
        };

        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        {
            result.glass_available = key.get_value::<u32, _>("EnableTransparency").unwrap_or(1) != 0;
            result.system_dark = key.get_value::<u32, _>("AppsUseLightTheme").unwrap_or(1) == 0;
        }
        let mut high = HIGHCONTRASTW {
            cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
            dwFlags: 0,
            lpszDefaultScheme: std::ptr::null_mut(),
        };
        let mut animate = 1i32;
        unsafe {
            if SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                high.cbSize,
                &mut high as *mut _ as _,
                0,
            ) != 0
                && high.dwFlags & HCF_HIGHCONTRASTON != 0
            {
                result.glass_available = false;
            }
            if SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                &mut animate as *mut _ as _,
                0,
            ) != 0
            {
                result.reduce_motion = animate == 0;
            }
        }
    }
    result
}

pub fn apply_window_appearance(cc: &CreationContext<'_>, material: GlassStyle, dark: bool) {
    let Ok(handle) = cc.window_handle() else {
        return;
    };
    apply_window_appearance_raw(handle.as_raw(), material, dark);
}

pub fn native_window_id(cc: &CreationContext<'_>) -> Option<isize> {
    let handle = cc.window_handle().ok()?;
    let RawWindowHandle::Win32(window) = handle.as_raw() else {
        return None;
    };
    Some(window.hwnd.get())
}

pub fn apply_window_appearance_raw(raw: RawWindowHandle, material: GlassStyle, dark: bool) {
    let RawWindowHandle::Win32(window) = raw else {
        return;
    };
    apply_window_appearance_hwnd(Some(window.hwnd.get()), material, dark);
}

#[cfg(windows)]
fn clipped_acrylic(hwnd: windows_sys::Win32::Foundation::HWND, enabled: bool, dark: bool) -> bool {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    #[repr(C)]
    struct AccentPolicy { state: i32, flags: u32, color: u32, animation: u32 }
    #[repr(C)]
    struct Attribute { kind: i32, data: *mut core::ffi::c_void, size: usize }
    type SetAttribute = unsafe extern "system" fn(windows_sys::Win32::Foundation::HWND, *mut Attribute) -> i32;
    // Unlike DWMSBT_TRANSIENTWINDOW, the accent surface follows SetWindowRgn.
    // Resolve the optional API dynamically, so absence cannot prevent startup.
    unsafe {
        let module = GetModuleHandleW(windows_sys::w!("user32.dll"));
        let Some(function) = GetProcAddress(module, c"SetWindowCompositionAttribute".as_ptr().cast()) else { return false; };
        let set: SetAttribute = std::mem::transmute(function);
        let mut policy = AccentPolicy {state: if enabled {4} else {0},flags:0,
            color:if dark {0x40231914} else {0x20ffffff},animation:0};
        let mut attribute = Attribute {kind:19,data:(&mut policy as *mut AccentPolicy).cast(),size:std::mem::size_of::<AccentPolicy>()};
        set(hwnd,&mut attribute)!=0
    }
}

pub fn apply_window_appearance_hwnd(handle: Option<isize>, material: GlassStyle, dark: bool) {
    let Some(handle) = handle else {
        return;
    };
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::HWND,
            Graphics::Dwm::DwmSetWindowAttribute,
        };
        let hwnd = handle as HWND;
        const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
        const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
        // Windows 11's system backdrop can paint the original 460x620 rectangle
        // even when GetWindowRgn correctly reports the smaller detail card.
        // Disable that surface, then use the region-clipped acrylic accent.
        const DWMWCP_DONOTROUND: i32 = 1;
        const DWMSBT_NONE: i32 = 1;
        let dark_value = i32::from(dark);
        let corner = DWMWCP_DONOTROUND;
        let backdrop = DWMSBT_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_value as *const _ as _,
            std::mem::size_of_val(&dark_value) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as _,
            std::mem::size_of_val(&corner) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as _,
            std::mem::size_of_val(&backdrop) as u32,
        );
        clipped_acrylic(hwnd, material == GlassStyle::Frosted, dark);
    }
    #[cfg(not(windows))]
    let _ = (handle, material, dark);
}

/// Apply the same silhouette to native composition and egui. Cache by physical
/// size/radius so moving or animating content never recreates the window region.
pub fn rounded_window(handle:Option<isize>,scale:f32,detail:bool,previous:Option<[i32;5]>)->Option<[i32;5]> {
    #[cfg(windows)]
    if let Some(handle)=handle {
        use windows_sys::Win32::{Foundation::RECT,Graphics::Gdi::{CreateRoundRectRgn,DeleteObject,SetWindowRgn},UI::WindowsAndMessaging::GetWindowRect};
        // SAFETY: the HWND belongs to this UI thread; RECT is writable. A successful
        // SetWindowRgn transfers region ownership to Windows; failure keeps it here.
        unsafe {
            let mut rect:RECT=std::mem::zeroed();
            if GetWindowRect(handle as _,&mut rect)==0 {return previous;}
            let shape=region_shape(rect.right-rect.left,rect.bottom-rect.top,scale,detail)?;
            if previous==Some(shape) {return previous;}
            let region=CreateRoundRectRgn(shape[0],shape[1],shape[0]+shape[2]+1,shape[1]+shape[3]+1,shape[4],shape[4]);
            if region.is_null() {return previous;}
            if SetWindowRgn(handle as _,region,1)==0 {DeleteObject(region);return previous;}
            return Some(shape);
        }
    }
    let _=(handle,scale,detail);
    previous
}

fn region_shape(width:i32,height:i32,scale:f32,detail:bool)->Option<[i32;5]> {
    if width<=0 || height<=0 || !scale.is_finite() || scale<=0.0 {return None;}
    let (left,top,w,h,radius)=if detail {
        let w=(crate::design::DETAIL_SIZE.x*scale).round() as i32;
        let h=(crate::design::DETAIL_SIZE.y*scale).round() as i32;
        if w>width || h>height {return None;}
        ((width-w)/2,(height-h)/2,w,h,crate::design::DETAIL_RADIUS)
    } else {(0,0,width,height,crate::design::WINDOW_RADIUS)};
    let diameter=(radius*2.0*scale).round() as i32;
    Some([left,top,w,h,diameter.clamp(2,w.min(h))])
}

/// Debug-only observation of our own demo window. No input injection or account data.
#[cfg(debug_assertions)]
pub fn inspect_window_silhouette(handle:Option<isize>)->serde_json::Value {
    #[cfg(windows)]
    if let Some(handle)=handle {
        use windows_sys::Win32::{Foundation::RECT,Graphics::Gdi::{CreateRectRgn,DeleteObject,GetWindowRgn,GetRgnBox,PtInRegion},UI::WindowsAndMessaging::{GetWindowRect,GetClientRect}};
        unsafe {
            let region=CreateRectRgn(0,0,0,0);
            if region.is_null(){return serde_json::json!({"error":"create-region"});}
            let kind=GetWindowRgn(handle as _,region);
            let mut bounds:RECT=std::mem::zeroed();GetRgnBox(region,&mut bounds);
            let mut window:RECT=std::mem::zeroed();GetWindowRect(handle as _,&mut window);
            let mut client:RECT=std::mem::zeroed();GetClientRect(handle as _,&mut client);
            let corner=PtInRegion(region,bounds.left,bounds.top)!=0;
            let center=PtInRegion(region,(bounds.left+bounds.right)/2,(bounds.top+bounds.bottom)/2)!=0;
            let mut backdrop=0i32;
            let backdrop_result=windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(handle as _,38,(&mut backdrop as *mut i32).cast(),std::mem::size_of::<i32>() as u32);
            let result=serde_json::json!({"regionType":kind,"bounds":[bounds.left,bounds.top,bounds.right,bounds.bottom],
                "windowSize":[window.right-window.left,window.bottom-window.top],"clientSize":[client.right-client.left,client.bottom-client.top],
                "cornerInside":corner,"centerInside":center,"systemBackdrop":backdrop,"backdropQuerySucceeded":backdrop_result==0});
            DeleteObject(region);return result;
        }
    }
    let _=handle;serde_json::json!({"error":"no-window"})
}

#[cfg(test)]
mod silhouette_tests {
    use super::*;
    #[test]
    fn region_uses_physical_dpi_and_one_shared_radius() {
        for scale in [1.0,1.25,1.5,2.0] {
            let shape=region_shape((460.0*scale) as i32,(620.0*scale) as i32,scale,false).unwrap();
            assert_eq!(shape[4],(48.0*scale).round() as i32);
            let detail=region_shape((460.0*scale) as i32,(620.0*scale) as i32,scale,true).unwrap();
            assert_eq!(detail,[ (36.0*scale) as i32,(40.0*scale) as i32,(388.0*scale) as i32,(540.0*scale) as i32,(60.0*scale) as i32 ]);
            assert_ne!(shape,detail); // Native region must be rebuilt on sheet open/close.
            let compact=region_shape((388.0*scale).round() as i32,(540.0*scale).round() as i32,scale,true).unwrap();
            assert_eq!(compact,[0,0,(388.0*scale).round() as i32,(540.0*scale).round() as i32,(60.0*scale).round() as i32]);
        }
        assert!(region_shape(0,620,1.0,false).is_none());assert!(region_shape(460,620,f32::NAN,false).is_none());
        assert!(region_shape(320,400,1.0,true).is_none());
    }
}

/// Post to winit's native message loop, which stays active while the panel is
/// hidden. Winit/eframe destroy their own window and tray on the owning thread.
pub fn request_window_exit(handle: Option<isize>) -> bool {
    #[cfg(windows)]
    return handle.is_some_and(|handle| unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        PostMessageW(handle as _, WM_CLOSE, 0, 0) != 0
    });
    #[cfg(not(windows))]
    { let _ = handle; false }
}

pub fn wake_window_for_exit(handle: Option<isize>) {
    #[cfg(windows)]
    if let Some(handle) = handle {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindowVisible, ShowWindow, SW_SHOWMINNOACTIVE};
        unsafe {
            if IsWindowVisible(handle as _) == 0 { ShowWindow(handle as _, SW_SHOWMINNOACTIVE); }
        }
    }
    #[cfg(not(windows))]
    let _ = handle;
}

pub fn wake_window(handle: Option<isize>) {
    #[cfg(windows)]
    if let Some(handle) = handle {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindowVisible, ShowWindow, SW_SHOWNOACTIVATE};
        unsafe {
            if IsWindowVisible(handle as _) == 0 { ShowWindow(handle as _, SW_SHOWNOACTIVATE); }
        }
    }
    #[cfg(not(windows))]
    let _ = handle;
}

/// Native popups and other windows owned by this process belong to the same group.
pub fn own_foreground(handle: Option<isize>) -> Option<bool> {
    #[cfg(windows)]
    return handle.and_then(|handle| unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow,GetWindowThreadProcessId};
        let foreground=GetForegroundWindow();
        if foreground.is_null() { return None; }
        let mut ours=0;let mut theirs=0;
        GetWindowThreadProcessId(handle as _,&mut ours);
        GetWindowThreadProcessId(foreground,&mut theirs);
        (ours!=0 && theirs!=0).then_some(ours==theirs)
    });
    #[cfg(not(windows))]
    { let _=handle; None }
}

pub fn is_window_visible(handle: Option<isize>) -> bool {
    #[cfg(windows)]
    return handle.is_some_and(|handle| unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(handle as _) != 0 });
    #[cfg(not(windows))]
    { let _ = handle; true }
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .map_err(|_| "无法设置当前用户的开机启动")?;
        if enabled {
            let path = std::env::current_exe().map_err(|_| "无法定位程序路径")?;
            key.set_value("HomePanel", &format!("\"{}\" --background", path.display()))
                .map_err(|_| "无法保存开机启动设置")?;
        } else {
            match key.delete_value("HomePanel") {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("无法关闭开机启动".into()),
            }
        }
    }
    #[cfg(not(windows))]
    let _ = enabled;
    Ok(())
}

pub fn create_tray_icon() -> Result<TrayIcon, String> {
    let menu = Menu::new();
    let open = MenuItem::with_id("open", "显示 / 隐藏面板", true, None);
    let settings = MenuItem::with_id("settings", "设置", true, None);
    let quit = MenuItem::with_id("quit", "退出 HomePanel", true, None);
    menu.append(&open).map_err(|_| "无法创建托盘菜单")?;
    menu.append(&settings).map_err(|_| "无法创建托盘菜单")?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|_| "无法创建托盘菜单")?;
    menu.append(&quit).map_err(|_| "无法创建托盘菜单")?;

    TrayIconBuilder::new()
        .with_icon(tray_icon_image()?)
        .with_tooltip("HomePanel · 米家设备")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|_| "无法创建 Windows 托盘图标".into())
}

fn tray_icon_image() -> Result<Icon, String> {
    let size = 32usize;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let roof = (8..=16).contains(&y)
                && (x as i32 - 16).abs() <= y as i32 - 6
                && (x as i32 - 16).abs() >= y as i32 - 9;
            let walls = (8..=24).contains(&x)
                && (15..=26).contains(&y)
                && (x <= 10 || x >= 22 || y >= 24);
            let door = (14..=18).contains(&x) && (20..=26).contains(&y);
            if roof || walls || door {
                let offset = (y * size + x) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[45, 167, 129, 255]);
            }
        }
    }
    Icon::from_rgba(pixels, size as u32, size as u32)
        .map_err(|_| "无法生成托盘图标".into())
}

pub fn panel_position(anchor: Option<[f64; 2]>, scale: f32, size: [f32; 2]) -> [f32; 2] {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let point = if let Some([x, y]) = anchor {
            windows_sys::Win32::Foundation::POINT {
                x: x.round() as i32,
                y: y.round() as i32,
            }
        } else {
            let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            let _ = GetCursorPos(&mut point);
            point
        };
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        if !monitor.is_null() {
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let work = info.rcWork;
                let scale = scale.max(1.0);
                let width = size[0] * scale;
                let height = size[1] * scale;
                let margin = 12.0 * scale;
                let x = (work.right as f32 - width - margin).max(work.left as f32 + margin);
                let y = (work.bottom as f32 - height - margin).max(work.top as f32 + margin);
                return [x / scale, y / scale];
            }
        }
    }
    #[cfg(not(windows))]
    let _ = anchor;
    let _ = scale;
    [12.0, 12.0]
}

pub fn acquire_single_instance(demo: bool) -> Result<bool, String> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS},
            System::Threading::CreateMutexW,
        };
        static INSTANCE_HANDLE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let mutex_name = if cfg!(debug_assertions) && demo { "Local\\HomePanel.Native.Demo.Singleton\0" } else { "Local\\HomePanel.Native.Singleton\0" };
        let name: Vec<u16> = mutex_name.encode_utf16().collect();
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if handle.is_null() {
            return Err("无法初始化单实例保护".into());
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(handle);
            activate_existing_window(demo);
            return Ok(false);
        }
        let _ = INSTANCE_HANDLE.set(handle as usize);
        return Ok(true);
    }
    #[cfg(not(windows))]
    Ok(true)
}

#[cfg(windows)]
fn activate_existing_window(demo: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    let title: Vec<u16> = if cfg!(debug_assertions)&&demo {"HomePanel · 演示预览\0"}else{"HomePanel\0"}.encode_utf16().collect();
    for _ in 0..25 {
        let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if !hwnd.is_null() {
            unsafe {
                ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
}
