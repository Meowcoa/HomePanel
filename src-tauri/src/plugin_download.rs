//! Independently authored model-based plugin retrieval. Download is not execution.
use crate::cloud::Cloud;
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{io::{Read,Write},path::{Path,PathBuf},time::Duration};

const MAX_DOWNLOAD:u64=128*1024*1024;
const MAX_EXPANDED:u64=256*1024*1024;

struct Candidate {url:url::Url,size:u64,version:u64,sdk_version:Option<u64>}

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct DownloadReport {
    pub bytes:u64,pub version:u64,pub sdk_version:Option<u64>,pub sha256:String,
    pub format:crate::plugin_probe::BundleFormat,pub archive_crc_verified:bool,
    pub declared_size_verified:bool,pub publisher_signature_verified:bool,pub can_execute:bool,
}

fn model_valid(model:&str)->bool {
    model.len()<=160 && model.split('.').count()>=3 && model.split('.').all(|part|!part.is_empty()
        && part.bytes().all(|c|c.is_ascii_lowercase()||c.is_ascii_digit()||c==b'_'))
}

pub async fn query(cloud:&Cloud,region:&str,model:&str)->Result<Value,String> {
    if !model_valid(model){return Err("设备型号格式无效".into());}
    if region!="cn"{return Err("官方插件查询目前只验证中国大陆服务区".into());}
    // Protocol facts independently checked against the supplied Android APK.
    cloud.query_plugin_metadata(region,json!({
        "latest_req":{"api_version":10117,"app_platform":"Android","region":"CN","plugins":[{"model":model}],"package_type":""},
        "backup_req":{"api_level":111,"app_platform":"phone","plugins":[{"model":model}]},"is_car_mihome":false
    })).await
}

pub async fn query_sdk(cloud:&Cloud,region:&str,version:u64)->Result<Value,String> {
    if region!="cn" || version==0 || version>100_000{return Err("SDK 服务区或版本尚未验证".into());}
    cloud.query_plugin_sdk(region,json!({"platform":"Android","sdk_version":[version]})).await
}

fn cdn_url(raw:&str)->Result<url::Url,String> {
    let url=url::Url::parse(raw).map_err(|_|"插件下载地址无效")?;
    let host=url.host_str().unwrap_or_default();
    if url.scheme()!="https" || url.port_or_known_default()!=Some(443) || !url.username().is_empty() || url.password().is_some()
        || url.fragment().is_some() || !(host=="iot-cdn.io.mi.com" || host.ends_with(".fds.api.mi-img.com") || host.ends_with(".fds.api.xiaomi.com")) {
        return Err("插件下载地址不在已核对的小米 HTTPS CDN 范围内".into());
    }
    Ok(url)
}

fn candidate(result:&Value,model:&str)->Result<Candidate,String> {
    if !model_valid(model){return Err("设备型号格式无效".into());}
    // Prefer the current RN record; old Android-native packages are only a
    // download fallback, never reinterpreted as Windows-compatible plugins.
    for (field,size_key) in [("latest_info","size"),("backup_info","length")] {
        let Some(rows)=result.get(field).filter(|v|!v.is_null())else{continue;};
        let rows=rows.as_array().ok_or("插件目录结构无效")?;
        let matched:Vec<_>=rows.iter().filter(|row|row["model"].as_str()==Some(model)).collect();
        if matched.is_empty(){continue;}
        if matched.len()!=1{return Err("同一型号返回多个插件，暂不自动选择".into());}
        let row=matched[0];
        let size=row[size_key].as_u64().filter(|size|*size>0&&*size<=MAX_DOWNLOAD).ok_or("插件未提供有效大小或超过 128 MiB")?;
        let version=row["version"].as_u64().filter(|v|*v>0).ok_or("插件版本无效")?;
        let url=row.get("safe_url").and_then(Value::as_str).filter(|s|!s.is_empty()).or_else(||row["download_url"].as_str()).ok_or("插件未提供下载地址")?;
        return Ok(Candidate{url:cdn_url(url)?,size,version,sdk_version:row["sdk_version"].as_u64()});
    }
    Err("米家没有返回这个型号的可下载插件".into())
}

fn sdk_candidate(result:&Value,requested:u64)->Result<Candidate,String> {
    let rows=result["info"].as_array().ok_or("SDK 目录结构无效")?;
    let matched:Vec<_>=rows.iter().filter(|row|row["sdk_version"].as_u64()==Some(requested)).collect();
    if matched.len()!=1{return Err("对应 SDK 缺失或目录存在歧义".into());}
    let row=matched[0];
    let size=row["size"].as_u64().filter(|v|*v>0&&*v<=MAX_DOWNLOAD).ok_or("SDK 大小无效或超过上限")?;
    let version=row["version"].as_u64().filter(|v|*v>0).ok_or("SDK 版本无效")?;
    let url=row["download_url"].as_str().ok_or("SDK 未提供下载地址")?;
    Ok(Candidate{url:cdn_url(url)?,size,version,sdk_version:Some(requested)})
}

