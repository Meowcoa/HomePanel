//! Prepare only exact packages whose Windows ABI is covered by the host matrix.
//! Local trust, full-file integrity and host compatibility are separate gates.
use crate::{cloud::Cloud,plugin_download::DownloadReport,plugin_probe::BundleFormat,plugin_signature::{self,Verification}};
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,io::Read,path::{Path,PathBuf}};

pub const SDK_SHA:&str="1e89d8a439ddc8a4327e96caa23f4e135f596081d46857cab16d71cf712710a2";
const MAX_FILE:u64=128*1024*1024;

#[derive(Clone,Debug)]
pub struct Profile {pub model:&'static str,pub sha:&'static str,pub version:u64,pub package:&'static str,pub display_version:&'static str}
pub const PROFILES:&[Profile]=&[
    Profile{model:"xiaomi.wifispeaker.l05c",sha:"717ebfe4d1946268056994e44ec623e15f2b77f0141a1c1df4f13035f57943f5",version:25,package:"com.xiaomi.wifispeaker",display_version:"1.0.72"},
    Profile{model:"chuangmi.plug.212a01",sha:"21c96ea1499d37ec10ebdc71ad51d2476fb525d46107eba2ce333c7f55c11976",version:31,package:"com.chuangmi.plug.spec",display_version:"1.0.85"},
];
pub fn profile(model:&str,report:&DownloadReport)->Result<&'static Profile,String> {
    PROFILES.iter().find(|p|p.model==model&&p.sha==report.sha256&&p.version==report.version
        &&report.sdk_version==Some(10116)&&report.format==BundleFormat::IndexedRam)
        .ok_or_else(||"此型号或插件版本尚未通过 Windows 宿主兼容验证；下载包已保留".into())
}

#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Prepared {
    pub model:String,pub version:u64,pub sha256:String,pub sdk_sha256:String,
    pub package_signature:Verification,pub sdk_signature:Verification,
    pub directory:PathBuf,
}
#[derive(Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Files {schema:u32,files:BTreeMap<String,String>}

