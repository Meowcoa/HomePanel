//! Non-executing compatibility gate for locally supplied Android RN packages.
//! Detection is not a sandbox or a claim that a package can execute on Windows.
use serde::{Serialize,Deserialize};
use serde_json::Value;
use std::{collections::BTreeSet, io::{Read, Seek}, path::Path};

const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_BUNDLE: u64 = 48 * 1024 * 1024;
pub const LICENSE: &str = include_str!("../../LICENSES/zip-MIT.txt");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BundleFormat { Javascript, IndexedRam, Hermes, Unknown }

impl BundleFormat {
    pub fn label(self) -> &'static str {
        match self { Self::Javascript => "JavaScript 文本", Self::IndexedRam => "RN RAM bundle",
            Self::Hermes => "Hermes 字节码", Self::Unknown => "未知格式" }
    }
}

pub fn identify(bytes: &[u8]) -> BundleFormat {
    if bytes.starts_with(&[0xe5,0xd1,0x0b,0xfb]) { BundleFormat::IndexedRam }
    else if bytes.starts_with(&[0xc6,0x1f,0xbc,0x03,0xc1,0x03,0x19,0x1f]) { BundleFormat::Hermes }
    else if std::str::from_utf8(bytes).is_ok() && ["__d(","var ","(function","function","'use strict'","\"use strict\""]
        .iter().any(|prefix| bytes.starts_with(prefix.as_bytes())) { BundleFormat::Javascript }
    else { BundleFormat::Unknown }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub package: String,
    pub version: String,
    pub min_sdk_api_level: Option<u64>,
    pub sdk_api_level: Option<u64>,
    pub platform: String,
    pub format: BundleFormat,
    pub bundle_bytes: u64,
    pub archive_bytes: u64,
    pub sha256: String,
    pub native_modules: Vec<String>,
    pub static_scan_is_exhaustive: bool,
    pub signature_files_present: bool,
    pub publisher_signature_verified: bool,
    pub can_execute: bool,
    pub fallback: &'static str,
    pub blockers: Vec<&'static str>,
}

fn read_entry<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, name: &str, max: u64) -> Result<Vec<u8>, String> {
    let matches = archive.file_names().filter(|n| *n == name).count();
    if matches != 1 { return Err("插件入口缺失或重复".into()); }
    let mut file = archive.by_name(name).map_err(|_| "无法打开插件入口")?;
    if file.size() > max { return Err("插件内容超过检测限制".into()); }
    let mut bytes = Vec::new();
    file.by_ref().take(max + 1).read_to_end(&mut bytes).map_err(|_| "插件内容损坏或无法解压")?;
    if bytes.len() as u64 > max { return Err("插件解压内容超过检测限制".into()); }
    Ok(bytes)
}

fn manifest_text(value: &Value, key: &str, max: usize) -> Result<String, String> {
    let text = value[key].as_str().ok_or("插件 manifest 缺少必要字段")?;
    if text.is_empty() || text.chars().count() > max || text.chars().any(char::is_control) {
        return Err("插件 manifest 字段无效".into());
    }
    Ok(text.to_owned())
}

fn module_references(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut names = BTreeSet::new();
    for tail in text.split("NativeModules.").skip(1) {
        let name: String = tail.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$').take(97).collect();
        if !name.is_empty() && name.len() <= 96 { names.insert(name); }
        if names.len() >= 256 { break; }
    }
    names.into_iter().collect()
}

pub fn inspect(path: &Path) -> Result<Report, String> {
    use sha2::{Digest, Sha256};
    let file = std::fs::File::open(path).map_err(|_| "无法打开插件包，请选择本机 ZIP 或 mpkg 文件")?;
    let meta = file.metadata().map_err(|_| "无法检查插件包")?;
    if !meta.is_file() || meta.len() > MAX_ARCHIVE { return Err("请选择不超过 64 MiB 的插件包文件".into()); }
    let mut bytes = Vec::new();
    file.take(MAX_ARCHIVE + 1).read_to_end(&mut bytes).map_err(|_| "无法读取插件包")?;
    if bytes.len() as u64 > MAX_ARCHIVE { return Err("插件包超过检测限制".into()); }
    let digest = format!("{:x}", Sha256::digest(&bytes));
    inspect_zip(std::io::Cursor::new(&bytes), meta.len(), digest)
}