struct PendingFile(PathBuf,bool);
impl Drop for PendingFile {fn drop(&mut self){if self.1{let _=std::fs::remove_file(&self.0);}}}

fn inspect_archive(path:&Path)->Result<(crate::plugin_probe::BundleFormat,bool),String> {
    use crate::plugin_probe::BundleFormat;
    let file=std::fs::File::open(path).map_err(|_|"无法检查下载文件")?;
    let mut header=[0u8;4];let mut probe=file.try_clone().map_err(|_|"无法检查下载文件")?;
    let count=probe.read(&mut header).map_err(|_|"无法检查下载文件")?;
    if count!=4 || !header.starts_with(b"PK"){return Ok((BundleFormat::Unknown,false));}
    // A cloned file shares its position on Windows. Reopen at the beginning.
    let file=std::fs::File::open(path).map_err(|_|"无法检查下载文件")?;
    let mut archive=zip::ZipArchive::new(file).map_err(|_|"插件 ZIP 目录损坏")?;
    if archive.len()>4096{return Err("插件文件数量超过检查限制".into());}
    let mut names=std::collections::HashSet::new();let mut total=0u64;let mut format=BundleFormat::Unknown;let mut has_entry=false;
    for index in 0..archive.len() {
        let mut entry=archive.by_index(index).map_err(|_|"插件 ZIP 条目损坏")?;
        let name=entry.name().to_owned();
        if entry.enclosed_name().is_none() || name.contains(['\\',':']) || !names.insert(name.to_lowercase())
            || entry.unix_mode().is_some_and(|mode|mode&0o170000==0o120000) {
            return Err("插件包含不安全或重复的文件路径".into());
        }
        total=total.checked_add(entry.size()).ok_or("插件解压大小无效")?;
        if total>MAX_EXPANDED{return Err("插件展开内容超过 256 MiB 检查限制".into());}
        if entry.is_dir() && entry.size()!=0{return Err("插件目录项包含非空内容".into());}
        let mut head=[0u8;256];let mut used=0usize;let mut actual=0u64;let mut buffer=[0u8;16384];
        loop {
            let count=entry.read(&mut buffer).map_err(|_|"插件 ZIP CRC 校验失败或内容损坏")?;
            if count==0{break;}
            actual=actual.saturating_add(count as u64);
            if actual>entry.size() || actual>MAX_EXPANDED{return Err("插件展开内容大小不符".into());}
            let keep=count.min(head.len()-used);head[used..used+keep].copy_from_slice(&buffer[..keep]);used+=keep;
        }
        if actual!=entry.size(){return Err("插件 ZIP 内容长度不符".into());}
        if matches!(name.as_str(),"android/main.bundle"|"main.bundle"|"android/sdk.bundle"|"sdk.bundle") {
            if has_entry{return Err("插件存在多个主入口".into());}has_entry=true;
            format=crate::plugin_probe::identify(&head[..used]);
        }
    }
    Ok((format,true))
}

