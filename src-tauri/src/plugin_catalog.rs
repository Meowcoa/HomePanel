//! Local download inventory. Entries are records of downloads, never run grants.
use crate::plugin_download::DownloadReport;
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{collections::HashSet,io::Read,path::{Path,PathBuf},sync::Mutex};

const MAX_ARCHIVE:u64=128*1024*1024;
const MAX_CATALOG:u64=512*1024;
const MAX_ENTRIES:usize=512;
static CATALOG_LOCK:Mutex<()>=Mutex::new(());

#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum PackageKind {Plugin,Sdk}

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ProjectSummary {pub package_path:Option<String>,pub version:Option<String>}

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct CatalogEntry {
    pub sha256:String,
    /// None means an old quarantine file with no recorded model association.
    pub kind:Option<PackageKind>,
    pub model:Option<String>,
    pub report:Option<DownloadReport>,
    pub downloaded_at_ms:Option<u64>,
    pub bytes:u64,
    pub public_project:Option<ProjectSummary>,
    /// Computed from the current filesystem; not a persisted assertion.
    #[serde(skip)] pub cache_available:bool,
}

#[derive(Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Catalog {schema:u32,entries:Vec<CatalogEntry>}

fn model_valid(model:&str)->bool {
    model.len()<=160 && model.split('.').count()>=3 && model.split('.').all(|part|!part.is_empty()
        && part.bytes().all(|c|c.is_ascii_lowercase()||c.is_ascii_digit()||c==b'_'))
}
fn sha_valid(sha:&str)->bool{sha.len()==64 && sha.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))}
fn is_reparse(metadata:&std::fs::Metadata)->bool {
    #[cfg(windows)] {use std::os::windows::fs::MetadataExt;if metadata.file_attributes()&0x400!=0{return true;}}
    metadata.file_type().is_symlink()
}
fn check_location(root:&Path)->Result<PathBuf,String>{
    let directory=root.join("plugin-downloads-v1");
    for path in [root,&directory] {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir()&&!is_reparse(&metadata)=>(),
            Ok(_)=>return Err("插件目录包含重解析点或不是普通目录".into()),
            Err(error) if error.kind()==std::io::ErrorKind::NotFound=>(),
            Err(_)=>return Err("无法检查插件目录".into()),
        }
    }
    Ok(directory)
}

/// Resolve only content-addressed files in this app's own quarantine directory.
/// Existence is not authenticity; callers must verify signatures and package pins.
pub fn cache_path(root:&Path,sha:&str)->Result<PathBuf,String>{
    if !sha_valid(sha){return Err("插件 SHA256 格式无效".into());}
    let directory=check_location(root)?.join("quarantine");
    let metadata=std::fs::symlink_metadata(&directory).map_err(|_|"插件缓存目录不存在")?;
    if !metadata.is_dir()||is_reparse(&metadata){return Err("插件缓存目录不是普通目录".into());}
    let path=directory.join(format!("{sha}.plugin-cache"));
    let metadata=std::fs::symlink_metadata(&path).map_err(|_|"下载记录对应的插件文件不存在")?;
    if !metadata.is_file()||is_reparse(&metadata)||metadata.len()==0||metadata.len()>MAX_ARCHIVE{return Err("插件缓存文件类型或大小无效".into());}
    Ok(path)
}