fn digest(bytes:&[u8])->String {format!("{:x}",Sha256::digest(bytes))}
fn is_reparse(metadata:&std::fs::Metadata)->bool {
    #[cfg(windows)] {use std::os::windows::fs::MetadataExt;if metadata.file_attributes()&0x400!=0{return true;}}
    metadata.file_type().is_symlink()
}
fn bytes(path:&Path,limit:u64)->Result<Vec<u8>,String> {
    let metadata=std::fs::symlink_metadata(path).map_err(|_|"无法检查插件文件")?;
    if !metadata.is_file() || is_reparse(&metadata){return Err("插件运行文件不是普通文件".into());}
    let file=std::fs::File::open(path).map_err(|_|"无法读取待准备的插件文件")?;
    if file.metadata().map_err(|_|"无法检查插件文件")?.len()>limit{return Err("插件文件超过限制".into());}
    let mut data=Vec::new();file.take(limit+1).read_to_end(&mut data).map_err(|_|"无法读取插件文件")?;
    if data.len() as u64>limit{return Err("插件文件超过限制".into());}Ok(data)
}
fn snapshot(root:&Path,sha:&str)->Result<Vec<u8>,String> {
    let data=bytes(&crate::plugin_catalog::cache_path(root,sha)?,MAX_FILE)?;
    if digest(&data)!=sha{return Err("完整插件包摘要不符，未加载或覆盖原文件".into());}Ok(data)
}
fn entry(archive:&mut zip::ZipArchive<std::io::Cursor<&[u8]>>,name:&str,limit:u64)->Result<Vec<u8>,String> {
    if archive.file_names().filter(|n|*n==name).count()!=1{return Err("插件入口缺失或重复".into());}
    let mut file=archive.by_name(name).map_err(|_|"无法打开插件入口")?;
    if file.size()>limit{return Err("插件入口超过上限".into());}
    let size=file.size();let mut out=Vec::new();file.by_ref().take(limit+1).read_to_end(&mut out).map_err(|_|"插件入口 CRC 检查失败")?;
    if out.len() as u64!=size{return Err("插件入口长度不符".into());}Ok(out)
}
fn put(dir:&Path,name:&str,data:&[u8],files:&mut BTreeMap<String,String>)->Result<(),String> {
    let path=dir.join(name);if let Some(parent)=path.parent(){std::fs::create_dir_all(parent).map_err(|_|"无法创建插件资源目录")?;}
    std::fs::write(&path,data).map_err(|_|"无法保存运行文件")?;files.insert(name.into(),digest(data));Ok(())
}
fn verify_pair(root:&Path,p:&Profile)->Result<(Verification,Verification),String> {
    let plugin=plugin_signature::verify_archive(&crate::plugin_catalog::cache_path(root,p.sha)?,root)?;
    let sdk=plugin_signature::verify_archive(&crate::plugin_catalog::cache_path(root,SDK_SHA)?,root)?;
    if plugin.package_sha256!=p.sha || sdk.package_sha256!=SDK_SHA{return Err("插件或 SDK 完整摘要与兼容记录不符".into());}
    Ok((plugin,sdk))
}
fn runtime_dir(root:&Path,p:&Profile)->PathBuf {root.join("plugin-runtimes-v2").join(p.sha)}
fn check_runtime_location(root:&Path,p:&Profile,allow_missing:bool)->Result<PathBuf,String> {
    let parent=root.join("plugin-runtimes-v2");let directory=runtime_dir(root,p);
    for path in [root,&parent,&directory] {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir()&&!is_reparse(&metadata)=>(),
            Ok(_)=>return Err("插件运行目录不是普通目录".into()),
            Err(error) if allow_missing&&error.kind()==std::io::ErrorKind::NotFound=>(),
            Err(_)=>return Err("无法检查插件运行目录".into()),
        }
    }
    Ok(directory)
}
fn staged_bytes(directory:&Path,name:&str)->Result<Vec<u8>,String> {
    let mut path=directory.to_path_buf();
    for part in Path::new(name).components() {
        let std::path::Component::Normal(part)=part else{return Err("运行文件路径无效".into());};
        path.push(part);
        let metadata=std::fs::symlink_metadata(&path).map_err(|_|"运行文件缺失")?;
        if is_reparse(&metadata){return Err("插件运行路径包含重解析点".into());}
    }
    bytes(&path,MAX_FILE)
}
fn validate_staged(directory:&Path,inventory:&Files,expected:BTreeMap<String,Vec<u8>>)->Result<(),String> {
    if inventory.schema!=1 || inventory.files.len()>4096{return Err("运行文件清单无效".into());}
    if inventory.files.len()!=expected.len(){return Err("运行文件清单缺失".into());}
    for (name,data) in expected {
        let expected_hash=digest(&data);
        if inventory.files.get(&name)!=Some(&expected_hash)||digest(&staged_bytes(directory,&name)?)!=expected_hash {
            return Err("运行文件已改变，未启动；请重新准备插件".into());
        }
    }
    Ok(())
}
fn sdk_cached(root:&Path,p:&Profile)->Result<bool,String> {
    // cache_path intentionally requires a file to exist. Determine absence only
    // after resolving the known plugin in the same checked quarantine directory.
    let plugin=crate::plugin_catalog::cache_path(root,p.sha)?;
    let sdk=plugin.with_file_name(format!("{SDK_SHA}.plugin-cache"));
    match std::fs::symlink_metadata(sdk) {
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>Ok(false),
        Err(_)=>Err("无法检查共享 SDK 缓存".into()),
        Ok(_)=>{crate::plugin_catalog::cache_path(root,SDK_SHA)?;Ok(true)}
    }
}

/// Reverify imported trust, both archive signatures, and every staged byte at launch.
pub fn validate(root:&Path,model:&str,report:&DownloadReport)->Result<Prepared,String> {
    let p=profile(model,report)?;let (package_signature,sdk_signature)=verify_pair(root,p)?;
    let directory=check_runtime_location(root,p,false)?;
    let raw=bytes(&directory.join("prepared-files.json"),512*1024)?;
    let inventory:Files=serde_json::from_slice(&raw).map_err(|_|"运行文件清单损坏，请重新准备")?;
    // The inventory is a local corruption detector, never a source of executable trust.
    let plugin=snapshot(root,p.sha)?;let sdk=snapshot(root,SDK_SHA)?;
    let expected=expected_files(p,&plugin,&sdk)?;
    validate_staged(&directory,&inventory,expected)?;
    Ok(Prepared{model:model.into(),version:p.version,sha256:p.sha.into(),sdk_sha256:SDK_SHA.into(),package_signature,sdk_signature,directory})
}

