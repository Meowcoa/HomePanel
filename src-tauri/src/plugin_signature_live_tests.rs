//! Opt-in verification against the user-provided APK and public plugin cache.
//! Does not load sessions, call the network, execute JavaScript, or control devices.
use super::*;
use std::io::Write;

fn rewrite_one(source:&Path,destination:&Path,target:&str,mutate:impl FnOnce(&mut Vec<u8>)) {
    let mut input=zip::ZipArchive::new(std::fs::File::open(source).unwrap()).unwrap();
    let mut output=zip::ZipWriter::new(std::fs::File::create(destination).unwrap());
    let mut mutate=Some(mutate);let mut found=false;
    for index in 0..input.len() {
        let mut entry=input.by_index(index).unwrap();
        if entry.name()!=target {output.raw_copy_file(entry).unwrap();continue;}
        assert!(!found);found=true;
        let mut bytes=Vec::new();entry.read_to_end(&mut bytes).unwrap();
        mutate.take().unwrap()(&mut bytes);
        output.start_file(target,zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
        output.write_all(&bytes).unwrap();
    }
    assert!(found);output.finish().unwrap();
}

#[test]
#[ignore="requires explicit HOMEPANEL_SIGNATURE_LIVE_APK and HOMEPANEL_SIGNATURE_LIVE_PACKAGE_DIR public sample paths"]
fn pinned_public_packages_verify_and_tampering_is_rejected() {
    let apk=PathBuf::from(std::env::var_os("HOMEPANEL_SIGNATURE_LIVE_APK").expect("set explicit public APK sample path"));
    let packages=PathBuf::from(std::env::var_os("HOMEPANEL_SIGNATURE_LIVE_PACKAGE_DIR").expect("set explicit public plugin sample directory"));
    let work=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("work");
    let directory=work.join(format!("HomePanel-signature-live-{}-{}",std::process::id(),crate::now_ms()));
    std::fs::create_dir(&directory).unwrap();
    let profile=directory.join("profile");
    let trust=import_root_from_apk(&apk,&profile).unwrap();
    assert_eq!(trust.root_sha256,ROOT_SHA256);assert!(!trust.independent_publisher_identity_verified);
    let samples=[
        ("717ebfe4d1946268056994e44ec623e15f2b77f0141a1c1df4f13035f57943f5",1_048_576,3_011_157,"android/main.bundle"),
        ("21c96ea1499d37ec10ebdc71ad51d2476fb525d46107eba2ce333c7f55c11976",830_635,830_635,"android/main.bundle"),
        ("1e89d8a439ddc8a4327e96caa23f4e135f596081d46857cab16d71cf712710a2",1_048_576,12_402_758,"android/sdk.bundle"),
    ];
    let mut reports=Vec::new();
    for (digest,signed,total,bundle_path) in samples {
        let package=packages.join(format!("{digest}.plugin-cache"));
        let report=verify_archive(&package,&profile).unwrap();
        assert_eq!(report.package_sha256,digest);assert_eq!(report.signed_bundle_bytes,signed);assert_eq!(report.total_bundle_bytes,total);
        assert!(report.apk_root_signature_verified&&report.certificate_chain_verified&&report.certificate_time_valid);
        assert!(!report.revocation_checked);assert!(!report.independent_publisher_identity_verified);
        for (case,target) in [("project","android/project.json"),("prefix",bundle_path),("signature","android/bundle.sign"),("certificate","android/bundle.cert")] {
            let changed=directory.join(format!("{digest}-{case}.zip"));
            rewrite_one(&package,&changed,target,|bytes|match case {
                "project"=>bytes.push(b' '),
                "prefix"=>bytes[0]^=1,
                "signature"=>{let last=bytes.len()-1;bytes[last]^=1;},
                "certificate"=>bytes.clear(),
                _=>unreachable!(),
            });
            assert!(verify_archive(&changed,&profile).is_err(),"tampered {case} must fail");
        }
        if signed<total {
            let changed=directory.join(format!("{digest}-unsigned-suffix.zip"));
            rewrite_one(&package,&changed,bundle_path,|bytes|bytes[signed as usize]^=1);
            let suffix_report=verify_archive(&changed,&profile).unwrap();
            // APK signing covers only the prefix. The parent launch gate MUST
            // compare the complete package digest to the audited package pin.
            assert_eq!(suffix_report.signed_bundle_sha256,report.signed_bundle_sha256);
            assert_ne!(suffix_report.bundle_sha256,report.bundle_sha256);
            assert_ne!(suffix_report.package_sha256,report.package_sha256);
        }
        reports.push(report);
    }
    let root=root_path(&profile);let original=std::fs::read(&root).unwrap();let mut corrupt=original.clone();corrupt[0]^=1;
    std::fs::write(&root,corrupt).unwrap();assert!(trust_info(&profile).is_err());std::fs::write(&root,original).unwrap();
    std::fs::write(directory.join("verification-facts-rust.json"),serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    println!("Public signature and tamper evidence: {}",directory.display());
}

#[test]
#[ignore="requires explicit public APK/plugin paths; stages verified packages without executing them"]
fn pinned_public_packages_prepare_validate_resources_and_reject_staged_tampering() {
    use crate::{plugin_catalog::{self,PackageKind},plugin_download::DownloadReport,plugin_prepare,plugin_probe::BundleFormat};
    let apk=PathBuf::from(std::env::var_os("HOMEPANEL_SIGNATURE_LIVE_APK").expect("set explicit public APK sample path"));
    let packages=PathBuf::from(std::env::var_os("HOMEPANEL_SIGNATURE_LIVE_PACKAGE_DIR").expect("set explicit public plugin sample directory"));
    let work=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("work");
    let root=work.join(format!("HomePanel-prepare-live-{}-{}",std::process::id(),crate::now_ms()));
    std::fs::create_dir(&root).unwrap();import_root_from_apk(&apk,&root).unwrap();
    let quarantine=root.join("plugin-downloads-v1/quarantine");std::fs::create_dir_all(&quarantine).unwrap();
    let mut reports=Vec::new();
    for (sha,version,kind,model,expected_bytes) in [
        (plugin_prepare::PROFILES[0].sha,25,PackageKind::Plugin,Some(plugin_prepare::PROFILES[0].model),3_803_341),
        (plugin_prepare::PROFILES[1].sha,31,PackageKind::Plugin,Some(plugin_prepare::PROFILES[1].model),1_036_829),
        (plugin_prepare::SDK_SHA,1,PackageKind::Sdk,None,3_131_835),
    ] {
        let source=packages.join(format!("{sha}.plugin-cache"));let destination=quarantine.join(format!("{sha}.plugin-cache"));
        assert_eq!(std::fs::copy(&source,&destination).unwrap(),expected_bytes);
        let report=DownloadReport{bytes:expected_bytes,version,sdk_version:Some(10116),sha256:sha.into(),format:BundleFormat::IndexedRam,
            archive_crc_verified:true,declared_size_verified:true,publisher_signature_verified:false,can_execute:false};
        plugin_catalog::record(&root,kind,model,&report).unwrap();reports.push(report);
    }
    let catalog=plugin_catalog::list(&root).unwrap();assert_eq!(catalog.len(),3);
    assert_eq!(catalog.iter().filter(|entry|entry.kind==Some(PackageKind::Plugin)).count(),2);
    assert!(catalog.iter().all(|entry|entry.cache_available&&!entry.report.as_ref().unwrap().can_execute));
    let mut facts=Vec::new();
    for (index,expected_images) in [(0,276usize),(1,73usize)] {
        let p=&plugin_prepare::PROFILES[index];let report=&reports[index];
        let prepared=plugin_prepare::prepare_local(&root,p.model,report).unwrap();
        assert_eq!(prepared.model,p.model);assert_eq!(prepared.sha256,p.sha);assert_eq!(prepared.version,p.version);
        assert_eq!(prepared.sdk_sha256,plugin_prepare::SDK_SHA);
        let validated=plugin_prepare::validate(&root,p.model,report).unwrap();assert_eq!(validated.directory,prepared.directory);
        // Calling prepare again must reuse and revalidate the same prepared files.
        assert_eq!(plugin_prepare::prepare_local(&root,p.model,report).unwrap().directory,prepared.directory);
        let manifest:serde_json::Value=serde_json::from_slice(&std::fs::read(prepared.directory.join("runtime-manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["package"],p.package);assert_eq!(manifest["version"],p.display_version);assert_eq!(manifest["sdkApiLevel"],10116);
        assert_eq!(manifest["entryModule"],10001);assert_eq!(manifest["kind"],"downloaded-rn");
        let assets:serde_json::Value=serde_json::from_slice(&std::fs::read(prepared.directory.join("local-assets.json")).unwrap()).unwrap();
        assert_eq!(assets.as_object().unwrap().len(),expected_images);
        assert_eq!(std::fs::read_dir(prepared.directory.join("images")).unwrap().count(),expected_images);
        let inventory_path=prepared.directory.join("prepared-files.json");let original_inventory=std::fs::read(&inventory_path).unwrap();
        let mut inventory:serde_json::Value=serde_json::from_slice(&original_inventory).unwrap();
        assert_eq!(inventory["files"].as_object().unwrap().len(),expected_images+4);
        let stage=prepared.directory.join("switch.bundle");let original=std::fs::read(&stage).unwrap();let mut changed=original.clone();changed.push(b' ');
        std::fs::write(&stage,&changed).unwrap();assert!(plugin_prepare::validate(&root,p.model,report).is_err());
        // A mutable local inventory cannot grant trust to a modified executable.
        inventory["files"]["switch.bundle"]=format!("{:x}",Sha256::digest(&changed)).into();
        std::fs::write(&inventory_path,serde_json::to_vec(&inventory).unwrap()).unwrap();assert!(plugin_prepare::validate(&root,p.model,report).is_err());
        let repaired=plugin_prepare::prepare_local(&root,p.model,report).unwrap();assert_eq!(repaired.directory,prepared.directory);
        assert_eq!(std::fs::read(&stage).unwrap(),original);plugin_prepare::validate(&root,p.model,report).unwrap();
        let prefix=format!("previous-{}-",p.sha);
        let backups:Vec<_>=std::fs::read_dir(prepared.directory.parent().unwrap()).unwrap().map(|entry|entry.unwrap().path())
            .filter(|path|path.file_name().unwrap().to_string_lossy().starts_with(&prefix)).collect();
        assert_eq!(backups.len(),1);assert_eq!(std::fs::read(backups[0].join("switch.bundle")).unwrap(),changed);
        facts.push(serde_json::json!({"model":p.model,"packageSha256":p.sha,"sdkSha256":plugin_prepare::SDK_SHA,
            "pluginVersion":p.display_version,"sdkApiLevel":10116,"uniqueImages":expected_images,"runtimeFiles":expected_images+4,
            "preparePassed":true,"validatePassed":true,"stagedTamperRejected":true,"rewrittenInventoryRejected":true,
            "stageRepairPassed":true,"previousStagePreserved":true,"pluginExecuted":false}));
    }
    std::fs::write(root.join("prepare-validation-facts.json"),serde_json::to_vec_pretty(&facts).unwrap()).unwrap();
    println!("Public prepare and staged-tamper evidence: {}",root.display());
}
