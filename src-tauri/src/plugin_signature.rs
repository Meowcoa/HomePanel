//! Independent RN package verifier using a user-imported, pinned APK public root.
//! The certificate is never installed into Windows trust stores or redistributed.
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, io::Read, path::{Path, PathBuf}};

pub const SOURCE_APK_SHA256: &str = "73335d3c17dff3e73097db7e2f86a373b7eb2c67611dc82749883ab2da3f0b6e";
pub const ROOT_SHA256: &str = "0efa59bd969170149136a494c735d2fabad3bddf28345c0d3435e9b337be8bbe";
const MAX_CERTIFICATE: u64 = 64 * 1024;
const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_BUNDLE: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct TrustRootInfo {
    pub root_sha256: String,
    pub source_apk_sha256: String,
    pub independent_publisher_identity_verified: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Verification {
    pub package_sha256: String,
    pub project_sha256: String,
    pub bundle_sha256: String,
    pub signed_bundle_sha256: String,
    pub signed_bundle_bytes: u64,
    pub total_bundle_bytes: u64,
    pub root_sha256: String,
    pub apk_root_signature_verified: bool,
    pub certificate_chain_verified: bool,
    pub certificate_time_valid: bool,
    pub revocation_checked: bool,
    pub independent_publisher_identity_verified: bool,
}

fn root_path(app_root: &Path) -> PathBuf {
    app_root.join("plugin-trust-v1").join(format!("{ROOT_SHA256}.der"))
}

fn hash_file(path: &Path, max: u64) -> Result<String, String> {
    let file=std::fs::File::open(path).map_err(|_|"无法打开待校验文件")?;
    let meta=file.metadata().map_err(|_|"无法检查待校验文件")?;
    if !meta.is_file() || meta.len()>max {return Err("待校验文件大小无效".into());}
    let mut file=file.take(max+1);let mut hasher=Sha256::new();let mut buffer=[0u8;16384];let mut count=0u64;
    loop {let read=file.read(&mut buffer).map_err(|_|"无法读取待校验文件")?;if read==0{break;}count+=read as u64;
        if count>max{return Err("待校验文件超过上限".into());}hasher.update(&buffer[..read]);}
    Ok(format!("{:x}",hasher.finalize()))
}

fn root_bytes(app_root: &Path) -> Result<Vec<u8>, String> {
    let path=root_path(app_root);
    let file=std::fs::File::open(path).map_err(|_|"尚未导入本机 APK 的公开验签根证书，请先导入米家 APK")?;
    if file.metadata().map_err(|_|"无法检查本机验签根")?.len()>MAX_CERTIFICATE{return Err("本机验签根文件过大".into());}
    let mut bytes=Vec::new();file.take(MAX_CERTIFICATE+1).read_to_end(&mut bytes).map_err(|_|"无法读取本机验签根")?;
    if bytes.len() as u64>MAX_CERTIFICATE || format!("{:x}",Sha256::digest(&bytes))!=ROOT_SHA256 {
        return Err("本机验签根与已核对的 APK 样本不符，未信任此文件".into());
    }
    Ok(bytes)
}

pub fn trust_info(app_root: &Path) -> Result<TrustRootInfo, String> {
    root_bytes(app_root)?;
    Ok(TrustRootInfo{root_sha256:ROOT_SHA256.into(),source_apk_sha256:SOURCE_APK_SHA256.into(),independent_publisher_identity_verified:false})
}

fn read_entry<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>, name: &str, limit: u64) -> Result<Vec<u8>, String> {
    if archive.file_names().filter(|file|*file==name).count()!=1{return Err("验签文件缺失或存在重复入口".into());}
    let mut entry=archive.by_name(name).map_err(|_|"无法打开验签文件")?;
    if entry.size()>limit{return Err("验签文件超过大小上限".into());}
    let mut bytes=Vec::new();entry.by_ref().take(limit+1).read_to_end(&mut bytes).map_err(|_|"验签文件解压或 CRC 检查失败")?;
    if bytes.len() as u64>limit || bytes.len() as u64!=entry.size(){return Err("验签文件实际大小不符".into());}
    Ok(bytes)
}