fn expected_files(p:&Profile,plugin:&[u8],sdk:&[u8])->Result<BTreeMap<String,Vec<u8>>,String> {
    let mut plugin=zip::ZipArchive::new(std::io::Cursor::new(plugin)).map_err(|_|"插件 ZIP 损坏")?;
    let mut sdk=zip::ZipArchive::new(std::io::Cursor::new(sdk)).map_err(|_|"SDK ZIP 损坏")?;
    let project:Value=serde_json::from_slice(&entry(&mut plugin,"android/project.json",64*1024)?).map_err(|_|"项目清单损坏")?;
    if project["package_path"].as_str()!=Some(p.package)||project["version"].as_str()!=Some(p.display_version)||project["platform"]!="android" {return Err("项目身份与兼容记录不符".into());}
    let mut result=BTreeMap::new();result.insert("switch.bundle".into(),entry(&mut plugin,"android/main.bundle",64*1024*1024)?);
    result.insert("sdk.bundle".into(),entry(&mut sdk,"android/sdk.bundle",64*1024*1024)?);
    let mut selected=BTreeMap::<String,(f64,String)>::new();
    for index in 0..plugin.len() {
        let file=plugin.by_index(index).map_err(|_|"资源目录损坏")?;let path=file.name().to_owned();let parts:Vec<_>=path.split('/').collect();
        if parts.len()!=3||parts[0]!="android"||file.enclosed_name().is_none(){continue;}
        let scale=match parts[1]{"drawable-mdpi"=>1.,"drawable-hdpi"=>1.5,"drawable-xhdpi"=>2.,"drawable-xxhdpi"=>3.,"drawable-xxxhdpi"=>4.,_=>continue};
        let name=parts[2];if !name.bytes().all(|c|c.is_ascii_lowercase()||c.is_ascii_digit()||c==b'_'||c==b'.')||![".png",".jpg",".jpeg",".webp"].iter().any(|ext|name.ends_with(ext)){continue;}
        if selected.get(name).is_none_or(|(old,_)|scale>*old){selected.insert(name.into(),(scale,path));}
    }
    let mut map=serde_json::Map::new();let mut total=0usize;
    for (name,(scale,path)) in selected {
        let data=entry(&mut plugin,&path,4*1024*1024)?;total+=data.len();if total>32*1024*1024{return Err("图片资源超过限制".into());}
        result.insert(format!("images/{name}"),data);map.insert(name.clone(),json!({"name":name,"scale":scale}));
    }
    result.insert("local-assets.json".into(),serde_json::to_vec(&map).map_err(|_|"资源清单失败")?);
    result.insert("runtime-manifest.json".into(),serde_json::to_vec(&json!({"schema":1,"kind":"downloaded-rn","package":p.package,"version":p.display_version,"sdkApiLevel":10116,"entryModule":10001})).map_err(|_|"运行清单失败")?);
    Ok(result)
}

pub async fn prepare(cloud:&Cloud,region:&str,root:&Path,model:&str,report:&DownloadReport)->Result<Prepared,String> {
    let p=profile(model,report)?;plugin_signature::trust_info(root)?;
    if !sdk_cached(root,p)? {
        let sdk=crate::plugin_download::download_sdk(cloud,region,10116,root).await?;
        if sdk.sha256!=SDK_SHA{return Err("SDK 已更新，已下载但暂未开放此版本运行".into());}
    }
    prepare_local(root,model,report)
}
pub fn prepare_local(root:&Path,model:&str,report:&DownloadReport)->Result<Prepared,String> {
    let p=profile(model,report)?;verify_pair(root,p)?;
    let directory=check_runtime_location(root,p,true)?;
    if directory.exists(){if let Ok(prepared)=validate(root,model,report){return Ok(prepared);}}
    let plugin=snapshot(root,p.sha)?;let sdk=snapshot(root,SDK_SHA)?;
    let files=expected_files(p,&plugin,&sdk)?;
    let parent=directory.parent().ok_or("运行目录无效")?;std::fs::create_dir_all(parent).map_err(|_|"无法创建运行目录")?;
    let stage=parent.join(format!("prepare-{}-{:016x}",std::process::id(),rand::random::<u64>()));
    std::fs::create_dir(&stage).map_err(|_|"无法创建准备目录")?;
    let mut inventory=BTreeMap::new();
    for (name,data) in files {put(&stage,&name,&data,&mut inventory)?;}
    std::fs::write(stage.join("prepared-files.json"),serde_json::to_vec(&Files{schema:1,files:inventory}).map_err(|_|"无法保存清单")?).map_err(|_|"无法保存清单")?;
    // Repair only this derived runtime. Keep the previous contents recoverable,
    // and commit the fully prepared replacement before allowing a new launch.
    let previous=if directory.exists() {
        check_runtime_location(root,p,false)?;
        let previous=parent.join(format!("previous-{}-{:016x}",p.sha,rand::random::<u64>()));
        std::fs::rename(&directory,&previous).map_err(|_|"运行文件正在使用，未覆盖原目录；请先关闭插件")?;
        Some(previous)
    }else{None};
    if std::fs::rename(&stage,&directory).is_err() {
        if let Some(previous)=previous {let _=std::fs::rename(previous,&directory);}
        return Err("无法提交运行目录，原文件保留".into());
    }
    validate(root,model,report)
}

