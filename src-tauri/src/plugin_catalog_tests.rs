//! Offline synthetic files only. No sessions, network, plugin execution, or devices.
use super::*;
use crate::plugin_probe::BundleFormat;
use std::{io::{Cursor,Write},sync::atomic::{AtomicU64,Ordering}};

static NEXT:AtomicU64=AtomicU64::new(0);
fn root(tag:&str)->PathBuf {
    let work=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("work");
    let path=work.join(format!("HomePanel-catalog-test-{tag}-{}-{}-{}",std::process::id(),crate::now_ms(),NEXT.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir(&path).unwrap();path
}
fn fixture(root:&Path,tag:&str)->DownloadReport{
    let mut archive=zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    archive.start_file("android/project.json",options).unwrap();
    archive.write_all(br#"{"package_path":"homepanel.fixture","version":"1.0","model":"never.guess.this","download_url":"https://fixture.invalid/not-for-catalog"}"#).unwrap();
    archive.start_file("android/main.bundle",options).unwrap();archive.write_all(tag.as_bytes()).unwrap();
    let bytes=archive.finish().unwrap().into_inner();let sha256=format!("{:x}",Sha256::digest(&bytes));
    let directory=root.join("plugin-downloads-v1/quarantine");std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join(format!("{sha256}.plugin-cache")),&bytes).unwrap();
    DownloadReport{bytes:bytes.len() as u64,version:17,sdk_version:Some(10116),sha256,format:BundleFormat::Unknown,
        archive_crc_verified:true,declared_size_verified:true,publisher_signature_verified:true,can_execute:true}
}
fn write_catalog(root:&Path,catalog:&serde_json::Value){std::fs::write(catalog_path(root).unwrap(),serde_json::to_vec(catalog).unwrap()).unwrap();}

#[test]
fn records_survive_reload_and_execution_flags_are_never_trusted(){
    let root=root("roundtrip");let report=fixture(&root,"one");
    let recorded=record(&root,PackageKind::Plugin,Some("homepanel.fixture.device"),&report).unwrap();
    assert!(recorded.cache_available);assert!(!recorded.report.as_ref().unwrap().can_execute);
    let bytes=std::fs::read(catalog_path(&root).unwrap()).unwrap();let text=std::str::from_utf8(&bytes).unwrap();
    assert!(!text.contains("download_url")&&!text.contains("fixture.invalid")&&!text.contains("never.guess.this"));
    let mut json:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
    json["entries"][0]["report"]["canExecute"]=true.into();json["entries"][0]["report"]["publisherSignatureVerified"]=true.into();write_catalog(&root,&json);
    let entries=list(&root).unwrap();assert_eq!(entries.len(),1);assert_eq!(entries[0].model.as_deref(),Some("homepanel.fixture.device"));
    assert!(entries[0].cache_available);let report=entries[0].report.as_ref().unwrap();assert!(!report.can_execute&&!report.publisher_signature_verified);
    assert_eq!(entries[0].public_project.as_ref().unwrap().package_path.as_deref(),Some("homepanel.fixture"));
}

#[test]
fn pre_catalog_files_are_visible_but_never_associated_by_guessing(){
    let root=root("legacy");let report=fixture(&root,"old package");
    std::fs::write(root.join("plugin-downloads-v1/quarantine/pending-123.part"),b"unfinished").unwrap();
    let entries=list(&root).unwrap();assert_eq!(entries.len(),1);let entry=&entries[0];
    assert_eq!(entry.sha256,report.sha256);assert!(entry.kind.is_none()&&entry.model.is_none()&&entry.report.is_none()&&entry.downloaded_at_ms.is_none());
    assert!(entry.cache_available);assert_eq!(entry.public_project.as_ref().unwrap().version.as_deref(),Some("1.0"));
    assert!(!catalog_path(&root).unwrap().exists(),"reading an old cache must not manufacture persisted download evidence");
}

#[test]
fn record_validates_hash_model_kind_and_retains_previous_catalog_on_error(){
    let root=root("reject");let report=fixture(&root,"valid");
    record(&root,PackageKind::Sdk,None,&report).unwrap();let before=std::fs::read(catalog_path(&root).unwrap()).unwrap();
    for (kind,model) in [(PackageKind::Plugin,None),(PackageKind::Plugin,Some("bad/../model")),(PackageKind::Sdk,Some("homepanel.fixture.device"))] {
        assert!(record(&root,kind,model,&report).is_err());
    }
    let mut wrong=report.clone();wrong.bytes+=1;assert!(record(&root,PackageKind::Sdk,None,&wrong).is_err());
    let path=cache_path(&root,&report.sha256).unwrap();let mut bytes=std::fs::read(&path).unwrap();bytes[0]^=1;std::fs::write(&path,bytes).unwrap();
    assert!(record(&root,PackageKind::Sdk,None,&report).is_err());assert_eq!(std::fs::read(catalog_path(&root).unwrap()).unwrap(),before);
    for sha in ["../escape",&"A".repeat(64),&"0/".repeat(32),""]{assert!(cache_path(&root,sha).is_err());}
}

#[test]
fn schema_duplicates_path_fields_and_size_limits_fail_closed(){
    let root=root("malformed");let report=fixture(&root,"valid");record(&root,PackageKind::Sdk,None,&report).unwrap();
    let original:serde_json::Value=serde_json::from_slice(&std::fs::read(catalog_path(&root).unwrap()).unwrap()).unwrap();
    let mut version=original.clone();version["schema"]=2.into();write_catalog(&root,&version);assert!(list(&root).is_err());
    let mut duplicate=original.clone();duplicate["entries"].as_array_mut().unwrap().push(original["entries"][0].clone());write_catalog(&root,&duplicate);assert!(list(&root).is_err());
    let mut excessive=original.clone();excessive["entries"]=vec![original["entries"][0].clone();MAX_ENTRIES+1].into();write_catalog(&root,&excessive);assert!(list(&root).is_err());
    let mut path=original.clone();path["entries"][0]["path"]="C:/outside.plugin-cache".into();write_catalog(&root,&path);assert!(list(&root).is_err());
    let mut mismatch=original.clone();mismatch["entries"][0]["report"]["sha256"]="0".repeat(64).into();write_catalog(&root,&mismatch);assert!(list(&root).is_err());
    std::fs::write(catalog_path(&root).unwrap(),vec![b' ';MAX_CATALOG as usize+1]).unwrap();assert!(list(&root).is_err());
    std::fs::write(catalog_path(&root).unwrap(),b"broken JSON").unwrap();assert!(record(&root,PackageKind::Sdk,None,&report).is_err());
    assert_eq!(std::fs::read(catalog_path(&root).unwrap()).unwrap(),b"broken JSON");
}

#[test]
fn parallel_records_do_not_lose_entries_and_repeated_records_are_deduplicated(){
    let root=root("parallel");let reports:(Vec<_>,Vec<_>)=(0..8).map(|index|{
        let report=fixture(&root,&format!("package {index}"));let root=root.clone();let expected=report.sha256.clone();
        let handle=std::thread::spawn(move||record(&root,PackageKind::Plugin,Some("homepanel.fixture.device"),&report).unwrap());(handle,expected)
    }).unzip();
    for handle in reports.0{handle.join().unwrap();}
    let entries=list(&root).unwrap();assert_eq!(entries.len(),8);for expected in reports.1{assert!(entries.iter().any(|entry|entry.sha256==expected));}
    let entry=&entries[0];record(&root,PackageKind::Plugin,entry.model.as_deref(),entry.report.as_ref().unwrap()).unwrap();assert_eq!(list(&root).unwrap().len(),8);
}

#[test]
fn missing_cache_keeps_a_visible_non_executable_record(){
    let root=root("missing");let report=fixture(&root,"valid");record(&root,PackageKind::Sdk,None,&report).unwrap();
    // Rename only this test-owned file; do not delete any user's cached plugin.
    std::fs::rename(cache_path(&root,&report.sha256).unwrap(),root.join("test-owned-cache-backup.bin")).unwrap();
    let entries=list(&root).unwrap();assert_eq!(entries.len(),1);assert!(!entries[0].cache_available);assert!(!entries[0].report.as_ref().unwrap().can_execute);
}