/// Import only the public root from the exact, user-provided APK sample.
/// This establishes local APK-root trust, not independent publisher identity.
pub fn import_root_from_apk(apk: &Path, app_root: &Path) -> Result<TrustRootInfo, String> {
    if hash_file(apk,1024*1024*1024)?!=SOURCE_APK_SHA256{return Err("APK 版本不属于已核对的验签样本，未导入根证书".into());}
    let file=std::fs::File::open(apk).map_err(|_|"无法打开本机 APK")?;
    let mut archive=zip::ZipArchive::new(file).map_err(|_|"本机 APK 结构损坏")?;
    let bytes=read_entry(&mut archive,"assets/MijiaRootCert.der",MAX_CERTIFICATE)?;
    if format!("{:x}",Sha256::digest(&bytes))!=ROOT_SHA256{return Err("APK 公开根指纹不符，未导入".into());}
    system::check_root(&bytes)?;
    let path=root_path(app_root);
    if path.exists(){return trust_info(app_root);}
    crate::storage::atomic_write(&path,&bytes)?;
    trust_info(app_root)
}

fn certificates(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let text=std::str::from_utf8(bytes).map_err(|_|"插件证书不是已识别的 PEM 格式")?;
    const BEGIN:&str="-----BEGIN CERTIFICATE-----";
    const END:&str="-----END CERTIFICATE-----";
    let mut rest=text.trim();let mut result=Vec::new();
    while !rest.is_empty() {
        let body=rest.strip_prefix(BEGIN).ok_or("插件证书存在未识别的数据")?;
        let (encoded,tail)=body.split_once(END).ok_or("插件证书 PEM 未闭合")?;
        let encoded:String=encoded.chars().filter(|c|c.is_ascii_whitespace()==false).collect();
        let der=BASE64.decode(encoded).map_err(|_|"插件证书 PEM 编码无效")?;
        if der.is_empty() || der.len() as u64>MAX_CERTIFICATE{return Err("插件证书大小无效".into());}
        result.push(der);if result.len()>2{return Err("插件证书链层数尚未适配".into());}
        rest=tail.trim();
    }
    if result.is_empty(){return Err("插件没有提供证书".into());}
    Ok(result)
}

fn signed_length(project: &[u8], total: usize) -> Result<usize, String> {
    let json:Value=serde_json::from_slice(project).map_err(|_|"插件项目清单无效")?;
    if !json.is_object(){return Err("插件项目清单不是对象".into());}
    let length=match json.get("signature") {
        None=>None,
        Some(value)=>Some(value.as_u64().ok_or("插件签名范围不是有效整数")?),
    };
    Ok(length.filter(|length|*length>1024 && *length<(total as u64)).map(|length|length as usize).unwrap_or(total))
}

fn signature_hash(project: &[u8], bundle: &[u8], count: usize) -> [u8;32] {
    let mut message=Sha256::new();message.update(Sha256::digest(project));message.update(Sha256::digest(&bundle[..count]));
    message.finalize().into()
}

fn der_len(bytes: &[u8], cursor: &mut usize) -> Result<usize, String> {
    let head=*bytes.get(*cursor).ok_or("ECDSA 签名长度缺失")?;*cursor+=1;
    if head<128{return Ok(head as usize);}
    let count=(head&127) as usize;
    if count==0 || count>2{return Err("ECDSA 签名长度无效".into());}
    let mut value=0usize;
    for index in 0..count {let byte=*bytes.get(*cursor).ok_or("ECDSA 签名长度截断")?;*cursor+=1;
        if index==0 && byte==0{return Err("ECDSA 签名长度不是规范 DER".into());}value=(value<<8)|(byte as usize);}
    if value<128{return Err("ECDSA 签名使用非规范长长度".into());}Ok(value)
}