#[cfg(test)] mod tests {
    use super::*;
    use std::io::{Cursor,Write};

    struct Temp(PathBuf);
    impl Temp {
        fn new()->Self {
            let path=std::env::temp_dir().join(format!("HomePanel-prepare-test-{}-{:016x}",std::process::id(),rand::random::<u64>()));
            std::fs::create_dir(&path).unwrap();Self(path)
        }
        fn quarantine(&self)->PathBuf {
            let path=self.0.join("plugin-downloads-v1").join("quarantine");std::fs::create_dir_all(&path).unwrap();path
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let base=std::env::temp_dir();
            if self.0.parent()==Some(base.as_path()) && self.0.file_name().is_some_and(|name|name.to_string_lossy().starts_with("HomePanel-prepare-test-")) {
                let _=std::fs::remove_dir_all(&self.0);
            }
        }
    }
    fn archive(entries:&[(&str,&[u8])])->Vec<u8> {
        let mut archive=zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name,data) in entries {archive.start_file(*name,options).unwrap();archive.write_all(data).unwrap();}
        archive.finish().unwrap().into_inner()
    }
    fn project(p:&Profile)->Value {json!({"package_path":p.package,"version":p.display_version,"platform":"android","signature":2048})}
    fn package(p:&Profile,project:&Value,main:&[u8])->Vec<u8> {
        let project=serde_json::to_vec(project).unwrap();
        let _=p;
        archive(&[("android/project.json",&project),("android/main.bundle",main),
            ("android/drawable-mdpi/power.png",b"low-density-image"),
            ("android/drawable-xxxhdpi/power.png",b"high-density-image"),
            ("android/drawable-xxhdpi/status.webp",b"status-image"),
            ("android/drawable-xxxhdpi/ignored.svg",b"not-an-image-type"),
            ("android/drawable-xhdpi/NON_LOWER.png",b"ignored-name")])
    }
    fn prepared_fixture(p:&Profile)->BTreeMap<String,Vec<u8>> {
        expected_files(p,&package(p,&project(p),b"fixture-main-suffix"),&archive(&[("android/sdk.bundle",b"fixture-sdk")])).unwrap()
    }
    fn stage(directory:&Path,expected:&BTreeMap<String,Vec<u8>>)->Files {
        let mut files=BTreeMap::new();
        for (name,data) in expected {put(directory,name,data,&mut files).unwrap();}
        Files{schema:1,files}
    }
    #[test] fn model_version_and_full_hash_are_all_required() {
        let mut r=DownloadReport{bytes:1,version:31,sdk_version:Some(10116),sha256:PROFILES[1].sha.into(),format:BundleFormat::IndexedRam,archive_crc_verified:true,declared_size_verified:true,publisher_signature_verified:true,can_execute:true};
        assert!(profile(PROFILES[1].model,&r).is_ok());
        assert!(profile(PROFILES[0].model,&r).is_err());
        r.sha256.replace_range(0..1,"0");assert!(profile(PROFILES[1].model,&r).is_err());
        r.sha256=PROFILES[1].sha.into();r.sdk_version=Some(10117);assert!(profile(PROFILES[1].model,&r).is_err());
    }
    #[test] fn a_digest_prefix_is_never_a_run_grant_and_versions_formats_must_match() {
        let p=&PROFILES[1];
        let mut r=DownloadReport{bytes:1,version:p.version,sdk_version:Some(10116),sha256:p.sha.into(),
            format:BundleFormat::IndexedRam,archive_crc_verified:true,declared_size_verified:true,publisher_signature_verified:true,can_execute:true};
        r.sha256.truncate(63);assert!(profile(p.model,&r).is_err());
        r.sha256=p.sha.into();r.sha256.replace_range(63..64,if &p.sha[63..64]=="0"{"1"}else{"0"});
        assert!(profile(p.model,&r).is_err());
        r.sha256=p.sha.into();r.version+=1;assert!(profile(p.model,&r).is_err());
        r.version=p.version;r.format=BundleFormat::Javascript;assert!(profile(p.model,&r).is_err());
    }
    #[test] fn complete_snapshot_pin_covers_bytes_after_the_declared_signature_prefix() {
        let root=Temp::new();let p=&PROFILES[1];let mut main=vec![b'x';2048];main.extend_from_slice(b"unsigned-suffix-a");
        let original=package(p,&project(p),&main);let full_sha=digest(&original);
        let path=root.quarantine().join(format!("{full_sha}.plugin-cache"));std::fs::write(&path,&original).unwrap();
        assert_eq!(snapshot(&root.0,&full_sha).unwrap(),original);
        let mut changed=main.clone();*changed.last_mut().unwrap()=b'b';assert_eq!(&changed[..2048],&main[..2048]);
        let modified=package(p,&project(p),&changed);assert_eq!(modified.len(),original.len());
        std::fs::write(&path,&modified).unwrap();assert!(snapshot(&root.0,&full_sha).is_err());
    }
    #[test] fn runtime_manifest_and_resource_identity_are_derived_from_the_pinned_package() {
        let p=&PROFILES[1];let expected=prepared_fixture(p);
        assert_eq!(expected.get("images/power.png").unwrap(),b"high-density-image");
        assert_eq!(expected.get("images/status.webp").unwrap(),b"status-image");
        assert!(!expected.contains_key("images/ignored.svg"));assert!(!expected.contains_key("images/NON_LOWER.png"));
        let assets:Value=serde_json::from_slice(&expected["local-assets.json"]).unwrap();
        assert_eq!(assets["power.png"],json!({"name":"power.png","scale":4.0}));
        let manifest:Value=serde_json::from_slice(&expected["runtime-manifest.json"]).unwrap();
        assert_eq!(manifest,json!({"schema":1,"kind":"downloaded-rn","package":p.package,"version":p.display_version,"sdkApiLevel":10116,"entryModule":10001}));
        let sdk=archive(&[("android/sdk.bundle",b"fixture-sdk")]);
        for (field,value) in [("package_path",json!("unrelated.package")),("version",json!("999.0")),("platform",json!("ios"))] {
            let mut wrong=project(p);wrong[field]=value;
            assert!(expected_files(p,&package(p,&wrong,b"main"),&sdk).is_err());
        }
    }
    #[test] fn staged_tampering_cannot_be_authorized_by_rewriting_the_local_inventory() {
        let root=Temp::new();let expected=prepared_fixture(&PROFILES[1]);let mut inventory=stage(&root.0,&expected);
        assert!(validate_staged(&root.0,&inventory,expected.clone()).is_ok());
        for name in ["switch.bundle","sdk.bundle","runtime-manifest.json","local-assets.json","images/power.png"] {
            let original=&expected[name];let mut changed=original.clone();changed.push(b' ');
            std::fs::write(root.0.join(name),&changed).unwrap();
            inventory.files.insert(name.into(),digest(&changed));
            assert!(validate_staged(&root.0,&inventory,expected.clone()).is_err(),"modified {name} was accepted");
            std::fs::write(root.0.join(name),original).unwrap();inventory.files.insert(name.into(),digest(original));
        }
        inventory.files.remove("sdk.bundle");assert!(validate_staged(&root.0,&inventory,expected.clone()).is_err());
        inventory.files.insert("sdk.bundle".into(),digest(&expected["sdk.bundle"]));inventory.schema=2;
        assert!(validate_staged(&root.0,&inventory,expected).is_err());
    }
    #[test] fn missing_sdk_is_a_download_condition_but_invalid_cached_objects_are_errors() {
        let root=Temp::new();let p=&PROFILES[1];let quarantine=root.quarantine();
        std::fs::write(quarantine.join(format!("{}.plugin-cache",p.sha)),b"nonexecuted-fixture").unwrap();
        assert!(!sdk_cached(&root.0,p).unwrap());
        let sdk=quarantine.join(format!("{SDK_SHA}.plugin-cache"));std::fs::create_dir(&sdk).unwrap();
        assert!(sdk_cached(&root.0,p).is_err());
        std::fs::remove_dir(&sdk).unwrap();std::fs::write(&sdk,b"nonexecuted-sdk-fixture").unwrap();
        assert!(sdk_cached(&root.0,p).unwrap());
    }
    #[test] fn launch_paths_reject_non_directory_roots_and_parent_traversal() {
        let root=Temp::new();let p=&PROFILES[1];
        assert!(check_runtime_location(&root.0,p,true).is_ok());
        assert!(check_runtime_location(&root.0,p,false).is_err());
        std::fs::write(root.0.join("plugin-runtimes-v2"),b"not-a-directory").unwrap();
        assert!(check_runtime_location(&root.0,p,true).is_err());
        assert!(staged_bytes(&root.0,"../outside.bundle").is_err());
        assert!(bytes(&root.0,MAX_FILE).is_err());
    }
}
