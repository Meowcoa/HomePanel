//! Single-device RN broker, used only when explicitly opening the optional host. No account credentials cross stdio.
//! This is an original protocol, not an implementation of the entire phone SDK.
use crate::{cloud::Cloud, model::{Device, Property, StateSource}};
use crate::plugin_bridge_extensions::{self as contracts, ActionTarget, PropertyTarget as Target, checked_property, validate_targets};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::{BufRead, Read, Write}, path::Path, process::{Command, Stdio}, sync::{Arc, atomic::Ordering}};

const MAX_FRAME: u64 = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    GetDevice {},
    GetCapabilities {},
    GetPrivacyStatus {},
    Read { targets: Vec<Target> },
    Write { target: Target, value: Value },
    #[serde(alias = "doAction")]
    Action { target: ActionTarget, inputs: Vec<Value> },
    LoadScenes { r#type:u32 },
    SpeakerRequest {path:String,method:String,message:Value},
}

struct Broker {
    device: Device,
    cloud: Option<Arc<Cloud>>,
    region: String,
    epoch: u64,
    writes_allowed: bool,
}

fn direct_value(property: &Property) -> Value {
    contracts::direct_value_at(property, crate::now_ms())
}

impl Broker {
    async fn current(&self) -> Result<Device, &'static str> {
        let Some(cloud) = &self.cloud else { return Ok(self.device.clone()); };
        if cloud.epoch.load(Ordering::SeqCst) != self.epoch { return Err("SESSION_REVOKED"); }
        let current = cloud.device(&self.device.id).await.map_err(|_| "DEVICE_UNAVAILABLE")?;
        if current.model != self.device.model || current.spec_urn != self.device.spec_urn { return Err("DEVICE_CHANGED"); }
        Ok(current)
    }

    async fn execute(&mut self, request: Request) -> Result<Value, &'static str> {
        let current = self.current().await?;
        match request {
            Request::GetDevice {} => Ok(json!({"protocol":2,"testData":self.cloud.is_none(),
                "name":current.name,"model":current.model,"online":current.online,"region":self.region,
                "spec":match &self.cloud {Some(cloud)=>Some(cloud.plugin_instance(&current).map_err(|_| "SPEC_UNAVAILABLE")?),None=>None},
                "writesAllowed":self.writes_allowed,"stateTransport":"polling",
                "properties":current.properties.iter().map(|p|json!({"siid":p.siid,"piid":p.piid,
                    "name":p.name,"format":p.format,"unit":p.unit,"range":p.range,"options":p.options,
                    "readable":p.readable,"writable":p.writable && self.writes_allowed})).collect::<Vec<_>>() })),
            Request::GetCapabilities {} => {
                let instance = self.cloud.as_ref().map(|cloud| cloud.plugin_instance(&current))
                    .transpose().map_err(|_| "SPEC_UNAVAILABLE")?;
                Ok(contracts::capabilities(&current, instance.as_ref(), self.writes_allowed))
            }
            Request::GetPrivacyStatus {} => {
                let cloud=self.cloud.as_ref().ok_or("TEST_SERVICE_UNAVAILABLE")?;
                let authorized=cloud.plugin_privacy_status(&self.region,&self.device,self.epoch).await?;
                self.current().await?;
                Ok(json!({"authorized":authorized,"source":"existing-cloud-record"}))
            }
            Request::Read { targets } => {
                validate_targets(&current, &targets)?;
                if !current.online { return Err("DEVICE_OFFLINE"); }
                let updated = if let Some(cloud) = &self.cloud {
                    cloud.read_device(&self.region, &self.device.id).await.map_err(|_| "DEVICE_READ_FAILED")?
                } else { self.device.clone() };
                self.current().await?; // Revoke a reply if logout/model change raced a read.
                validate_targets(&updated, &targets)?;
                Ok(json!(targets.iter().map(|target| direct_value(checked_property(&updated,target,false).expect("validated target"))).collect::<Vec<_>>()))
            }
            Request::Write { target, value } => {
                if !self.writes_allowed { return Err("READ_ONLY_SESSION"); }
                if !current.online { return Err("DEVICE_OFFLINE"); }
                let property = checked_property(&current, &target, true)?;
                // Writes without readback cannot satisfy this broker's confirmation contract.
                if !property.readable { return Err("READBACK_UNSUPPORTED"); }
                crate::model::validate_property(property, &value).map_err(|_| "INVALID_PROPERTY_VALUE")?;
                let updated = if let Some(cloud) = &self.cloud {
                    cloud.plugin_write(&self.region, &self.device, self.epoch, &target, value, self.writes_allowed).await?
                } else {
                    let property = self.device.properties.iter_mut().find(|p|p.siid==target.siid && p.piid==target.piid).expect("validated fixture");
                    property.value=Some(value); property.error=None;
                    property.source=StateSource::Device; property.checked_at=Some(crate::now_ms());
                    self.device.clone()
                };
                self.current().await?;
                Ok(direct_value(checked_property(&updated,&target,false)?))
            }
            Request::Action { target, inputs } => {
                if !self.writes_allowed { return Err("READ_ONLY_SESSION"); }
                // A fixture must never pretend to have executed a hardware action.
                let cloud = self.cloud.as_ref().ok_or("TEST_ACTION_UNAVAILABLE")?;
                let reply = cloud.plugin_action(&self.region, &self.device, self.epoch,
                    &target, &inputs, self.writes_allowed).await?;
                self.current().await?;
                Ok(reply)
            }
            Request::LoadScenes {r#type}=>{
                if r#type!=8{return Err("SCENE_TYPE_UNSUPPORTED");}
                let cloud=self.cloud.as_ref().ok_or("TEST_SERVICE_UNAVAILABLE")?;
                let raw=cloud.plugin_timers(&self.region,&self.device,self.epoch).await?;
                self.current().await?;
                crate::plugin_services::timers_reply(&raw,&self.device.id)
            }
            Request::SpeakerRequest {path,method,message}=>{
                if path!="mediaplayer"||method!="player_get_play_status"||!message.as_object().is_some_and(|m|m.is_empty()){return Err("SPEAKER_REQUEST_UNSUPPORTED");}
                let cloud=self.cloud.as_ref().ok_or("TEST_SERVICE_UNAVAILABLE")?;
                let raw=cloud.plugin_speaker_status(&self.region,&self.device,self.epoch).await?;
                self.current().await?;
                crate::plugin_services::speaker_reply(&raw)
            }
        }
    }
}