fn ecdsa_der_to_raw(signature: &[u8], width: usize) -> Result<Vec<u8>, String> {
    if !matches!(width,32|48|66) || signature.len()>144 || signature.first()!=Some(&0x30){return Err("ECDSA 签名格式无效".into());}
    let mut cursor=1usize;let length=der_len(signature,&mut cursor)?;
    if cursor.checked_add(length)!=Some(signature.len()){return Err("ECDSA 签名存在尾随或截断数据".into());}
    let mut raw=vec![0u8;width*2];
    for index in 0..2 {
        if signature.get(cursor)!=Some(&0x02){return Err("ECDSA 签名分量无效".into());}cursor+=1;
        let length=der_len(signature,&mut cursor)?;let end=cursor.checked_add(length).ok_or("ECDSA 签名分量过大")?;
        let mut integer=signature.get(cursor..end).filter(|v|!v.is_empty()).ok_or("ECDSA 签名分量截断")?;
        if integer[0]&0x80!=0{return Err("ECDSA 签名分量为负数".into());}
        if integer[0]==0 && integer.len()>1 {
            if integer[1]&0x80==0{return Err("ECDSA 签名分量不是规范 DER".into());}integer=&integer[1..];
        }
        if integer.len()>width{return Err("ECDSA 签名分量超过曲线长度".into());}
        let start=index*width+width-integer.len();raw[start..index*width+width].copy_from_slice(integer);cursor=end;
    }
    if cursor!=signature.len(){return Err("ECDSA 签名分量数量无效".into());}Ok(raw)
}

/// Every launch should rerun this against the exact package being launched.
/// Successful verification does not sign unsigned assets or the bundle suffix.
pub fn verify_archive(package: &Path, app_root: &Path) -> Result<Verification, String> {
    let root=root_bytes(app_root)?;
    let file=std::fs::File::open(package).map_err(|_|"无法打开插件包")?;
    let metadata=file.metadata().map_err(|_|"无法检查插件包")?;
    if !metadata.is_file() || metadata.len()>MAX_ARCHIVE{return Err("插件包大小无效".into());}
    // Hash and parse the same bounded snapshot; reopening a path after hashing
    // would allow a replacement between the package pin and signature checks.
    let mut snapshot=Vec::new();file.take(MAX_ARCHIVE+1).read_to_end(&mut snapshot).map_err(|_|"无法读取插件包")?;
    if snapshot.len() as u64>MAX_ARCHIVE || snapshot.len() as u64!=metadata.len(){return Err("插件包读取期间大小发生变化".into());}
    let package_sha256=format!("{:x}",Sha256::digest(&snapshot));
    let mut archive=zip::ZipArchive::new(std::io::Cursor::new(snapshot)).map_err(|_|"插件包不是有效 ZIP")?;
    if archive.len()>4096{return Err("插件包文件数量超过上限".into());}
    let mut names=HashSet::new();let mut total=0u64;
    for index in 0..archive.len() {
        let entry=archive.by_index(index).map_err(|_|"插件包目录损坏")?;
        if entry.enclosed_name().is_none() || entry.name().contains(['\\',':']) || !names.insert(entry.name().to_lowercase())
            || entry.unix_mode().is_some_and(|mode|mode&0o170000==0o120000) || (entry.is_dir()&&entry.size()!=0) {
            return Err("插件包包含不安全或重复的路径".into());
        }
        total=total.checked_add(entry.size()).ok_or("插件展开大小无效")?;
        if total>256*1024*1024{return Err("插件包展开大小超过上限".into());}
    }
    let android=archive.file_names().any(|name|name=="android/project.json");
    if android && archive.file_names().any(|name|name=="project.json"){return Err("插件包包含歧义项目入口".into());}
    let prefix=if android{"android/"}else{""};
    let project=read_entry(&mut archive,&format!("{prefix}project.json"),64*1024)?;
    let certs=certificates(&read_entry(&mut archive,&format!("{prefix}bundle.cert"),MAX_CERTIFICATE)?)?;
    let signature=read_entry(&mut archive,&format!("{prefix}bundle.sign"),1024)?;
    let main=format!("{prefix}main.bundle");let sdk=format!("{prefix}sdk.bundle");
    let has_main=archive.file_names().any(|name|name==main);let has_sdk=archive.file_names().any(|name|name==sdk);
    if has_main && has_sdk{return Err("插件包包含歧义 bundle 入口".into());}
    let bundle=read_entry(&mut archive,if has_main{&main}else{&sdk},MAX_BUNDLE)?;
    let count=signed_length(&project,bundle.len())?;
    system::verify(&root,&certs,&signature,&signature_hash(&project,&bundle,count))?;
    Ok(Verification{package_sha256,project_sha256:format!("{:x}",Sha256::digest(&project)),bundle_sha256:format!("{:x}",Sha256::digest(&bundle)),
        signed_bundle_sha256:format!("{:x}",Sha256::digest(&bundle[..count])),signed_bundle_bytes:count as u64,total_bundle_bytes:bundle.len() as u64,
        root_sha256:ROOT_SHA256.into(),apk_root_signature_verified:true,certificate_chain_verified:true,certificate_time_valid:true,
        revocation_checked:false,independent_publisher_identity_verified:false})
}

