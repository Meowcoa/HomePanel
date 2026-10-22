//! Synthetic model metadata, ZIP fixtures and loopback HTTP only.
use super::*;
use crate::plugin_probe::BundleFormat;
use std::{
    io::{BufRead, BufReader, Cursor},
    net::TcpListener,
    sync::{atomic::{AtomicU64, Ordering}, mpsc},
    thread::JoinHandle,
    time::Instant,
};

const MODEL: &str = "homepanel.fixture.device";
static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

fn root(tag: &str) -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap();
    let work = workspace.join("work");
    assert!(work.is_dir());
    let path = work.join(format!("HomePanel-plugin-download-test-{tag}-{}-{}-{}",
        std::process::id(), crate::now_ms(), NEXT_ROOT.fetch_add(1, Ordering::Relaxed)));
    assert!(!path.exists());
    std::fs::create_dir(&path).unwrap();
    path
}

fn row() -> Value {
    json!({"model":MODEL,"plugin_id":1_000_001,"plugin_package_id":1_000_002,
        "version":17,"sdk_version":10116,"size":128,
        "download_url":"https://iot-cdn.io.mi.com/synthetic.zip?Signature=fixture-signed-url"})
}

fn source(size: usize) -> Candidate {
    Candidate {url:cdn_url("https://iot-cdn.io.mi.com/synthetic.zip?Signature=fixture-signed-url").unwrap(),
        size:size as u64,version:17,sdk_version:Some(10116)}
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        archive.start_file(*name, options).unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap().into_inner()
}

fn valid_zip() -> Vec<u8> {
    zip(&[("android/project.json",br#"{"package_path":"homepanel.fixture","platform":"android","version":"1.0"}"#),
        ("android/main.bundle",b"__d(function(){return 'fixture-readable-payload-Unique';});")])
}

fn save_fixture(root: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    assert!(!name.contains(['/', '\\', ':']));
    let path = root.join(name);
    assert!(!path.exists());
    std::fs::write(&path, bytes).unwrap();
    path
}

fn quarantine_files(root: &Path) -> Vec<PathBuf> {
    let path = root.join("plugin-downloads-v1/quarantine");
    if !path.exists() {return Vec::new();}
    std::fs::read_dir(path).unwrap().map(|entry|entry.unwrap().path()).collect()
}

fn assert_no_pending(root: &Path) {
    assert!(quarantine_files(root).iter().all(|path|path.extension().and_then(|x|x.to_str())!=Some("part")),
        "failed/cancelled downloads must release and remove only their temporary file");
}

fn read_line_before(reader: &mut BufReader<std::net::TcpStream>, line: &mut String, deadline: Instant) {
    loop {
        match reader.read_line(line) {
            Ok(0) => panic!("loopback fixture request ended before its headers"),
            Ok(_) => return,
            Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
                && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => panic!("loopback fixture could not read request headers: {error}"),
        }
    }
}

fn server(raw: Vec<u8>, delay_close: Duration) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let deadline = Instant::now()+Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream,_))=>break stream,
                Err(error) if error.kind()==std::io::ErrorKind::WouldBlock && Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(5)),
                Err(error)=>panic!("loopback fixture did not receive its request: {error}"),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        let request_deadline = Instant::now()+Duration::from_secs(5);
        loop {
            let mut line = String::new();
            read_line_before(&mut reader,&mut line,request_deadline);
            request.push_str(&line);
            if line=="\r\n" {break;}
            assert!(request.len()<16384);
        }
        // A cancelled downloader may close the connection before the fixture's
        // final write; the server has no account data and no external endpoint.
        let _=stream.write_all(&raw);
        if !delay_close.is_zero() {std::thread::sleep(delay_close);}
        request
    });
    (format!("http://{address}/fixture"),handle)
}

fn fixed_response(body: &[u8]) -> Vec<u8> {
    let mut raw=format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).into_bytes();
    raw.extend_from_slice(body);raw
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

async fn loopback_response(raw: Vec<u8>, delay_close: Duration) -> (reqwest::Response,JoinHandle<String>) {
    let (url,handle)=server(raw,delay_close);
    let client=reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3)).build().unwrap();
    (client.get(url).send().await.unwrap(),handle)
}

#[test]
fn metadata_selects_the_exact_model_and_never_the_first_unrelated_row() {
    let mut unrelated=row();unrelated["model"]=json!("other.fixture.device");
    unrelated["download_url"]=json!("https://untrusted.example.test/fixture");
    let result=json!({"latest_info":[unrelated.clone(),row()]});
    assert_eq!(candidate(&result,MODEL).unwrap().version,17);
    assert!(candidate(&json!({"latest_info":[unrelated]}),MODEL).is_err());
    assert!(candidate(&json!({"latest_info":[row(),row()]}),MODEL).is_err());
    assert!(candidate(&result,"homepanel.fixture.other").is_err());
}