fn summary_valid(summary:&ProjectSummary)->bool {
    let safe=|value:&Option<String>,max:usize|value.as_ref().is_none_or(|v|!v.is_empty()&&v.len()<=max
        &&v.bytes().all(|c|c.is_ascii_alphanumeric()||b"._-+".contains(&c)));
    safe(&summary.package_path,160)&&safe(&summary.version,80)
}
fn reset_execution_flags(report:&mut DownloadReport){report.publisher_signature_verified=false;report.can_execute=false;}
fn validate_entry(entry:&mut CatalogEntry)->Result<(),String>{
    if !sha_valid(&entry.sha256)||entry.bytes==0||entry.bytes>MAX_ARCHIVE{return Err("插件目录含有无效摘要或大小".into());}
    match entry.kind {
        Some(PackageKind::Plugin) if entry.model.as_deref().is_some_and(model_valid)=>(),
        Some(PackageKind::Sdk) if entry.model.is_none()=>(),
        _=>return Err("插件目录的型号关联无效".into()),
    }
    if !entry.downloaded_at_ms.is_some_and(|time|time>0&&time<=4_102_444_800_000){return Err("插件目录时间无效".into());}
    let report=entry.report.as_mut().ok_or("插件目录缺少下载报告")?;
    if report.sha256!=entry.sha256||report.bytes!=entry.bytes||report.version==0
        ||report.sdk_version.is_some_and(|v|v==0||v>100_000){return Err("插件目录报告与摘要不符".into());}
    if entry.public_project.as_ref().is_some_and(|summary|!summary_valid(summary)){return Err("插件目录公开清单字段无效".into());}
    reset_execution_flags(report);entry.cache_available=false;Ok(())
}
fn catalog_path(root:&Path)->Result<PathBuf,String>{Ok(check_location(root)?.join("catalog-v1.json"))}
fn load(root:&Path)->Result<Catalog,String>{
    let path=catalog_path(root)?;
    let metadata=match std::fs::symlink_metadata(&path){Ok(value)=>value,Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(Catalog{schema:1,entries:Vec::new()}),Err(_)=>return Err("无法读取插件下载目录".into())};
    if !metadata.is_file()||is_reparse(&metadata)||metadata.len()>MAX_CATALOG{return Err("插件下载目录文件类型或大小无效".into());}
    let file=std::fs::File::open(path).map_err(|_|"无法读取插件下载目录")?;let mut bytes=Vec::new();
    file.take(MAX_CATALOG+1).read_to_end(&mut bytes).map_err(|_|"无法读取插件下载目录")?;
    if bytes.len() as u64>MAX_CATALOG{return Err("插件下载目录超过大小上限".into());}
    let mut catalog:Catalog=serde_json::from_slice(&bytes).map_err(|_|"插件下载目录损坏，原记录已保留")?;
    if catalog.schema!=1||catalog.entries.len()>MAX_ENTRIES{return Err("插件下载目录版本或条数不受支持".into());}
    let mut keys=HashSet::new();
    for entry in &mut catalog.entries {
        validate_entry(entry)?;
        if !keys.insert((entry.sha256.clone(),entry.kind==Some(PackageKind::Sdk),entry.model.clone())){return Err("插件下载目录存在重复记录".into());}
    }
    Ok(catalog)
}

fn public_summary(path:&Path)->Option<ProjectSummary>{
    let mut archive=zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
    if archive.len()>4096{return None;}
    let android=archive.file_names().filter(|name|*name=="android/project.json").count();
    let root=archive.file_names().filter(|name|*name=="project.json").count();
    let name=match (android,root){(1,0)=>"android/project.json",(0,1)=>"project.json",_=>return None};
    let mut entry=archive.by_name(name).ok()?;if entry.size()>64*1024{return None;}
    let mut bytes=Vec::new();entry.by_ref().take(64*1024+1).read_to_end(&mut bytes).ok()?;
    if bytes.len()>64*1024||bytes.len() as u64!=entry.size(){return None;}
    let json:serde_json::Value=serde_json::from_slice(&bytes).ok()?;
    let summary=ProjectSummary{package_path:json.get("package_path").and_then(|v|v.as_str()).map(str::to_owned),
        version:json.get("version").and_then(|v|v.as_str()).map(str::to_owned)};
    if summary_valid(&summary)&&(summary.package_path.is_some()||summary.version.is_some()){Some(summary)}else{None}
}