async fn receive(mut response:reqwest::Response,source:&Candidate,root:&Path)->Result<DownloadReport,String> {
    if !response.status().is_success(){return Err("官方插件 CDN 未接受下载请求，请重新查询".into());}
    if response.content_length().is_some_and(|size|size!=source.size){return Err("插件下载长度与目录不符".into());}
    let directory=root.join("plugin-downloads-v1").join("quarantine");
    std::fs::create_dir_all(&directory).map_err(|_|"无法创建本机插件缓存")?;
    let mut pending=PendingFile(directory.join(format!("pending-{}-{:016x}.part",std::process::id(),rand::random::<u64>())),false);
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&pending.0).map_err(|_|"无法创建插件下载临时文件")?;
    pending.1=true;
    let mut digest=Sha256::new();let mut bytes=0u64;
    while let Some(chunk)=response.chunk().await.map_err(|_|"插件下载中断，请重试")? {
        bytes=bytes.saturating_add(chunk.len() as u64);
        if bytes>source.size || bytes>MAX_DOWNLOAD{return Err("插件下载超过声明大小".into());}
        file.write_all(&chunk).map_err(|_|"无法保存插件下载")?;digest.update(&chunk);
    }
    if bytes!=source.size{return Err("插件下载不完整，请重试".into());}
    file.sync_all().map_err(|_|"无法提交插件下载")?;drop(file);
    let sha256=format!("{:x}",digest.finalize());let version=source.version;let sdk_version=source.sdk_version;
    tokio::task::spawn_blocking(move||{
        let (format,archive_crc_verified)=inspect_archive(&pending.0)?;
        let destination=directory.join(format!("{sha256}.plugin-cache"));
        if destination.exists() {
            let mut existing=std::fs::File::open(&destination).map_err(|_|"无法检查已有插件缓存")?;
            if existing.metadata().map_err(|_|"无法检查已有插件缓存")?.len()!=bytes{return Err("已有缓存长度不符，原文件未覆盖".into());}
            let mut hash=Sha256::new();let mut buffer=[0u8;16384];
            loop {let read=existing.read(&mut buffer).map_err(|_|"无法检查已有插件缓存")?;if read==0{break;}hash.update(&buffer[..read]);}
            if format!("{:x}",hash.finalize())!=sha256{return Err("已有缓存校验失败，原文件未覆盖".into());}
        }else{
            std::fs::rename(&pending.0,&destination).map_err(|_|"无法保存插件缓存；未覆盖原文件")?;
        }
        // CRC/SHA256 verify local integrity. The separate RN certificate chain
        // is not implemented, so this cache never becomes an executable source.
        Ok(DownloadReport{bytes,version,sdk_version,sha256,format,archive_crc_verified,declared_size_verified:true,publisher_signature_verified:false,can_execute:false})
    }).await.map_err(|_|"插件检查任务未完成")?
}

fn download_client()->Result<reqwest::Client,String> {
    reqwest::Client::builder().timeout(Duration::from_secs(90)).connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none()).user_agent(concat!("HomePanel/",env!("CARGO_PKG_VERSION"))).build().map_err(|_|"无法初始化插件下载".into())
}
pub async fn download(cloud:&Cloud,region:&str,model:&str,root:&Path)->Result<DownloadReport,String> {
    let data=query(cloud,region,model).await?;let source=candidate(&data,model)?;
    let client=download_client()?;
    // Separate client, no account cookies, Authorization, Referer or redirects.
    let response=client.get(source.url.clone()).header("Accept-Encoding","identity").send().await.map_err(|_|"无法连接官方插件 CDN")?;
    let report=receive(response,&source,root).await?;
    crate::plugin_catalog::record(root,crate::plugin_catalog::PackageKind::Plugin,Some(model),&report)?;
    Ok(report)
}

pub async fn download_sdk(cloud:&Cloud,region:&str,version:u64,root:&Path)->Result<DownloadReport,String> {
    let data=query_sdk(cloud,region,version).await?;let source=sdk_candidate(&data,version)?;
    let response=download_client()?.get(source.url.clone()).header("Accept-Encoding","identity").send().await.map_err(|_|"无法连接官方 SDK CDN")?;
    let report=receive(response,&source,root).await?;
    crate::plugin_catalog::record(root,crate::plugin_catalog::PackageKind::Sdk,None,&report)?;
    Ok(report)
}