#[test]
fn invalid_current_metadata_does_not_silently_downgrade_to_a_backup_package() {
    let mut backup=row();backup["length"]=json!(128);backup["version"]=json!(2);
    for size in [Value::Null,json!(0),json!(MAX_DOWNLOAD+1),json!("128")] {
        let mut current=row();current["size"]=size;
        assert!(candidate(&json!({"latest_info":[current],"backup_info":[backup.clone()]}),MODEL).is_err());
    }
    let old=candidate(&json!({"latest_info":[],"backup_info":[backup]}),MODEL).unwrap();
    assert_eq!(old.version,2);
    let mut hostile=row();hostile["safe_url"]=json!("https://attacker.example.test/fixture?Signature=fixture-private-value");
    let error=candidate(&json!({"latest_info":[hostile]}),MODEL).err().unwrap();
    assert!(!error.contains("fixture-private-value"));
}

#[test]
fn cdn_validation_requires_https_port_and_exact_domain_boundaries() {
    for url in ["https://iot-cdn.io.mi.com/a?Signature=fixture-only",
        "https://cdn.cnbj1.fds.api.mi-img.com/a","https://cnbj1.fds.api.xiaomi.com/a"] {
        assert!(cdn_url(url).is_ok());
    }
    for url in ["http://iot-cdn.io.mi.com/a","https://iot-cdn.io.mi.com:444/a",
        "https://iot-cdn.io.mi.com.evil.example/a","https://evilfds.api.mi-img.com/a",
        "https://iot-cdn.io.mi.com@attacker.example/a","https://user:password@iot-cdn.io.mi.com/a",
        "https://iot-cdn.io.mi.com/a#fragment","file:///fixture","https://127.0.0.1/a"] {
        assert!(cdn_url(url).is_err(),"unexpectedly allowed URL fixture");
    }
}

#[test]
fn production_download_client_omits_credentials_and_does_not_follow_a_redirect() {
    let second=TcpListener::bind("127.0.0.1:0").unwrap();
    second.set_nonblocking(true).unwrap();let destination=second.local_addr().unwrap();
    let (stop,stopped)=mpsc::channel();
    let second_server=std::thread::spawn(move||{
        let deadline=Instant::now()+Duration::from_secs(5);
        loop {
            if let Ok((mut stream,_))=second.accept(){
                let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                return true;
            }
            if stopped.try_recv().is_ok(){return false;}
            assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));
        }
    });
    let raw=format!("HTTP/1.1 302 Found\r\nLocation: http://{destination}/unexpected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes();
    let (url,first_server)=server(raw,Duration::ZERO);
    let response=runtime().block_on(async {download_client().unwrap().get(url).send().await}).unwrap();
    let status=response.status();drop(response);stop.send(()).unwrap();
    assert!(!second_server.join().unwrap(),"a plugin CDN redirect must never be followed");
    assert_eq!(status.as_u16(),302);
    let headers=first_server.join().unwrap().to_ascii_lowercase();
    for header in ["\r\ncookie:","\r\nauthorization:","\r\nreferer:"] {assert!(!headers.contains(header));}
}