#[cfg(any(test, debug_assertions))]
fn fixture() -> Device {
    Device { id:"homepanel-test-device".into(), model:"homepanel.test.plug".into(),
        name:"接口测试插座 · 测试数据".into(), online:true, spec_status:"ready".into(),
        properties:vec![Property {siid:2,piid:1,name:"电源".into(),format:"bool".into(),
            readable:true,writable:true,value:Some(json!(false)),source:StateSource::Device,
            checked_at:Some(crate::now_ms()),..Default::default()}], ..Default::default() }
}

fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, &'static str> {
    let mut bytes = Vec::new();
    let count = reader.take(MAX_FRAME + 1).read_until(b'\n', &mut bytes).map_err(|_| "PIPE_READ_FAILED")?;
    if count == 0 { return Ok(None); }
    if count as u64 > MAX_FRAME || !bytes.ends_with(b"\n") { return Err("INVALID_FRAME"); }
    Ok(Some(bytes))
}

/// Explicit developer CLI only. Normal HomePanel startup never starts the host.
#[cfg(debug_assertions)]
pub fn run(args: &[String]) -> Option<Result<(), &'static str>> {
    let official = args.iter().position(|a| a == "--official-plugin-host-self-test");
    let self_test = args.iter().position(|a| a == "--plugin-host-self-test");
    let normal = args.iter().position(|a| a == "--plugin-host");
    let index = official.or(self_test).or(normal)?;
    Some(run_host(args, index, self_test.is_some() || official.is_some(), official.is_some()))
}