fn inspect_zip<R: Read + Seek>(reader: R, archive_bytes: u64, sha256: String) -> Result<Report, String> {
    let mut archive = zip::ZipArchive::new(reader).map_err(|_| "不是可检测的 ZIP 插件包；不会尝试执行")?;
    if archive.len() > 4096 { return Err("插件文件数量超过检测限制".into()); }
    // Never extract paths. Reject path traversal and ambiguous duplicate entries.
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| "插件目录损坏")?;
        if entry.enclosed_name().is_none() || entry.name().contains('\\') || !names.insert(entry.name().to_owned()) {
            return Err("插件包含不安全或重复的文件路径".into());
        }
    }
    let raw = read_entry(&mut archive,"android/project.json", 32 * 1024)?;
    let manifest: Value = serde_json::from_slice(&raw).map_err(|_| "插件 manifest JSON 无效")?;
    let platform = manifest_text(&manifest,"platform",24)?;
    let bundle = read_entry(&mut archive,"android/main.bundle",MAX_BUNDLE)?;
    let format = identify(&bundle);
    let mut blockers = vec!["当前仅提供离线兼容检测，尚未接入可执行官方插件的 Windows RN 宿主",
        "米家原生服务、控件、权限和状态事件需要逐项验证；缺失能力不会返回假成功",
        "未验证发行签名及插件使用许可"];
    if format == BundleFormat::Hermes { blockers.push("Hermes 字节码还需匹配引擎版本"); }
    if format == BundleFormat::IndexedRam { blockers.push("RAM bundle 需要对应模块加载器及 RN 版本适配"); }
    if platform != "android" { blockers.push("该包的平台标记不属于已识别的 Android 候选格式"); }
    Ok(Report {
        package:manifest_text(&manifest,"package_path",160)?, version:manifest_text(&manifest,"version",48)?,
        min_sdk_api_level:manifest["min_sdk_api_level"].as_u64(),sdk_api_level:manifest["sdk_api_level"].as_u64(),
        platform,format,bundle_bytes:bundle.len() as u64,archive_bytes,sha256,
        native_modules:module_references(&bundle),static_scan_is_exhaustive:false,
        signature_files_present:names.iter().any(|s|s.ends_with(".sign")),
        publisher_signature_verified:false,can_execute:false,fallback:"保留原生适配面板或基础控制页",blockers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    fn package(path: &str, body: &[u8]) -> Vec<u8> {
        let mut zip=zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("android/project.json",options).unwrap();
        zip.write_all(br#"{"package_path":"homepanel.fixture","version":"1.0.0","platform":"android","sdk_api_level":1}"#).unwrap();
        zip.start_file(path,options).unwrap();zip.write_all(body).unwrap();
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn detects_bundle_formats_without_executing_and_never_claims_compatibility() {
        for (body,format) in [(b"__d(function(){NativeModules.MIOTSpec.read();})".as_slice(),BundleFormat::Javascript),
            (&[0xe5,0xd1,0x0b,0xfb,0,0],BundleFormat::IndexedRam),
            (&[0xc6,0x1f,0xbc,0x03,0xc1,0x03,0x19,0x1f],BundleFormat::Hermes),
            (&[0xff,0xff],BundleFormat::Unknown)] {
            let report=inspect_zip(Cursor::new(package("android/main.bundle",body)),1,"fixture".into()).unwrap();
            assert_eq!(report.format,format);assert!(!report.can_execute);assert!(!report.publisher_signature_verified);
            assert!(!report.static_scan_is_exhaustive);assert!(!report.blockers.is_empty());
        }
    }
    #[test]
    fn rejects_bad_archives_paths_and_missing_entry() {
        assert!(inspect_zip(Cursor::new(vec![1,2,3]),3,String::new()).is_err());
        for path in ["../escape", "android/other.bundle", "android\\main.bundle"] {
            assert!(inspect_zip(Cursor::new(package(path,b"__d()")),1,String::new()).is_err());
        }
    }
    #[test]
    fn module_names_are_bounded_deduplicated_and_do_not_export_manifest_secrets() {
        let body=b"__d(function(){NativeModules.MIOTSpec.read();NativeModules.MIOTSpec.write();NativeModules.MIOTDevice;})";
        let report=inspect_zip(Cursor::new(package("android/main.bundle",body)),1,String::new()).unwrap();
        assert_eq!(report.native_modules,vec!["MIOTDevice","MIOTSpec"]);
        let json=serde_json::to_string(&report).unwrap();assert!(!json.contains("developer_id"));
    }
}