#[cfg(windows)]
mod system {
    use super::*;
    use std::{mem::size_of,ptr::{null,null_mut}};
    use windows_sys::Win32::Security::Cryptography::*;
    struct Certificate(*mut CERT_CONTEXT);
    impl Certificate {
        fn new(bytes:&[u8])->Result<Self,String>{let value=unsafe{CertCreateCertificateContext(X509_ASN_ENCODING,bytes.as_ptr(),bytes.len() as u32)};
            if value.is_null(){Err("无法解码插件 X509 证书".into())}else{Ok(Self(value))}}
    }
    impl Drop for Certificate {fn drop(&mut self){unsafe{CertFreeCertificateContext(self.0);}}}
    struct Store(HCERTSTORE);
    impl Store {
        fn new()->Result<Self,String>{let value=unsafe{CertOpenStore(CERT_STORE_PROV_MEMORY,0,0,CERT_STORE_CREATE_NEW_FLAG,null())};
            if value.is_null(){Err("无法初始化离线证书存储".into())}else{Ok(Self(value))}}
        fn add(&self,bytes:&[u8])->Result<(),String>{if unsafe{CertAddEncodedCertificateToStore(self.0,X509_ASN_ENCODING,bytes.as_ptr(),bytes.len() as u32,CERT_STORE_ADD_ALWAYS,null_mut())}==0{
            Err("无法载入离线证书链".into())}else{Ok(())}}
    }
    impl Drop for Store {fn drop(&mut self){unsafe{CertCloseStore(self.0,0);}}}
    struct ChainEngine(HCERTCHAINENGINE);
    impl Drop for ChainEngine {fn drop(&mut self){unsafe{CertFreeCertificateChainEngine(self.0);}}}
    struct Chain(*mut CERT_CHAIN_CONTEXT);
    impl Drop for Chain {fn drop(&mut self){unsafe{CertFreeCertificateChain(self.0);}}}
    struct Key(BCRYPT_KEY_HANDLE);
    impl Drop for Key {fn drop(&mut self){unsafe{BCryptDestroyKey(self.0);}}}

    pub(super) fn check_root(bytes:&[u8])->Result<(),String>{
        let root=Certificate::new(bytes)?;
        unsafe {
            if CertVerifyTimeValidity(null(),(*root.0).pCertInfo)!=0{return Err("本机 APK 的公开根证书不在有效期内".into());}
            if CryptVerifyCertificateSignatureEx(0,X509_ASN_ENCODING,CRYPT_VERIFY_CERT_SIGN_SUBJECT_CERT,root.0.cast(),CRYPT_VERIFY_CERT_SIGN_ISSUER_CERT,root.0.cast(),CRYPT_VERIFY_CERT_SIGN_DISABLE_MD2_MD4_FLAG,null_mut())==0 {
                return Err("本机 APK 公开根证书的自签名无效".into());
            }
        }
        Ok(())
    }