#[cfg(debug_assertions)]
fn run_host(args: &[String], index: usize, self_test: bool, official: bool) -> Result<(), &'static str> {
    let path = Path::new(args.get(index + 1).ok_or("HOST_PATH_REQUIRED")?).canonicalize().map_err(|_| "HOST_PATH_INVALID")?;
    if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("exe") { return Err("HOST_PATH_INVALID"); }
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().map_err(|_| "RUNTIME_UNAVAILABLE")?;
    let mut broker = if self_test {
        Broker {device:fixture(),cloud:None,region:"cn".into(),epoch:0,writes_allowed:true}
    } else {
        let did_index=args.iter().position(|a|a=="--device-id").ok_or("DEVICE_ID_REQUIRED")?;
        let did=args.get(did_index+1).filter(|s|!s.starts_with("--")).ok_or("DEVICE_ID_REQUIRED")?;
        let (root,cache)=crate::app_directories(false).map_err(|_| "ACCOUNT_STORAGE_UNAVAILABLE")?;
        let region=crate::load_preferences(&root).region;
        let cloud=Arc::new(Cloud::new(root,cache).map_err(|_| "CLOUD_UNAVAILABLE")?);
        runtime.block_on(cloud.discover(&region)).map_err(|_| "DEVICE_DISCOVERY_FAILED")?;
        let device=runtime.block_on(cloud.load_spec(did)).map_err(|_| "DEVICE_UNAVAILABLE")?;
        let epoch=cloud.epoch.load(Ordering::SeqCst);
        Broker {device,cloud:Some(cloud),region,epoch,writes_allowed:args.iter().any(|a|a=="--allow-device-writes")}
    };
    use std::os::windows::process::CommandExt;
    let mut child=Command::new(&path).arg(if official {"--official-plugin-self-test"} else if self_test {"--broker-self-test"} else {"--broker"})
        .current_dir(path.parent().ok_or("HOST_PATH_INVALID")?).creation_flags(0x08000000)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_| "HOST_START_FAILED")?;
    let mut input=child.stdin.take().ok_or("PIPE_UNAVAILABLE")?;
    let mut output=std::io::BufReader::new(child.stdout.take().ok_or("PIPE_UNAVAILABLE")?);
    let session= (|| {
        let mut count=0;
        let mut window=std::time::Instant::now();
        let mut burst=0;
        while let Some(bytes)=read_frame(&mut output)? {
            count+=1;
            if count>4096 { return Err("SESSION_REQUEST_LIMIT"); }
            if window.elapsed()>=std::time::Duration::from_secs(10) {window=std::time::Instant::now();burst=0;}
            burst+=1;
            let result=if burst>16 {Err("RATE_LIMITED")} else {
                match serde_json::from_slice::<Request>(&bytes) {
                    Ok(request)=>runtime.block_on(broker.execute(request)),
                    Err(_)=>Err("INVALID_REQUEST"),
                }
            };
            let response=match result {Ok(value)=>json!({"ok":true,"result":value}),Err(code)=>json!({"ok":false,"error":code})};
            let mut bytes=serde_json::to_vec(&response).map_err(|_| "RESPONSE_INVALID")?;
            if bytes.len() as u64>=MAX_FRAME {bytes=b"{\"ok\":false,\"error\":\"RESPONSE_TOO_LARGE\"}".to_vec();}
            bytes.push(b'\n');
            input.write_all(&bytes).and_then(|_|input.flush()).map_err(|_| "PIPE_WRITE_FAILED")?;
        }
        Ok(())
    })();
    drop(input);
    if session.is_err() { let _=child.kill(); }
    let status=child.wait().map_err(|_| "HOST_WAIT_FAILED")?;
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    session?;
    if !status.success() {return Err("HOST_EXIT_FAILED");}
    Ok(())
}

pub struct Session {
    child: Arc<std::sync::Mutex<std::process::Child>>,
    result: std::sync::mpsc::Receiver<Result<(), &'static str>>,
}
impl Session {
    pub fn outcome(&self) -> Option<Result<(), &'static str>> { self.result.try_recv().ok() }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            if child.try_wait().ok().flatten().is_none() { let _ = child.kill(); }
            let _ = child.wait();
        }
    }
}

/// Owns the same Cloud session as the GUI. Credentials and real device IDs never
/// cross the pipe. Native writes still validate SPEC and perform direct readback.
pub fn launch(cloud: Arc<Cloud>, device: Device, region: String, root: &Path,
    writes_allowed: bool, context: eframe::egui::Context) -> Result<Session, String> {
    if !crate::plugin_runtime::supports(&device) { return Err("当前候选仅开放单路通用插座，请使用原生控制面板".into()); }
    let epoch=cloud.epoch.load(Ordering::SeqCst);
    let plugin_dir = crate::plugin_runtime::validate(root)?;
    launch_directory(cloud,device,region,plugin_dir,writes_allowed,epoch,context)
}

pub fn launch_prepared(cloud:Arc<Cloud>,device:Device,region:String,root:&Path,report:&crate::plugin_download::DownloadReport,
    writes_allowed:bool,epoch:u64,context:eframe::egui::Context)->Result<Session,String> {
    if cloud.epoch.load(Ordering::SeqCst)!=epoch{return Err("账号已变更，取消插件启动".into());}
    let prepared=crate::plugin_prepare::validate(root,&device.model,report)?;
    launch_directory(cloud,device,region,prepared.directory,writes_allowed,epoch,context)
}

