//! User-supplied, exact-version interoperability candidate. No Xiaomi code is shipped.
use crate::{model::Device, storage};
use sha2::{Digest, Sha256};
use std::{io::{Read, Seek}, path::{Path, PathBuf}};

const SDK_HASH: &str = "aefa10092eb349175a21ea4025f2f8352a4c4461ed05324b4cfb1a2b2b598c01";
const PLUGIN_HASH: &str = "38f58139ff556c9f0e576962a7e51f885cf508da626e939d9516344d014aa11f";
const ZIP_HASH: &str = "0a0fa3b0a5889db5237b4649d6d9bc53d86276c33f85a701629d0822b8f412e1";
const MAX_SOURCE: u64 = 48 * 1024 * 1024;
pub fn directory(root: &Path) -> PathBuf { root.join("local-plugin-v1") }
pub fn host_path() -> Result<PathBuf, String> {
    let path = std::env::current_exe().map_err(|_| "无法定位安装目录")?
        .parent().ok_or("无法定位安装目录")?.join("PluginHost/HomePanelPluginHost.exe");
    if !path.is_file() { return Err("未安装插件宿主，请使用集成版安装包".into()); }
    Ok(path)
}
pub fn supports(device: &Device) -> bool {
    device.spec_status == "ready" && device.spec_urn.starts_with("urn:miot-spec-v2:device:outlet:")
        && device.properties.iter().filter(|p| p.key == "on" && p.service_key == "switch" && p.format == "bool" && p.readable).count() == 1
}
fn check(bytes: &[u8], hash: &str) -> Result<(), String> {
    if bytes.len() as u64 > MAX_SOURCE || format!("{:x}", Sha256::digest(bytes)) != hash {
        return Err("文件版本不在本机已研究样本列表内，未导入或执行。当前仅支持 miot.plugin.spec 1.1.10 样本。".into());
    }
    Ok(())
}
fn source(path: &Path, hash: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|_| "尚未导入本机米家 APK 样本，请先在设置中导入")?;
    if file.metadata().map_err(|_| "无法检查插件文件")?.len() > MAX_SOURCE { return Err("插件文件过大".into()); }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE + 1).read_to_end(&mut bytes).map_err(|_| "无法读取插件文件")?;
    check(&bytes, hash)?; Ok(bytes)
}
pub fn validate(root: &Path) -> Result<PathBuf, String> {
    let dir = directory(root);
    source(&dir.join("sdk.bundle"), SDK_HASH)?;
    source(&dir.join("switch.bundle"), PLUGIN_HASH)?;
    Ok(dir)
}
fn entry<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Result<Vec<u8>, String> {
    if zip.file_names().filter(|n| *n == name).count() != 1 { return Err("APK 缺少所需文件或包含重复入口".into()); }
    let mut file = zip.by_name(name).map_err(|_| "无法读取 APK 文件")?;
    if file.size() > MAX_SOURCE { return Err("APK 中的插件文件超过大小限制".into()); }
    let mut bytes = Vec::new();
    file.by_ref().take(MAX_SOURCE + 1).read_to_end(&mut bytes).map_err(|_| "APK 内容损坏")?;
    if bytes.len() as u64 > MAX_SOURCE { return Err("APK 内容超过解压限制".into()); }
    Ok(bytes)
}
/// Only two pinned JS sources are copied. Never extract arbitrary paths, account data,
/// native phone libraries, certificates, or APK signing/developer credentials.
/// Raster resources from the pinned ZIP remain in the importing user profile.
pub fn import_apk(path: &Path, root: &Path) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|_| "无法打开本机 APK")?;
    if file.metadata().map_err(|_| "无法检查 APK")?.len() > 1024 * 1024 * 1024 { return Err("请选择小于 1 GiB 的 APK".into()); }
    let mut apk = zip::ZipArchive::new(file).map_err(|_| "不是可读取的 APK/ZIP 文件")?;
    let sdk = entry(&mut apk, "assets/plugin_rn_sdk/sdk.bundle")?;
    check(&sdk, SDK_HASH)?;
    let plugin_zip = entry(&mut apk, "assets/rn_plugins/miot.switch.spec.zip")?;
    if format!("{:x}",Sha256::digest(&plugin_zip)) != ZIP_HASH {return Err("插件资源包版本不匹配，未导入".into());}
    let mut plugin = zip::ZipArchive::new(std::io::Cursor::new(plugin_zip)).map_err(|_| "插件压缩包损坏")?;
    let bundle = entry(&mut plugin, "android/main.bundle")?;
    check(&bundle, PLUGIN_HASH)?;
    // Validate both candidates before writing. Never replace a different existing pair.
    let dir = directory(root);
    if dir.exists() { validate(root)?; return Ok(()); }
    let staging = root.join(format!("local-plugin-import-{}-{}", std::process::id(), crate::now_ms()));
    std::fs::create_dir(&staging).map_err(|_| "无法创建本机插件目录")?;
    storage::atomic_write(&staging.join("sdk.bundle"), &sdk)?;
    storage::atomic_write(&staging.join("switch.bundle"), &bundle)?;
    let images=staging.join("images");std::fs::create_dir(&images).map_err(|_|"无法创建本机图片目录")?;
    let mut selected=std::collections::BTreeMap::<String,(f64,String)>::new();
    for index in 0..plugin.len() {
        let file=plugin.by_index(index).map_err(|_|"资源目录损坏")?;
        let path=file.name().to_owned();let parts:Vec<_>=path.split('/').collect();
        if parts.len()!=3 || parts[0]!="android" || file.enclosed_name().is_none() {continue;}
        let scale=match parts[1] {"drawable-mdpi"=>1.0,"drawable-hdpi"=>1.5,"drawable-xhdpi"=>2.0,"drawable-xxhdpi"=>3.0,"drawable-xxxhdpi"=>4.0,_=>continue};
        let name=parts[2];
        if !name.bytes().all(|c|c.is_ascii_lowercase()||c.is_ascii_digit()||c==b'_'||c==b'.') || ![".png",".jpg",".jpeg",".webp"].iter().any(|ext|name.ends_with(ext)) {continue;}
        if selected.get(name).is_none_or(|(old,_)|scale>*old) {selected.insert(name.to_owned(),(scale,path));}
    }
    let mut map=serde_json::Map::new();let mut total=0usize;
    for (name,(scale,path)) in selected {
        let bytes=entry(&mut plugin,&path)?;total+=bytes.len();
        if bytes.len()>4*1024*1024 || total>32*1024*1024 {return Err("图片资源超过本机导入限制".into());}
        storage::atomic_write(&images.join(&name),&bytes)?;
        map.insert(name.clone(),serde_json::json!({"name":name,"scale":scale}));
    }
    storage::atomic_write(&staging.join("local-assets.json"),serde_json::to_string(&map).map_err(|_|"资源清单无效")?.as_bytes())?;
    std::fs::rename(&staging, &dir).map_err(|_| "无法保存本机插件；原文件未覆盖")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn altered_or_unknown_sources_are_rejected() {
        assert!(check(b"__d(function(){})", SDK_HASH).is_err());
        assert!(check(b"__d(function(){})", PLUGIN_HASH).is_err());
    }
    #[test] fn a_device_name_cannot_enable_the_official_candidate() {
        let mut d = Device {name:"插座".into(),spec_status:"ready".into(),..Default::default()};
        assert!(!supports(&d));
        d.spec_urn = "urn:miot-spec-v2:device:outlet:1".into();
        let p = crate::model::Property {key:"on".into(),service_key:"switch".into(),format:"bool".into(),readable:true,..Default::default()};
        d.properties.push(p.clone()); assert!(supports(&d));
        d.properties.push(p); assert!(!supports(&d));
    }
}