    fn signature_matches(cert:&Certificate,signature:&[u8],hash:&[u8;32])->Result<bool,String>{
        let mut key=null_mut();
        if unsafe{CryptImportPublicKeyInfoEx2(X509_ASN_ENCODING,&(*(*cert.0).pCertInfo).SubjectPublicKeyInfo,0,null(),&mut key)}==0{return Ok(false);}
        let key=Key(key);let mut length=0u32;let mut written=0u32;
        if unsafe{BCryptGetProperty(key.0,BCRYPT_SIGNATURE_LENGTH,(&mut length as *mut u32).cast(),size_of::<u32>() as u32,&mut written,0)}<0
            || written!=size_of::<u32>() as u32 || !matches!(length,64|96|132) {return Ok(false);}
        let signature=ecdsa_der_to_raw(signature,(length/2) as usize)?;
        Ok(unsafe{BCryptVerifySignature(key.0,null(),hash.as_ptr(),hash.len() as u32,signature.as_ptr(),signature.len() as u32,0)}>=0)
    }

    fn chain_valid(root:&[u8],certs:&[Vec<u8>],leaf:&Certificate)->Result<(),String>{
        let roots=Store::new()?;roots.add(root)?;
        let additional=Store::new()?;for cert in certs{additional.add(cert)?;}
        let config=CERT_CHAIN_ENGINE_CONFIG{cbSize:size_of::<CERT_CHAIN_ENGINE_CONFIG>() as u32,hExclusiveRoot:roots.0,
            dwUrlRetrievalTimeout:1,MaximumCachedCertificates:8,..Default::default()};
        let mut engine=null_mut();
        if unsafe{CertCreateCertificateChainEngine(&config,&mut engine)}==0{return Err(format!("无法初始化本机 APK 根证书链校验（Windows {}）",unsafe{windows_sys::Win32::Foundation::GetLastError()}));}
        let engine=ChainEngine(engine);let para=CERT_CHAIN_PARA{cbSize:size_of::<CERT_CHAIN_PARA>() as u32,dwUrlRetrievalTimeout:1,..Default::default()};
        let flags=CERT_CHAIN_CACHE_ONLY_URL_RETRIEVAL|CERT_CHAIN_DISABLE_AIA|CERT_CHAIN_DISABLE_AUTH_ROOT_AUTO_UPDATE;
        let mut chain=null_mut();
        if unsafe{CertGetCertificateChain(engine.0,leaf.0,null(),additional.0,&para,flags,null(),&mut chain)}==0{return Err("无法建立插件离线证书链".into());}
        let chain=Chain(chain);
        let policy=CERT_CHAIN_POLICY_PARA{cbSize:size_of::<CERT_CHAIN_POLICY_PARA>() as u32,..Default::default()};
        let mut status=CERT_CHAIN_POLICY_STATUS{cbSize:size_of::<CERT_CHAIN_POLICY_STATUS>() as u32,..Default::default()};
        if unsafe{CertVerifyCertificateChainPolicy(CERT_CHAIN_POLICY_BASE,chain.0,&policy,&mut status)}==0 || status.dwError!=0 {
            return Err("插件证书链或有效期校验未通过".into());
        }
        unsafe {
            if (*chain.0).cChain!=1 || (*chain.0).rgpChain.is_null(){return Err("插件证书链结构无效".into());}
            let simple=*(*chain.0).rgpChain;
            if simple.is_null() || (*simple).cElement<2 || (*simple).cElement>4 || (*simple).rgpElement.is_null(){return Err("插件证书链层数无效".into());}
            let last=*(*simple).rgpElement.add((*simple).cElement as usize-1);
            if last.is_null() || (*last).pCertContext.is_null(){return Err("插件证书链缺少根证书".into());}
            let cert=(*last).pCertContext;
            let bytes=std::slice::from_raw_parts((*cert).pbCertEncoded,(*cert).cbCertEncoded as usize);
            if format!("{:x}",Sha256::digest(bytes))!=ROOT_SHA256{return Err("插件证书没有链到本机已核对的 APK 根".into());}
        }
        Ok(())
    }