fn launch_directory(cloud:Arc<Cloud>,device:Device,region:String,plugin_dir:std::path::PathBuf,
    writes_allowed:bool,epoch:u64,context:eframe::egui::Context)->Result<Session,String> {
    cloud.plugin_instance(&device)?;
    let path = crate::plugin_runtime::host_path()?;
    if cloud.epoch.load(Ordering::SeqCst)!=epoch{return Err("账号已变更，取消插件启动".into());}
    use std::os::windows::process::CommandExt;
    let mut child = Command::new(&path).arg("--official-plugin").arg(plugin_dir)
        .env("HOMEPANEL_PARENT_PID", std::process::id().to_string())
        .current_dir(path.parent().ok_or("无法定位插件宿主目录")?).creation_flags(0x08000000)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
        .map_err(|_| "无法启动插件宿主，请重新安装集成版")?;
    let mut input = child.stdin.take().ok_or("无法创建设备管道")?;
    let mut output = std::io::BufReader::new(child.stdout.take().ok_or("无法创建设备管道")?);
    let child = Arc::new(std::sync::Mutex::new(child));
    let owned_child = child.clone();
    let mut broker = Broker {device,cloud:Some(cloud),region,epoch,writes_allowed};
    let (tx,result) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut result = (|| {
            let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().map_err(|_| "RUNTIME_UNAVAILABLE")?;
            let mut window = std::time::Instant::now(); let mut burst = 0;
            while let Some(bytes) = read_frame(&mut output)? {
                if window.elapsed() >= std::time::Duration::from_secs(10) { window=std::time::Instant::now();burst=0; }
                burst+=1;
                let result = if burst>24 {Err("RATE_LIMITED")} else {
                    match serde_json::from_slice::<Request>(&bytes) {
                        Ok(request)=>runtime.block_on(broker.execute(request)),Err(_)=>Err("INVALID_REQUEST"),
                    }
                };
                let reply = match result {Ok(value)=>json!({"ok":true,"result":value}),Err(code)=>json!({"ok":false,"error":code})};
                let mut bytes=serde_json::to_vec(&reply).map_err(|_| "RESPONSE_INVALID")?;
                if bytes.len() as u64>=MAX_FRAME {bytes=b"{\"ok\":false,\"error\":\"RESPONSE_TOO_LARGE\"}".to_vec();}
                bytes.push(b'\n'); input.write_all(&bytes).and_then(|_|input.flush()).map_err(|_| "PIPE_WRITE_FAILED")?;
            }
            runtime.shutdown_timeout(std::time::Duration::from_secs(2));
            Ok(())
        })();
        drop(input);
        if let Ok(mut child) = owned_child.lock() {
            // Give normal process teardown time after stdout EOF. A broken pipe
            // or abnormal host exit must not look like a user closing a window.
            let mut status=child.try_wait().ok().flatten();
            for _ in 0..20 {if status.is_some(){break;}std::thread::sleep(std::time::Duration::from_millis(10));status=child.try_wait().ok().flatten();}
            if status.is_none() {let _=child.kill();if result.is_ok(){result=Err("HOST_TRANSPORT_CLOSED");}}
            if child.wait().ok().is_some_and(|status|!status.success())&&result.is_ok(){result=Err("HOST_EXIT_FAILED");}
        }
        let _=tx.send(result);context.request_repaint();
    });
    Ok(Session{child,result})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target()->Target {Target{siid:2,piid:1,miid:None}}
    #[test] fn strict_requests_reject_device_escape_and_unknown_operations() {
        for raw in [r#"{"op":"read","did":"other","targets":[]}"#,r#"{"op":"rpc","method":"remove"}"#,r#"{"op":"getDevice","token":"x"}"#,
            r#"{"op":"getCapabilities","did":"other"}"#,
            r#"{"op":"action","target":{"siid":3,"aiid":1,"did":"other"},"inputs":[]}"#,
            r#"{"op":"action","target":{"siid":3,"aiid":1}}"#] {
            assert!(serde_json::from_str::<Request>(raw).is_err());
        }
        for op in ["action","doAction"] {
            assert!(matches!(serde_json::from_value::<Request>(json!({"op":op,"target":{"siid":3,"aiid":1},"inputs":[]})),Ok(Request::Action{..})));
        }
    }
    #[test] fn target_permissions_versions_and_batch_limits_are_checked() {
        let mut d=fixture();
        let mut t=target();t.miid=Some(0);
        assert_eq!(validate_targets(&d,&[t]),Err("SPEC_V3_UNSUPPORTED"));
        assert_eq!(validate_targets(&d,&[]),Err("INVALID_TARGET_COUNT"));
        assert_eq!(validate_targets(&d,&vec![target();13]),Err("INVALID_TARGET_COUNT"));
        assert_eq!(validate_targets(&d,&[target(),target()]),Err("DUPLICATE_TARGET"));
        d.properties[0].readable=false;
        assert_eq!(validate_targets(&d,&[target()]),Err("PERMISSION_DENIED"));
        assert!(checked_property(&d,&Target{siid:9,..target()},true).is_err());
    }
    #[test] fn cached_and_failed_values_never_look_like_confirmed_device_state() {
        let mut d=fixture();assert_eq!(direct_value(&d.properties[0])["available"],true);
        d.properties[0].source=StateSource::Cloud;
        assert_eq!(direct_value(&d.properties[0])["value"],Value::Null);
        d.properties[0].source=StateSource::Device;d.properties[0].error=Some("error".into());
        assert_eq!(direct_value(&d.properties[0])["available"],false);
    }
    #[test] fn write_requires_permission_and_confirmable_type() {
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let mut b=Broker{device:fixture(),cloud:None,region:"cn".into(),epoch:0,writes_allowed:false};
        assert_eq!(rt.block_on(b.execute(Request::Write{target:target(),value:json!(true)})),Err("READ_ONLY_SESSION"));
        b.writes_allowed=true;
        assert_eq!(rt.block_on(b.execute(Request::Write{target:target(),value:json!("true")})),Err("INVALID_PROPERTY_VALUE"));
        assert_eq!(b.device.properties[0].value,Some(json!(false)));
        assert_eq!(rt.block_on(b.execute(Request::Write{target:target(),value:json!(true)})).unwrap()["value"],true);
        b.device.properties[0].readable=false;
        assert_eq!(rt.block_on(b.execute(Request::Write{target:target(),value:json!(false)})),Err("READBACK_UNSUPPORTED"));
    }
    #[test] fn capabilities_report_the_protocol_and_fixture_actions_cannot_claim_hardware_success() {
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let mut b=Broker{device:fixture(),cloud:None,region:"cn".into(),epoch:0,writes_allowed:false};
        let capabilities=rt.block_on(b.execute(Request::GetCapabilities{})).unwrap();
        assert_eq!(capabilities["protocol"],2);
        assert_eq!(capabilities["readOnly"],true);
        assert_eq!(capabilities["stateTransport"],"polling");
        assert_eq!(capabilities["properties"][0]["write"],false);
        assert_eq!(rt.block_on(b.execute(Request::Action{target:ActionTarget{siid:3,aiid:1,miid:None},inputs:vec![]})),Err("READ_ONLY_SESSION"));
        b.writes_allowed=true;
        assert_eq!(rt.block_on(b.execute(Request::Action{target:ActionTarget{siid:3,aiid:1,miid:None},inputs:vec![]})),Err("TEST_ACTION_UNAVAILABLE"));
    }
    #[test] fn framing_rejects_truncation_and_excess_bytes() {
        assert!(read_frame(&mut std::io::Cursor::new(b"{}" )).is_err());
        assert!(read_frame(&mut std::io::Cursor::new(vec![b'x';MAX_FRAME as usize+1])).is_err());
        assert!(read_frame(&mut std::io::Cursor::new(b"" )).unwrap().is_none());
        assert_eq!(read_frame(&mut std::io::Cursor::new(b"{}\n" )).unwrap().unwrap(),b"{}\n");
    }
    #[test] fn a_device_lease_is_revoked_by_account_or_model_changes_without_network() {
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        // An intentionally nonexistent test path: no real account directory is read.
        let root=std::env::temp_dir().join(format!("HomePanel-absent-broker-test-{}-{}",std::process::id(),crate::now_ms()));
        assert!(!root.exists());
        let cloud=Arc::new(Cloud::new(root.clone(),root.join("cache")).unwrap());
        rt.block_on(cloud.devices.lock()).insert(fixture().id.clone(),fixture());
        let broker=Broker{device:fixture(),cloud:Some(cloud.clone()),region:"cn".into(),epoch:0,writes_allowed:false};
        assert!(rt.block_on(broker.current()).is_ok());
        rt.block_on(cloud.devices.lock()).get_mut(&fixture().id).unwrap().model="another.model".into();
        assert!(matches!(rt.block_on(broker.current()),Err("DEVICE_CHANGED")));
        cloud.epoch.fetch_add(1,Ordering::SeqCst);
        assert!(matches!(rt.block_on(broker.current()),Err("SESSION_REVOKED")));
        assert!(!root.exists());
    }
}