#[test]
fn http_header_and_stream_size_mismatches_are_rejected_without_retaining_partial_files() {
    let root=root("size");let rt=runtime();
    let fixtures=[
        (fixed_response(b"12345"),6usize),
        (b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\n12345\r\n0\r\n\r\n".to_vec(),4usize),
        (b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n12345".to_vec(),6usize),
    ];
    for (raw,size) in fixtures {
        rt.block_on(async {
            let (response,server)=loopback_response(raw,Duration::ZERO).await;
            assert!(receive(response,&source(size),&root).await.is_err());
            server.join().unwrap();
        });
        assert_no_pending(&root);
        assert!(quarantine_files(&root).is_empty());
    }
}

#[test]
fn cancelled_http_receive_releases_its_windows_file_before_removing_the_part() {
    let root=root("cancel");
    runtime().block_on(async {
        let raw=b"HTTP/1.1 200 OK\r\nContent-Length: 32\r\nConnection: close\r\n\r\nfirst".to_vec();
        let (response,server)=loopback_response(raw,Duration::from_millis(250)).await;
        let result=tokio::time::timeout(Duration::from_millis(50),receive(response,&source(32),&root)).await;
        assert!(result.is_err(),"fixture must cancel an in-progress receive");
        server.join().unwrap();
    });
    assert_no_pending(&root);assert!(quarantine_files(&root).is_empty());
}

#[test]
fn archive_inspection_rejects_traversal_case_collisions_ambiguous_entries_and_directory_payloads() {
    let root=root("paths");
    let fixtures=[
        zip(&[("../escape.js",b"fixture")]),
        zip(&[("C:/escape.js",b"fixture")]),
        zip(&[("android\\main.bundle",b"fixture")]),
        zip(&[("android/main.bundle",b"__d(fixture)"),("ANDROID/MAIN.BUNDLE",b"__d(other)")]),
        zip(&[("main.bundle",b"unknown first"),("android/main.bundle",b"__d(second)")]),
        zip(&[("android/",b"not an empty directory")]),
    ];
    for (index,bytes) in fixtures.iter().enumerate(){
        let path=save_fixture(&root,&format!("rejected-{index}.zip"),bytes);
        assert!(inspect_archive(&path).is_err());
    }
    let valid=save_fixture(&root,"empty-directory.zip",&zip(&[("android/",b""),("android/main.bundle",b"__d(fixture)")]));
    assert_eq!(inspect_archive(&valid).unwrap(),(BundleFormat::Javascript,true));
}

#[test]
fn corrupted_zip_payloads_do_not_receive_an_integrity_success() {
    let root=root("crc");let mut bytes=valid_zip();
    let marker=b"fixture-readable-payload-Unique";
    let offset=bytes.windows(marker.len()).position(|part|part==marker).unwrap();
    bytes[offset]^=1;
    let path=save_fixture(&root,"corrupted.zip",&bytes);
    assert!(inspect_archive(&path).is_err());
    runtime().block_on(async{
        let (response,server)=loopback_response(fixed_response(&bytes),Duration::ZERO).await;
        assert!(receive(response,&source(bytes.len()),&root).await.is_err());
        server.join().unwrap();
    });
    assert_no_pending(&root);assert!(quarantine_files(&root).is_empty());
}

#[test]
fn downloaded_valid_zip_is_content_addressed_and_stays_unauthenticated_and_non_executable() {
    let root=root("valid");let bytes=valid_zip();let expected=format!("{:x}",Sha256::digest(&bytes));
    let rt=runtime();
    for _ in 0..2 {
        let report=rt.block_on(async{
            let (response,server)=loopback_response(fixed_response(&bytes),Duration::ZERO).await;
            let report=receive(response,&source(bytes.len()),&root).await.unwrap();server.join().unwrap();report
        });
        assert_eq!(report.sha256,expected);assert_eq!(report.bytes,bytes.len() as u64);
        assert_eq!(report.format,BundleFormat::Javascript);
        assert!(report.declared_size_verified&&report.archive_crc_verified);
        assert!(!report.publisher_signature_verified&&!report.can_execute);
        let json=serde_json::to_string(&report).unwrap();
        assert!(!json.contains("fixture-signed-url")&&!json.contains("https://"));
    }
    let files=quarantine_files(&root);assert_eq!(files.len(),1);assert_no_pending(&root);
    assert_eq!(files[0].file_name().unwrap().to_str().unwrap(),format!("{expected}.plugin-cache"));
    assert_eq!(std::fs::read(&files[0]).unwrap(),bytes);
}

#[test]
fn unknown_downloads_remain_opaque_and_corrupt_existing_cache_is_never_replaced() {
    let root=root("opaque");let bytes=b"opaque fixture content, never executable";
    let expected=format!("{:x}",Sha256::digest(bytes));let rt=runtime();
    let report=rt.block_on(async{
        let (response,server)=loopback_response(fixed_response(bytes),Duration::ZERO).await;
        let report=receive(response,&source(bytes.len()),&root).await.unwrap();server.join().unwrap();report
    });
    assert_eq!(report.format,BundleFormat::Unknown);
    assert!(!report.archive_crc_verified&&!report.publisher_signature_verified&&!report.can_execute);
    let cache=root.join("plugin-downloads-v1/quarantine").join(format!("{expected}.plugin-cache"));
    let mut damaged=bytes.to_vec();damaged[0]^=1;std::fs::write(&cache,&damaged).unwrap();
    rt.block_on(async{
        let (response,server)=loopback_response(fixed_response(bytes),Duration::ZERO).await;
        assert!(receive(response,&source(bytes.len()),&root).await.is_err());server.join().unwrap();
    });
    assert_eq!(std::fs::read(cache).unwrap(),damaged);
    assert_no_pending(&root);assert_eq!(quarantine_files(&root).len(),1);
}
