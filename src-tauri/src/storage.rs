use serde::{Deserialize,Serialize};
use std::path::{Path,PathBuf};
use zeroize::Zeroizing;

#[derive(Clone,Serialize,Deserialize,zeroize::Zeroize,zeroize::ZeroizeOnDrop)]
pub struct Session { pub user_id:String,pub ssecurity:String,pub service_token:String,pub agent:String }

#[cfg(windows)]
fn protect(bytes:&[u8],decrypt:bool)->Result<Vec<u8>,String> {
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB,CryptProtectData,CryptUnprotectData,CRYPTPROTECT_UI_FORBIDDEN};
    use windows_sys::Win32::Foundation::LocalFree;
    let input=CRYPT_INTEGER_BLOB{cbData:bytes.len() as u32,pbData:bytes.as_ptr() as *mut u8};
    let mut output=CRYPT_INTEGER_BLOB{cbData:0,pbData:std::ptr::null_mut()};
    let ok=unsafe{if decrypt{CryptUnprotectData(&input,std::ptr::null_mut(),std::ptr::null(),std::ptr::null_mut(),std::ptr::null(),CRYPTPROTECT_UI_FORBIDDEN,&mut output)}else{CryptProtectData(&input,std::ptr::null(),std::ptr::null(),std::ptr::null_mut(),std::ptr::null(),CRYPTPROTECT_UI_FORBIDDEN,&mut output)}};
    if ok==0 {return Err("Windows 无法保护或恢复账号会话，请重新扫码".into());}
    let result=unsafe{std::slice::from_raw_parts(output.pbData,output.cbData as usize).to_vec()};
    unsafe{LocalFree(output.pbData as *mut core::ffi::c_void);}
    Ok(result)
}
#[cfg(not(windows))] fn protect(_:&[u8],_:bool)->Result<Vec<u8>,String>{Err("此版本仅支持 Windows 的安全会话存储".into())}

pub fn save_session(root:&Path,session:&Session)->Result<(),String>{
    std::fs::create_dir_all(root).map_err(|_|"无法创建应用数据目录")?;
    let raw=Zeroizing::new(serde_json::to_vec(session).map_err(|_|"无法保存账号会话")?);
    let encrypted=protect(&raw,false)?;
    atomic_write(&root.join("session.dpapi"),&encrypted)
}
pub fn load_session(root:&Path)->Result<Option<Session>,String>{
    let path=root.join("session.dpapi");if !path.exists(){return Ok(None);}
    let bytes=std::fs::read(path).map_err(|_|"无法读取加密会话")?;
    let decoded=Zeroizing::new(protect(&bytes,true)?);
    serde_json::from_slice(&decoded).map(Some).map_err(|_|"加密会话已失效，请重新扫码".into())
}
pub fn clear_session(root:&Path)->Result<(),String>{let path=root.join("session.dpapi");if path.exists(){std::fs::remove_file(path).map_err(|_|"无法清除本应用的账号会话")?;}Ok(())}
pub fn atomic_write(path:&Path,bytes:&[u8])->Result<(),String>{
    if let Some(parent)=path.parent(){std::fs::create_dir_all(parent).map_err(|_|"无法创建应用数据目录")?;}
    let temp=PathBuf::from(format!("{}.tmp",path.display()));
    std::fs::write(&temp,bytes).map_err(|_|"无法写入应用数据")?;
    // Windows rename replaces a file through MoveFileEx, unlike std::fs::rename
    // on some filesystems. Backup-and-swap keeps the old preferences on failure.
    #[cfg(windows)] {
        use std::os::windows::ffi::OsStrExt;
        #[link(name="kernel32")] extern "system" {fn MoveFileExW(from:*const u16,to:*const u16,flags:u32)->i32;}
        let from:Vec<u16>=temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe{MoveFileExW(from.as_ptr(),to.as_ptr(),1|8)}==0{return Err("无法提交应用数据，原文件保留".into());}
    }
    #[cfg(not(windows))] std::fs::rename(&temp,path).map_err(|_|"无法提交应用数据")?;
    Ok(())
}
#[cfg(all(test,windows))] mod tests {use super::*;#[test]fn dpapi_roundtrip_and_ciphertext(){let plain=b"homepanel-test-not-a-real-session";let data=protect(plain,false).unwrap();assert_ne!(data,plain);assert_eq!(protect(&data,true).unwrap(),plain);assert!(protect(b"broken",true).is_err());}}
