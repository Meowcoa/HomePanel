//! Installer exit handshake, restricted to the exact selected HomePanel executable.
use std::path::Path;

fn normalized(path:&Path)->Result<String,String> {
    std::fs::canonicalize(path).ok().and_then(|p|p.to_str().map(str::to_lowercase))
        .ok_or("无法确认 HomePanel 安装路径".into())
}
fn event_name(path:&Path)->Result<Vec<u16>,String> {
    use sha2::{Digest,Sha256};
    let name=format!("Local\\HomePanel.UpdateExit.{:x}\0",Sha256::digest(normalized(path)?.as_bytes()));
    Ok(name.encode_utf16().collect())
}

pub fn listen(exit:impl FnOnce()+Send+'static)->Result<(),String> {
    #[cfg(windows)] {
        use std::os::windows::io::{AsRawHandle,FromRawHandle,OwnedHandle};
        use windows_sys::Win32::{Foundation::WAIT_OBJECT_0,System::Threading::{CreateEventW,WaitForSingleObject,INFINITE}};
        let exe=std::env::current_exe().map_err(|_|"无法定位程序路径")?;
        let name=event_name(&exe)?;
        let raw=unsafe{CreateEventW(std::ptr::null(),0,0,name.as_ptr())};
        if raw.is_null(){return Err("无法建立安装退出通知".into());}
        let event=unsafe{OwnedHandle::from_raw_handle(raw)};
        std::thread::Builder::new().name("homepanel-update-exit".into()).spawn(move || {
            if unsafe{WaitForSingleObject(event.as_raw_handle(),INFINITE)}==WAIT_OBJECT_0 {exit();}
        }).map_err(|_|"无法启动安装退出通知")?;
        Ok(())
    }
    #[cfg(not(windows))] {let _=exit;Ok(())}
}

/// true means an older/nonresponsive process required the authorized fallback.
/// This command runs before config directories, sessions, tray or cloud startup.
pub fn close_for_update(target:&Path)->Result<bool,String> {
    if !target.file_name().and_then(|n|n.to_str()).is_some_and(|n|n.eq_ignore_ascii_case("HomePanel.exe")) {
        return Err("安装退出操作仅允许目标目录中的 HomePanel.exe".into());
    }
    if !target.exists(){return Ok(false);}
    #[cfg(windows)] {
        use std::os::windows::io::{AsRawHandle,FromRawHandle,OwnedHandle};
        use windows_sys::core::BOOL;
        use windows_sys::Win32::{Foundation::{HANDLE,INVALID_HANDLE_VALUE,LPARAM,WAIT_OBJECT_0},
            System::{Diagnostics::ToolHelp::*,Threading::*},UI::WindowsAndMessaging::*};
        fn image(handle:HANDLE)->Option<String> {
            let mut buffer=vec![0u16;32768];let mut count=buffer.len() as u32;
            if unsafe{QueryFullProcessImageNameW(handle,PROCESS_NAME_WIN32,buffer.as_mut_ptr(),&mut count)}==0{return None;}
            normalized(Path::new(&String::from_utf16(&buffer[..count as usize]).ok()?)).ok()
        }
        fn running(expected:&str)->Result<Vec<(u32,OwnedHandle)>,String> {
            let raw=unsafe{CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0)};
            if raw==INVALID_HANDLE_VALUE{return Err("无法检查正在运行的 HomePanel".into());}
            let snapshot=unsafe{OwnedHandle::from_raw_handle(raw)};
            let mut entry:PROCESSENTRY32W=unsafe{std::mem::zeroed()};entry.dwSize=std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut found=Vec::new();let mut next=unsafe{Process32FirstW(snapshot.as_raw_handle(),&mut entry)};
            while next!=0 {
                let length=entry.szExeFile.iter().position(|c|*c==0).unwrap_or(entry.szExeFile.len());
                let name=String::from_utf16_lossy(&entry.szExeFile[..length]);
                if name.eq_ignore_ascii_case("HomePanel.exe") && entry.th32ProcessID!=unsafe{GetCurrentProcessId()} {
                    let raw=unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|PROCESS_SYNCHRONIZE|PROCESS_TERMINATE,0,entry.th32ProcessID)};
                    if !raw.is_null() {
                        let process=unsafe{OwnedHandle::from_raw_handle(raw)};
                        if image(process.as_raw_handle()).as_deref()==Some(expected){found.push((entry.th32ProcessID,process));}
                    }
                }
                next=unsafe{Process32NextW(snapshot.as_raw_handle(),&mut entry)};
            }
            Ok(found)
        }
        unsafe extern "system" fn close_owned_window(window:HANDLE,param:LPARAM)->BOOL {
            let mut pid=0;unsafe{GetWindowThreadProcessId(window,&mut pid)};
            if pid==param as u32 {unsafe{PostMessageW(window,WM_CLOSE,0,0)};}
            1
        }
        let expected=normalized(target)?;let processes=running(&expected)?;
        if processes.is_empty(){return Ok(false);}
        let name=event_name(target)?;
        let raw=unsafe{OpenEventW(EVENT_MODIFY_STATE,0,name.as_ptr())};
        let graceful=if raw.is_null(){false}else{
            let event=unsafe{OwnedHandle::from_raw_handle(raw)};
            unsafe{SetEvent(event.as_raw_handle())!=0}
        };
        if !graceful {for (pid,_) in &processes {unsafe{EnumWindows(Some(close_owned_window),*pid as LPARAM)};}}
        let mut forced=false;
        for (_,process) in processes {
            let handle=process.as_raw_handle();
            if unsafe{WaitForSingleObject(handle,if graceful {6000}else{1000})}==WAIT_OBJECT_0 {continue;}
            // Recheck identity on the same open handle; never kill by image name
            // or a PID that could now belong to a different process.
            if image(handle).as_deref()!=Some(expected.as_str()) {return Err("旧版进程身份已变化，已停止安装".into());}
            if unsafe{TerminateProcess(handle,0)}==0 || unsafe{WaitForSingleObject(handle,3000)}!=WAIT_OBJECT_0 {
                return Err("旧版 HomePanel 无法退出，请手动退出后重试".into());
            }
            forced=true;
        }
        // Old integrated hosts use a parent-exit watchdog; allow it to release DLLs.
        if forced {std::thread::sleep(std::time::Duration::from_millis(1500));}
        if !running(&expected)?.is_empty(){return Err("目标目录的 HomePanel 仍在运行，已停止安装".into());}
        Ok(forced)
    }
    #[cfg(not(windows))] {Err("安装自动退出仅支持 Windows".into())}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn installer_exit_rejects_other_programs_before_process_enumeration() {
        assert!(close_for_update(Path::new("notepad.exe")).is_err());
        assert!(close_for_update(Path::new("HomePanelPluginHost.exe")).is_err());
        let absent=std::env::temp_dir().join(format!("homepanel-absent-update-{}",crate::now_ms())).join("HomePanel.exe");
        assert_eq!(close_for_update(&absent).unwrap(),false);
    }
    #[test] fn update_notifications_are_bound_to_the_canonical_executable_path() {
        let own=std::env::current_exe().unwrap();let name=event_name(&own).unwrap();
        assert_eq!(name,event_name(&own.parent().unwrap().join(".").join(own.file_name().unwrap())).unwrap());
        assert_eq!(name.last(),Some(&0));
    }
}