/// Persist only safe metadata after confirming the cache matches the report.
pub fn record(root:&Path,kind:PackageKind,model:Option<&str>,report:&DownloadReport)->Result<CatalogEntry,String>{
    let _guard=CATALOG_LOCK.lock().map_err(|_|"插件下载目录正在恢复，请重启程序")?;
    let path=cache_path(root,&report.sha256)?;
    let file=std::fs::File::open(&path).map_err(|_|"无法核对插件缓存")?;
    if file.metadata().map_err(|_|"无法核对插件缓存")?.len()!=report.bytes{return Err("插件缓存大小与下载报告不符".into());}
    let mut hash=Sha256::new();let mut stream=file.take(MAX_ARCHIVE+1);let mut buffer=[0u8;16*1024];let mut read_total=0u64;
    loop {let count=stream.read(&mut buffer).map_err(|_|"无法核对插件缓存")?;if count==0{break;}read_total+=count as u64;if read_total>MAX_ARCHIVE{return Err("插件缓存超过上限".into());}hash.update(&buffer[..count]);}
    if read_total!=report.bytes||format!("{:x}",hash.finalize())!=report.sha256{return Err("插件缓存 SHA256 与下载报告不符".into());}
    let mut normalized=report.clone();reset_execution_flags(&mut normalized);
    let mut entry=CatalogEntry{sha256:report.sha256.clone(),kind:Some(kind),model:model.map(str::to_owned),report:Some(normalized),
        downloaded_at_ms:Some(crate::now_ms()),bytes:report.bytes,public_project:public_summary(&path),cache_available:false};
    validate_entry(&mut entry)?;
    let mut catalog=load(root)?;
    if let Some(existing)=catalog.entries.iter_mut().find(|old|old.sha256==entry.sha256&&old.kind==entry.kind&&old.model==entry.model){*existing=entry.clone();}
    else {if catalog.entries.len()>=MAX_ENTRIES{return Err("插件下载记录已达 512 条上限，未覆盖原记录".into());}catalog.entries.push(entry.clone());}
    let bytes=serde_json::to_vec_pretty(&catalog).map_err(|_|"无法保存插件下载目录")?;
    if bytes.len() as u64>MAX_CATALOG{return Err("插件下载目录超过大小上限，未覆盖原记录".into());}
    let destination=catalog_path(root)?;
    let temporary=PathBuf::from(format!("{}.tmp",destination.display()));
    match std::fs::symlink_metadata(&temporary) {
        Ok(metadata) if !metadata.is_file()||is_reparse(&metadata)=>return Err("插件下载目录临时文件类型不安全，原记录已保留".into()),
        Ok(_)=>(),
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>(),
        Err(_)=>return Err("无法检查插件下载目录临时文件".into()),
    }
    crate::storage::atomic_write(&destination,&bytes)?;
    entry.cache_available=true;Ok(entry)
}

/// Merge persisted model associations with pre-catalog quarantine files.
/// Unknown files never acquire a guessed model, SDK kind, version, or run status.
pub fn list(root:&Path)->Result<Vec<CatalogEntry>,String>{
    let _guard=CATALOG_LOCK.lock().map_err(|_|"插件下载目录正在恢复，请重启程序")?;
    let mut entries=load(root)?.entries;
    let mut known=HashSet::new();
    for entry in &mut entries {
        known.insert(entry.sha256.clone());
        entry.cache_available=cache_path(root,&entry.sha256).ok().and_then(|path|std::fs::metadata(path).ok()).is_some_and(|meta|meta.len()==entry.bytes);
    }
    let directory=check_location(root)?.join("quarantine");
    match std::fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir()&&!is_reparse(&metadata)=>(),
        Ok(_)=>return Err("插件缓存目录不是普通目录".into()),
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(entries),
        Err(_)=>return Err("无法检查插件缓存目录".into()),
    }
    let files=std::fs::read_dir(directory).map_err(|_|"无法读取插件缓存列表")?;
    let mut count=0usize;let mut inspected=0usize;
    for file in files {
        inspected+=1;if inspected>MAX_ENTRIES*2{return Err("插件缓存目录项超过检查上限".into());}
        let file=file.map_err(|_|"无法读取插件缓存条目")?;let name=file.file_name();let Some(name)=name.to_str()else{continue;};
        let Some(sha)=name.strip_suffix(".plugin-cache").filter(|sha|sha_valid(sha))else{continue;};
        count+=1;if count>MAX_ENTRIES{return Err("插件缓存文件数量超过 512 条检查上限".into());}
        if known.contains(sha){continue;}
        let Ok(path)=cache_path(root,sha)else{continue;};
        let bytes=std::fs::metadata(&path).map_err(|_|"无法检查插件缓存文件")?.len();
        entries.push(CatalogEntry{sha256:sha.into(),kind:None,model:None,report:None,downloaded_at_ms:None,bytes,public_project:public_summary(&path),cache_available:true});
        if entries.len()>MAX_ENTRIES{return Err("插件目录合计条数超过 512 条检查上限".into());}
    }
    entries.sort_by(|a,b|b.downloaded_at_ms.cmp(&a.downloaded_at_ms).then(a.sha256.cmp(&b.sha256)).then(a.model.cmp(&b.model)));
    Ok(entries)
}

#[cfg(test)]
#[path="plugin_catalog_tests.rs"]
mod tests;