#[cfg(debug_assertions)]
pub fn run(args:&[String])->Option<i32> {
    if args.get(1).map(String::as_str)==Some("--download-plugin-sdk-probe") {
        let result=(||{
            let version=args.get(2).and_then(|v|v.parse::<u64>().ok()).ok_or("invalid_version")?;
            let root=std::env::var_os("APPDATA").map(PathBuf::from).ok_or("environment")?.join("io.github.meowcoa.homepanel");
            let cache=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or("environment")?.join("io.github.meowcoa.homepanel");
            let cloud=Cloud::new(root.clone(),cache).map_err(|_|"initialization")?;
            let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_|"runtime")?;
            runtime.block_on(download_sdk(&cloud,"cn",version,&root))
        })();
        match result {Ok(report)=>{println!("{}",serde_json::to_string(&report).unwrap());return Some(0)},Err(code)=>{println!("{}",json!({"error":code}));return Some(2)}}
    }
    if args.get(1).map(String::as_str)==Some("--query-plugin-sdk-probe") {
        let result=(||{
            let version=args.get(2).and_then(|v|v.parse::<u64>().ok()).ok_or("invalid_version")?;
            let root=std::env::var_os("APPDATA").map(PathBuf::from).ok_or("environment")?.join("io.github.meowcoa.homepanel");
            let cache=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or("environment")?.join("io.github.meowcoa.homepanel");
            let cloud=Cloud::new(root,cache).map_err(|_|"initialization")?;
            let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_|"runtime")?;
            let data=runtime.block_on(query_sdk(&cloud,"cn",version)).map_err(|_|"sdk_query_failed")?;
            fn shape(v:&Value)->Value {match v {Value::Object(m)=>Value::Object(m.iter().map(|(k,v)|(k.clone(),shape(v))).collect()),Value::Array(a)=>json!({"array":true,"count":a.len(),"sample":a.first().map(shape)}),Value::String(_)=>json!({"type":"string"}),Value::Number(_)=>json!({"type":"number"}),Value::Bool(_)=>json!({"type":"bool"}),Value::Null=>Value::Null}}
            Ok::<_,&str>(shape(&data))
        })();
        match result {Ok(report)=>{println!("{}",report);return Some(0)},Err(code)=>{println!("{}",json!({"error":code}));return Some(2)}}
    }
    let download_requested=match args.get(1).map(String::as_str){Some("--query-plugin-probe")=>false,Some("--download-plugin-probe")=>true,_=>return None};
    let result=(||{
        if args.len()!=3 || !model_valid(&args[2]){return Err("invalid_arguments");}
        let root=std::env::var_os("APPDATA").map(std::path::PathBuf::from).ok_or("environment")?.join("io.github.meowcoa.homepanel");
        let cache=std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).ok_or("environment")?.join("io.github.meowcoa.homepanel");
        let cloud=Cloud::new(root.clone(),cache).map_err(|_|"initialization")?;
        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_|"runtime")?;
        if download_requested {
            let report=runtime.block_on(download(&cloud,"cn",&args[2],&root)).map_err(|error|if error.contains("AUTH_REQUIRED"){"authentication"}else if error.contains("CDN"){"cdn_failed"}else if error.contains("ZIP"){"archive_failed"}else{"download_failed"})?;
            return serde_json::to_value(report).map_err(|_|"report_failed");
        }
        let result=runtime.block_on(query(&cloud,"cn",&args[2])).map_err(|error|if error.contains("AUTH_REQUIRED"){"authentication"}else{"query_failed"})?;
        // Output only shape, never response strings, identities or signed URLs.
        fn shape(value:&Value,depth:usize)->Value {
            if depth>3{return json!({"present":true});}
            match value {
                Value::Object(map)=>Value::Object(map.iter().filter(|(key,_)|[
                    "latest_info","backup_info","plugins","model","plugin_id","package_id","version","package_name","length","type","api_level",
                    "download_url","safe_url","md5","sha256","sign","signature","new_version","force","change_log","min_sdk_api_level","sdk_api_level",
                    "url","package_size","size","package_md5","checksum","hash","rn_version","sdk_version","app_platform","package_type","plugin_package_id",
                    "stand_plugin_info","increment_plugin_info","full_define_plugin_info"
                ].contains(&key.as_str())).map(|(key,value)|{
                    let mut result=shape(value,depth+1);
                    if matches!(key.as_str(),"download_url"|"safe_url") {
                        if let Some(raw)=value.as_str(){if let Ok(url)=url::Url::parse(raw){
                            let host=url.host_str().unwrap_or_default();
                            result["https"]=json!(url.scheme()=="https");result["http"]=json!(url.scheme()=="http");
                            result["known_fds_host"]=json!(host.ends_with(".fds.api.mi-img.com")||host.ends_with(".fds.api.xiaomi.com"));
                            result["cdn_host"]=json!(host);
                            result["has_userinfo"]=json!(!url.username().is_empty()||url.password().is_some());
                        }}
                    }
                    (key.clone(),result)
                }).collect()),
                Value::Array(rows)=>json!({"is_array":true,"has_rows":!rows.is_empty(),"sample":rows.first().map(|v|shape(v,depth+1))}),
                Value::Null=>json!({"present":false}),Value::String(text)=>json!({"is_string":true,"nonempty":!text.is_empty()}),
                Value::Number(_)=>json!({"is_number":true}),Value::Bool(_)=>json!({"is_bool":true}),
            }
        }
        Ok(shape(&result,0))
    })();
    match result {Ok(report)=>{println!("{}",json!({"schema":"homepanel_plugin_probe_v1","read_only":!download_requested,"device_writes":false,"metadata":report}));Some(0)},
        Err(code)=>{println!("{}",json!({"schema":"homepanel_plugin_probe_v1","read_only":!download_requested,"device_writes":false,"error":code}));Some(2)}}
}

#[cfg(test)]
#[path="plugin_download_tests.rs"]
mod regression_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn only_bounded_model_identifiers_are_accepted() {
        assert!(model_valid("xiaomi.wifispeaker.l05c"));
        for model in ["","one.two","../model","xiaomi..model","xiaomi.model.X","xiaomi.model.name?x=y","xiaomi.model.name\n"] {
            assert!(!model_valid(model));
        }
    }
}