    pub(super) fn verify(root:&[u8],certs:&[Vec<u8>],signature:&[u8],hash:&[u8;32])->Result<(),String>{
        check_root(root)?;
        for bytes in certs {
            let cert=Certificate::new(bytes)?;
            if signature_matches(&cert,signature,hash)? {return chain_valid(root,certs,&cert);}
        }
        Err("插件内容签名校验未通过，不能运行".into())
    }
}

#[cfg(not(windows))]
mod system {
    pub(super) fn check_root(_: &[u8])->Result<(),String>{Err("插件验签目前仅支持 Windows 系统加密接口".into())}
    pub(super) fn verify(_: &[u8],_: &[Vec<u8>],_: &[u8],_: &[u8;32])->Result<(),String>{Err("插件验签目前仅支持 Windows 系统加密接口".into())}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn ecdsa_der_enforces_canonical_integers_and_exact_length(){
        let der=[0x30,6,2,1,1,2,1,2];let raw=ecdsa_der_to_raw(&der,32).unwrap();assert_eq!(raw[31],1);assert_eq!(raw[63],2);
        for bad in [vec![],vec![0x30,6,2,1,0x80,2,1,2],vec![0x30,7,2,2,0,1,2,1,2],vec![0x30,0x81,6,2,1,1,2,1,2],vec![0x30,6,2,1,1,2,1,2,0]] {
            assert!(ecdsa_der_to_raw(&bad,32).is_err());
        }
        assert!(ecdsa_der_to_raw(&der,16).is_err());
    }
    #[test] fn signed_range_does_not_claim_unsigned_suffix_and_hashes_raw_manifest(){
        let project=br#"{"signature":2048}"#;let mut bundle=vec![7u8;4096];let count=signed_length(project,bundle.len()).unwrap();assert_eq!(count,2048);
        let before=signature_hash(project,&bundle,count);bundle[3000]^=1;assert_eq!(signature_hash(project,&bundle,count),before);
        bundle[0]^=1;assert_ne!(signature_hash(project,&bundle,count),before);
        assert_ne!(signature_hash(br#"{ "signature":2048}"#,&bundle,count),signature_hash(project,&bundle,count));
        for project in [br#"{"signature":1024}"#.as_slice(),br#"{"signature":4096}"#,br#"{}"#]{assert_eq!(signed_length(project,4096).unwrap(),4096);}
        for project in [br#"{"signature":-1}"#.as_slice(),br#"{"signature":"2048"}"#,br#"[]"#]{assert!(signed_length(project,4096).is_err());}
    }
    #[test] fn malformed_or_extra_pem_data_is_never_silently_trusted(){
        assert!(certificates(b"").is_err());assert!(certificates(b"not a cert").is_err());
        assert!(certificates(b"-----BEGIN CERTIFICATE-----\n!\n-----END CERTIFICATE-----").is_err());
        assert!(certificates(b"-----BEGIN CERTIFICATE-----\nMAA=\n-----END CERTIFICATE-----tail").is_err());
        assert_eq!(certificates(b"-----BEGIN CERTIFICATE-----\nMAA=\n-----END CERTIFICATE-----").unwrap(),vec![vec![0x30,0]]);
    }
    #[test] fn wrong_apk_is_rejected_before_any_root_file_can_be_created(){
        let work=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("work");
        let directory=work.join(format!("HomePanel-signature-negative-{}-{}",std::process::id(),crate::now_ms()));
        std::fs::create_dir(&directory).unwrap();let apk=directory.join("synthetic.apk");std::fs::write(&apk,b"synthetic APK; no certificate").unwrap();
        let destination=directory.join("profile");assert!(import_root_from_apk(&apk,&destination).is_err());assert!(!destination.exists());
    }
}

#[cfg(all(test,windows))]
#[path="plugin_signature_live_tests.rs"]
mod live_tests;
