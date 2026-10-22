use crate::{crypto,model::*,storage::{self,Session},now_ms};
use base64::{engine::general_purpose::STANDARD as B64,Engine};
use reqwest::{Client,cookie::{CookieStore,Jar}};
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use std::{collections::{HashMap,HashSet},path::PathBuf,sync::{Arc,atomic::{AtomicU64,Ordering}},time::{Duration,Instant}};
use tokio::sync::{watch,Mutex,RwLock,Semaphore};

pub const REGIONS:&[&str]=&["cn","de","i2","ru","sg","tw","us"];
const AGENT:&str="HomePanel/0.1.1 APP/com.xiaomi.mihome APPV/10.5.201";

#[derive(Clone,Copy,PartialEq,Eq)]
enum LoginService {MiHome,Speaker}
impl LoginService {fn sid(self)->&'static str {match self {Self::MiHome=>"xiaomiio",Self::Speaker=>"micoapi"}}}

fn login_url(raw:&str,service:LoginService)->Result<url::Url,String> {
    if let Ok(url)=official_url(raw){return Ok(url);}
    let url=url::Url::parse(raw).map_err(|_|"登录地址无效")?;
    if service==LoginService::Speaker && url.scheme()=="https" && url.username().is_empty() && url.password().is_none()
        && url.port_or_known_default()==Some(443) && url.host_str()==Some("api2.mina.xiaoaisound.com") {return Ok(url);}
    Err("登录返回了未授权的服务地址".into())
}

pub struct Cloud {
    pub session:RwLock<Option<Session>>,pub devices:Mutex<HashMap<String,Device>>,
    pub homes:Mutex<Vec<Home>>,pub epoch:AtomicU64,pub login:Mutex<LoginStatus>,login_tx:watch::Sender<LoginStatus>,pub has_saved_session:bool,
    client:Client,public:Client,root:PathBuf,cache:PathBuf,service:LoginService,
    // Public label metadata is isolated from account requests; failed fetches retry later.
    labels:Mutex<HashMap<String,(Instant,Option<Value>)>>,
    catalog:Mutex<HashMap<String,String>>,catalog_gate:Mutex<()>,read_gate:Semaphore,
    device_gates:Mutex<HashMap<String,Arc<Mutex<()>>>>,
}
#[derive(Clone,Serialize,Default)]
#[serde(rename_all="camelCase")]
pub struct LoginStatus {pub stage:String,pub qr:Option<String>,pub expires_at:Option<u64>,pub message:String}
#[derive(Deserialize)] struct QrResponse {qr:String,lp:String,timeout:u64}

pub fn network_error(e:reqwest::Error)->String {
    if e.is_timeout(){"连接超时，请检查网络后重试"}else if e.is_connect(){"无法连接小米服务，请检查网络"}else{"网络请求失败，请稍后重试"}.into()
}
pub fn official_url(raw:&str)->Result<url::Url,String>{
    let u=url::Url::parse(raw).map_err(|_|"图片或登录地址无效")?;
    let h=u.host_str().unwrap_or_default();
    if u.scheme()!="https" || !u.username().is_empty() || u.password().is_some() || !["xiaomi.com","mi.com","mi-img.com","xiaomi.net"].iter().any(|base|h==*base||h.ends_with(&format!(".{base}"))) {return Err("服务返回了非官方 HTTPS 地址，已拒绝访问".into());}
    Ok(u)
}
fn parse_account(raw:&str)->Result<Value,String>{serde_json::from_str(raw.strip_prefix("&&&START&&&").unwrap_or(raw)).map_err(|_|"登录服务响应格式已变化".into())}
fn region_url(region:&str)->Result<String,String>{if !REGIONS.contains(&region){return Err("不支持的服务地区".into());}Ok(format!("https://{}api.io.mi.com/app",if region=="cn"{String::new()}else{format!("{region}.")}))}

fn session_exchange_url(data:&Value,service:LoginService)->Result<url::Url,String> {
    let mut location=login_url(data["location"].as_str().ok_or("登录缺少会话交换地址")?,service)?;
    if service!=LoginService::Speaker {return Ok(location);}
    let signatures:Vec<_>=location.query_pairs().filter(|(key,_)|key=="clientSign").map(|(_,value)|value.into_owned()).collect();
    if signatures.len()==1 && !signatures[0].is_empty(){return Ok(location);}
    if !signatures.is_empty(){return Err("小爱会话校验参数无效，请重新扫码".into());}
    // MiService STS protocol: use this login's nonce and ssecurity. Do not
    // convert a 64-bit nonce to floating point or borrow the MiHome session.
    let nonce=match &data["nonce"] {
        Value::Number(number) if number.is_i64()||number.is_u64()=>number.to_string(),
        Value::String(text) if text.parse::<i64>().is_ok()||text.parse::<u64>().is_ok()=>text.clone(),
        _=>return Err("扫码结果缺少有效的小爱会话校验参数，请重新扫码".into()),
    };
    let secret=data["ssecurity"].as_str().filter(|s|!s.is_empty()).ok_or("小爱会话校验参数缺失，请重新扫码")?;
    use sha1::{Digest,Sha1};
    let input=zeroize::Zeroizing::new(format!("nonce={nonce}&{secret}"));
    let signature=B64.encode(Sha1::digest(input.as_bytes()));
    location.query_pairs_mut().append_pair("clientSign",&signature);
    Ok(location)
}
fn session_exchange_error(service:LoginService,status:reqwest::StatusCode)->String {
    let label=if service==LoginService::Speaker {"小爱"}else{"米家"};
    let reason=if status.is_redirection(){"登录回调跳转未完成"}
        else if matches!(status.as_u16(),401|403){"登录授权未通过"}
        else if status.is_server_error(){"登录服务暂不可用"}else{"登录会话交换失败"};
    // Only a fixed label and HTTP status. Never include the signed callback,
    // response body, cookies or account identifiers in UI errors or logs.
    format!("{label}{reason}（HTTP {}），请重新扫码",status.as_u16())
}

impl Cloud {
    pub fn new(root:PathBuf,cache:PathBuf)->Result<Self,String>{Self::new_service(root,cache,LoginService::MiHome)}
    pub(crate) fn new_speaker(root:PathBuf,cache:PathBuf)->Result<Self,String>{Self::new_service(root,cache,LoginService::Speaker)}
    fn new_service(root:PathBuf,cache:PathBuf,service:LoginService)->Result<Self,String>{
        let redirects=reqwest::redirect::Policy::custom(|attempt|{if attempt.previous().len()>6 {attempt.error("redirect limit")}else if official_url(attempt.url().as_str()).is_ok(){attempt.follow()}else{attempt.stop()}});
        let client=Client::builder().user_agent(AGENT).timeout(Duration::from_secs(18)).redirect(redirects).build().map_err(|_|"无法初始化网络")?;
        let public=Client::builder().user_agent("HomePanel/0.1.1").timeout(Duration::from_secs(18)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_|"无法初始化图片网络")?;
        let session=storage::load_session(&root).unwrap_or(None);let has_saved_session=session.is_some();
        let (login_tx,_)=watch::channel(LoginStatus::default());
        Ok(Self{session:RwLock::new(session),devices:Mutex::new(HashMap::new()),homes:Mutex::new(vec![]),epoch:AtomicU64::new(0),login:Mutex::new(LoginStatus::default()),login_tx,has_saved_session,client,public,root,cache,service,labels:Mutex::new(HashMap::new()),catalog:Mutex::new(HashMap::new()),catalog_gate:Mutex::new(()),read_gate:Semaphore::new(4),device_gates:Mutex::new(HashMap::new())})
    }
    pub fn subscribe_login(&self)->watch::Receiver<LoginStatus>{self.login_tx.subscribe()}
    async fn publish_login(&self,status:LoginStatus){*self.login.lock().await=status.clone();self.login_tx.send_replace(status);}
    pub async fn cancel_login(&self)->Result<(),String>{
        self.epoch.fetch_add(1,Ordering::SeqCst);
        let status=LoginStatus::default();*self.login.lock().await=status.clone();self.login_tx.send_replace(status);Ok(())
    }
    pub async fn begin_login(&self)->Result<(u64,String,Client,Arc<Jar>),String>{
        let generation=self.epoch.fetch_add(1,Ordering::SeqCst)+1;
        let jar=Arc::new(Jar::default());
        let service=self.service;
        let redirects=reqwest::redirect::Policy::custom(move |attempt|{if attempt.previous().len()>6{attempt.error("redirect limit")}else if login_url(attempt.url().as_str(),service).is_ok(){attempt.follow()}else{attempt.stop()}});
        let client=Client::builder().cookie_provider(jar.clone()).user_agent(AGENT).timeout(Duration::from_secs(18)).redirect(redirects).build().map_err(|_|"无法初始化登录请求")?;
        let (qs,callback)=if service==LoginService::Speaker {
            // Ask Passport for the current service callback instead of guessing a
            // Xiaomiio callback or copying private mobile application secrets.
            let bootstrap=client.get("https://account.xiaomi.com/pass/serviceLogin").query(&[("sid",service.sid()),("_json","true")]).send().await.map_err(network_error)?;
            if !bootstrap.status().is_success(){return Err("无法获取小爱服务登录入口".into());}
            let data=parse_account(&bootstrap.text().await.map_err(network_error)?)?;
            let callback=data["callback"].as_str().ok_or("小爱服务暂未提供扫码入口，请稍后重试")?.to_owned();
            login_url(&callback,service)?;
            let qs=data["qs"].as_str().ok_or("小爱登录参数缺失，请稍后重试")?.to_owned();
            (qs,callback)
        } else {("%3Fsid%3Dxiaomiio%26_json%3Dtrue".into(),"https://sts.api.io.mi.com/sts".into())};
        let response=client.get("https://account.xiaomi.com/longPolling/loginUrl").query(&[("_qrsize","480"),("qs",qs.as_str()),("callback",callback.as_str()),("_hasLogo","false"),("sid",service.sid()),("serviceParam",""),("_locale","zh_CN"),("_dc",&now_ms().to_string())]).send().await.map_err(network_error)?;
        if !response.status().is_success(){return Err("无法获取登录二维码，请稍后重试".into());}
        let value=parse_account(&response.text().await.map_err(network_error)?)?;
        let qr:QrResponse=serde_json::from_value(value).map_err(|_|"小米暂未返回可用二维码，请稍后重试")?;
        let image_url=login_url(&qr.qr,service)?;login_url(&qr.lp,service)?;
        let response=client.get(image_url).send().await.map_err(network_error)?;
        if !response.status().is_success(){return Err("二维码图片加载失败".into());}
        let mime=response.headers().get("content-type").and_then(|x|x.to_str().ok()).unwrap_or("image/png").split(';').next().unwrap_or("image/png").to_string();
        if !matches!(mime.as_str(),"image/png"|"image/jpeg"|"image/webp"){return Err("二维码图片类型无效".into());}
        let bytes=response.bytes().await.map_err(network_error)?;if bytes.len()>1_000_000{return Err("二维码响应过大".into());}
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("登录已取消".into());}
        let seconds=if qr.timeout>3600{qr.timeout/1000}else{qr.timeout}.clamp(30,300);
        self.publish_login(LoginStatus{stage:"waiting".into(),qr:Some(format!("data:{mime};base64,{}",B64.encode(&bytes))),expires_at:Some(now_ms()+seconds*1000),message:if service==LoginService::Speaker {"请使用支持小米账号扫码的 App，确认小爱音箱服务授权"}else{"请使用米家 App 扫码，并在手机上确认登录"}.into()}).await;
        Ok((generation,qr.lp,client,jar))
    }
    pub async fn poll_login(&self,generation:u64,url:String,client:Client,jar:Arc<Jar>){
        let outcome=self.poll_inner(generation,&url,&client,&jar).await;
        if self.epoch.load(Ordering::SeqCst)!=generation{return;}
        if let Err(message)=outcome{self.publish_login(LoginStatus{stage:if message.contains("过期"){"expired"}else{"error"}.into(),message,..Default::default()}).await;}
    }
    async fn poll_inner(&self,generation:u64,url:&str,client:&Client,jar:&Arc<Jar>)->Result<(),String>{
        loop{
            if self.epoch.load(Ordering::SeqCst)!=generation{return Ok(());}
            let expires=self.login.lock().await.expires_at.unwrap_or(0);
            if now_ms()>expires{return Err("二维码已过期，请刷新后重新扫码".into());}
            let response=match client.get(url).timeout(Duration::from_secs(10)).send().await{Ok(r)=>r,Err(e)if e.is_timeout()=>continue,Err(e)=>return Err(network_error(e))};
            if response.status().as_u16()==408{continue;}
            if !response.status().is_success(){return Err("登录验证失败，请重新扫码".into());}
            let data=parse_account(&response.text().await.map_err(network_error)?)?;
            if data["userId"].is_null(){if data["code"].as_i64().is_some_and(|c|c!=0){return Err("二维码已过期或未获手机确认，请重新扫码".into());}tokio::time::sleep(Duration::from_millis(750)).await;continue;}
            let user_id=string_id(&data["userId"]);
            let ssecurity=data["ssecurity"].as_str().ok_or("登录未完成，请重新扫码")?.to_string();
            let location=session_exchange_url(&data,self.service)?;
            let response=client.get(location.clone()).send().await.map_err(network_error)?;
            if !response.status().is_success(){return Err(session_exchange_error(self.service,response.status()));}
            let cookie_url=if self.service==LoginService::Speaker {location} else {url::Url::parse("https://api.io.mi.com/").unwrap()};
            let cookies=jar.cookies(&cookie_url).and_then(|x|x.to_str().ok().map(str::to_string)).unwrap_or_default();
            let token=cookies.split(';').filter_map(|x|x.trim().split_once('=')).find(|(k,_)|*k=="serviceToken").map(|(_,v)|v.to_string()).or_else(||response.cookies().find(|c|c.name()=="serviceToken").map(|c|c.value().to_string())).ok_or("小米未返回设备访问会话，请重新扫码")?;
            let session=Session{user_id,ssecurity,service_token:token,agent:AGENT.into()};
            // Recheck under the session write lock: cancel/logout must not allow
            // a late long-poll result to restore a removed account.
            let mut current=self.session.write().await;
            if self.epoch.load(Ordering::SeqCst)!=generation{return Ok(());}
            storage::save_session(&self.root,&session)?;*current=Some(session);drop(current);
            if self.epoch.load(Ordering::SeqCst)!=generation{return Ok(());}
            self.devices.lock().await.clear();self.homes.lock().await.clear();
            if self.epoch.load(Ordering::SeqCst)!=generation{return Ok(());}
            self.publish_login(LoginStatus{stage:"success".into(),message:if self.service==LoginService::Speaker {"已连接小爱音箱服务"}else{"已连接米家账号"}.into(),..Default::default()}).await;
            return Ok(());
        }
    }
    pub async fn logout(&self)->Result<(),String>{self.epoch.fetch_add(1,Ordering::SeqCst);let mut session=self.session.write().await;storage::clear_session(&self.root)?;*session=None;self.devices.lock().await.clear();self.homes.lock().await.clear();let status=LoginStatus::default();*self.login.lock().await=status.clone();self.login_tx.send_replace(status);Ok(())}
    pub(crate) async fn expire_session(&self,generation:u64)->Result<(),String>{
        let mut session=self.session.write().await;
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
        storage::clear_session(&self.root)?;*session=None;drop(session);
        self.devices.lock().await.clear();self.homes.lock().await.clear();
        let status=LoginStatus{stage:"error".into(),message:"登录已失效，请重新扫码".into(),..Default::default()};*self.login.lock().await=status.clone();self.login_tx.send_replace(status);Ok(())
    }
    pub async fn api(&self,region:&str,path:&str,data:Value)->Result<Value,String>{
        self.api_request(region,path,data,true).await
    }
    pub async fn query_plugin_metadata(&self,region:&str,data:Value)->Result<Value,String>{
        self.api_request(region,"/v2/plugin/fetch_plugin",data,false).await
    }
    pub async fn query_plugin_sdk(&self,region:&str,data:Value)->Result<Value,String>{
        self.api_request(region,"/v2/plugin/fetch_sdk",data,false).await
    }
    pub(crate) async fn plugin_speaker_status(&self,region:&str,lease:&Device,expected_epoch:u64)->Result<Value,&'static str>{
        if !crate::speaker_service::eligible(lease)||!lease.spec_urn.starts_with("urn:miot-spec-v2:device:speaker:")||lease.spec_status!="ready"{return Err("SPEAKER_DEVICE_UNSUPPORTED");}
        let gate=self.device_gate(&lease.id).await;let _operation=gate.lock().await;
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let current=self.device(&lease.id).await.map_err(|_|"DEVICE_UNAVAILABLE")?;
        if current.model!=lease.model||current.spec_urn!=lease.spec_urn{return Err("DEVICE_CHANGED");}
        if !current.online{return Err("DEVICE_OFFLINE");}
        let result=self.api_request_at(region,"/v2/remote/ubus",json!({"did":lease.id,"path":"mediaplayer","method":"player_get_play_status","message":{}}),true,Some(expected_epoch))
            .await.map_err(|_|"SPEAKER_STATE_UNAVAILABLE")?;
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let current=self.device(&lease.id).await.map_err(|_|"DEVICE_UNAVAILABLE")?;
        if current.model!=lease.model||current.spec_urn!=lease.spec_urn{return Err("DEVICE_CHANGED");}Ok(result)
    }
    pub(crate) async fn plugin_timers(&self,region:&str,lease:&Device,expected_epoch:u64)->Result<Value,&'static str>{
        if lease.spec_status!="ready"{return Err("SPEC_UNAVAILABLE");}
        let gate=self.device_gate(&lease.id).await;let _operation=gate.lock().await;
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let current=self.device(&lease.id).await.map_err(|_|"DEVICE_UNAVAILABLE")?;
        if current.model!=lease.model||current.spec_urn!=lease.spec_urn{return Err("DEVICE_CHANGED");}
        let result=self.api_request_at(region,"/scene/list",json!({"did":lease.id,"st_id":8,"type":0}),true,Some(expected_epoch)).await.map_err(|_|"TIMERS_UNAVAILABLE")?;
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let current=self.device(&lease.id).await.map_err(|_|"DEVICE_UNAVAILABLE")?;
        if current.model!=lease.model||current.spec_urn!=lease.spec_urn{return Err("DEVICE_CHANGED");}Ok(result)
    }
    pub(crate) async fn plugin_privacy_status(&self,region:&str,lease:&Device,expected_epoch:u64)->Result<bool,&'static str>{
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let current=self.device(&lease.id).await.map_err(|_|"DEVICE_UNAVAILABLE")?;
        if current.model!=lease.model||current.spec_urn!=lease.spec_urn{return Err("DEVICE_CHANGED");}
        // Read the phone SDK's existing consent record. Never create or update
        // a consent record, and never treat a missing record as agreement.
        let raw=self.api_request_at(region,"/device/batchdevicedatas",json!([{ "did":lease.id,"props":["prop.s_auth_config"] }]),false,Some(expected_epoch))
            .await.map_err(|_|"PRIVACY_STATUS_UNAVAILABLE")?;
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let current=self.device(&lease.id).await.map_err(|_|"DEVICE_UNAVAILABLE")?;
        if current.model!=lease.model||current.spec_urn!=lease.spec_urn{return Err("DEVICE_CHANGED");}
        crate::plugin_services::privacy_authorized(&raw,&lease.id)
    }
    async fn api_request(&self,region:&str,path:&str,data:Value,expire:bool)->Result<Value,String>{
        self.api_request_at(region,path,data,expire,None).await
    }
    async fn api_request_at(&self,region:&str,path:&str,data:Value,expire:bool,expected_epoch:Option<u64>)->Result<Value,String>{
        let generation=self.epoch.load(Ordering::SeqCst);
        if expected_epoch.is_some_and(|expected|expected!=generation){return Err("账号状态已改变，请重新刷新".into());}
        let session=self.session.read().await.clone().ok_or("AUTH_REQUIRED: 请先扫码连接米家账号")?;
        // A plugin lease must never borrow a replacement account's credentials
        // after waiting for the session lock. Keep this snapshot for the request.
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
        if !path.starts_with('/')||path.contains('?')||path.contains("..") {return Err("无效的设备请求".into());}
        let nonce=crypto::nonce();let fields=crypto::fields_at(path,&data.to_string(),&session.ssecurity,&nonce)?;
        let response=self.client.post(format!("{}{path}",region_url(region)?)).header("Cookie",format!("userId={}; serviceToken={}; yetAnotherServiceToken={}; locale=zh_CN; timezone=GMT%2B08%3A00; channel=MI_APP_STORE",session.user_id,session.service_token,session.service_token)).header("Accept-Encoding","identity").header("MIOT-ENCRYPT-ALGORITHM","ENCRYPT-RC4").header("x-xiaomi-protocal-flag-cli","PROTOCAL-HTTP2").form(&fields).send().await.map_err(network_error)?;
        let status=response.status();if matches!(status.as_u16(),401|403){if expire{self.expire_session(generation).await?;}return Err("AUTH_REQUIRED: 登录已失效，请重新扫码".into());}if !status.is_success(){return Err(format!("小米服务暂不可用（HTTP {}）",status.as_u16()));}
        let body=if expire {response.text().await.map_err(network_error)?}else{
            if response.content_length().is_some_and(|size|size>1024*1024){return Err("插件元数据响应过大".into());}
            let mut response=response;let mut bytes=Vec::new();
            while let Some(chunk)=response.chunk().await.map_err(network_error)? {
                if bytes.len().saturating_add(chunk.len())>1024*1024{return Err("插件元数据响应过大".into());}
                bytes.extend_from_slice(&chunk);
            }
            String::from_utf8(bytes).map_err(|_|"无法解析插件服务响应")?
        };
        let value=match serde_json::from_str::<Value>(&body){Ok(v)=>v,Err(_)=>serde_json::from_slice(&crypto::decrypt(&session.ssecurity,&nonce,&body)?).map_err(|_|"无法解析设备服务响应")?};
        let code=value["code"].as_i64().ok_or("设备服务响应格式无效")?;
        if matches!(code,-10001|-10002|-10003|-10004|401|403){if expire{self.expire_session(generation).await?;}return Err("AUTH_REQUIRED: 登录已失效，请重新扫码".into());}
        if code!=0{return Err(format!("设备服务拒绝了请求（{code}）"));}
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
        Ok(value["result"].clone())
    }
    pub async fn discover(&self,region:&str)->Result<(),String>{
        let generation=self.epoch.load(Ordering::SeqCst);
        let owner=self.session.read().await.as_ref().map(|s|s.user_id.clone()).ok_or("AUTH_REQUIRED: 请先扫码")?;
        let result=self.api(region,"/v2/homeroom/gethome",json!({"fg":true,"fetch_share":true,"fetch_share_dev":true,"limit":300,"app_ver":7})).await?;
        let mut homes:Vec<Home>=result["homelist"].as_array().into_iter().flatten().map(|h|parse_home(h,&owner)).collect();
        // Shared homes are reported separately by Xiaomi's device-count endpoint.
        if let Ok(shared)=self.api(region,"/v2/user/get_device_cnt",json!({"fetch_own":true,"fetch_share":true})).await{
            for h in shared["share"]["share_family"].as_array().into_iter().flatten(){let id=string_id(&h["home_id"]);if !homes.iter().any(|x|x.id==id){homes.push(Home{id,name:h["home_name"].as_str().unwrap_or("共享家庭").into(),owner:string_id(&h["home_owner"]),rooms:vec![]});}}
        }
        let old=self.devices.lock().await.clone();let mut devices=HashMap::new();
        for home in &homes{
            let mut cursor=String::new();let mut seen=HashSet::new();
            for page_index in 0..30{
                let payload=json!({"home_owner":home.owner.parse::<u64>().unwrap_or(0),"home_id":home.id.parse::<u64>().unwrap_or(0),"limit":200,"get_split_device":true,"support_smart_home":true,"start_did":cursor});
                let page=self.api(region,"/v2/home/home_device_list",payload).await?;
                let rows=page["device_info"].as_array().or_else(||page["list"].as_array()).cloned().unwrap_or_default();
                for row in &rows{
                    let id=string_id(&row["did"]);if id.is_empty(){continue;}
                    let model=row["model"].as_str().unwrap_or_default().to_string();
                    let room=home.rooms.iter().find(|r|r.dids.contains(&id)).map(|r|r.name.clone()).unwrap_or_else(||row["room_name"].as_str().unwrap_or("未分配房间").into());
                    let mut d=old.get(&id).filter(|d|d.model==model).cloned().unwrap_or_default();
                    d.id=id.clone();d.name=row["name"].as_str().unwrap_or("未命名设备").into();d.kind=kind_from_spec(&d.spec_urn,&model);d.model=model;d.home_id=home.id.clone();d.room=room;d.online=row["isOnline"].as_bool().or_else(||row["isOnline"].as_i64().map(|x|x!=0)).unwrap_or(false);
                    if d.spec_status.is_empty(){d.spec_status="unloaded".into();}
                    devices.insert(id,d);
                }
                let more=page["has_more"].as_bool().or_else(||page["has_more"].as_i64().map(|n|n!=0)).unwrap_or(false);
                if !more {break;}
                if page_index==29{return Err("设备列表超过分页上限，已停止，避免显示不完整列表".into());}
                let next=string_id(&page["next_start_did"]);let next=if next.is_empty(){rows.last().map(|r|string_id(&r["did"])).unwrap_or_default()}else{next};
                if next.is_empty()||!seen.insert(next.clone()){return Err("设备列表分页异常，已停止，避免显示不完整列表".into());}cursor=next;
            }
        }
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
        let mut current=self.devices.lock().await;
        for device in devices.values_mut() {
            if let Some(latest)=current.get(&device.id) {
                let mut merged=latest.clone();crate::state_sync::merge(&mut merged,device.clone());*device=merged;
            }
        }
        *current=devices;drop(current);*self.homes.lock().await=homes;Ok(())
    }
    pub async fn device(&self,id:&str)->Result<Device,String>{self.devices.lock().await.get(id).cloned().ok_or("设备不在当前账号列表中，请刷新".into())}
    pub async fn load_spec(&self,id:&str)->Result<Device,String>{
        let d=self.device(id).await?;
        if d.spec_status=="unsupported" { return Ok(d); }
        if d.spec_status=="ready" {
            if let Some(labels)=self.load_labels(&d.spec_urn).await {
                let mut map=self.devices.lock().await;
                let entry=map.get_mut(id).filter(|entry|entry.model==d.model).ok_or("设备已移除")?;
                apply_spec_labels(&mut entry.properties,&mut entry.actions,&labels);
                return Ok(entry.clone());
            }
            return Ok(d);
        }
        let urn=self.resolve_model(&d.model).await?;
        let Some(urn)=urn else {let mut map=self.devices.lock().await;let x=map.get_mut(id).ok_or("设备已移除")?;x.spec_status="unsupported".into();return Ok(x.clone());};
        let file=self.cache.join("specs").join(format!("{}.json",hash_name(urn.as_bytes())));
        let cached=std::fs::read(&file).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok());
        let spec=match cached{Some(v)=>v,None=>{let r=self.public.get("https://miot-spec.org/miot-spec-v2/instance").query(&[("type",&urn)]).send().await.map_err(network_error)?;if !r.status().is_success(){return Err("设备功能定义暂不可用".into());}let v=r.json::<Value>().await.map_err(network_error)?;if !v["services"].is_array(){return Err("设备功能定义格式无效".into());}let _=storage::atomic_write(&file,v.to_string().as_bytes());v}};
        let (mut properties,mut actions)=parse_spec(&spec);
        if let Some(labels)=self.load_labels(&urn).await { apply_spec_labels(&mut properties,&mut actions,&labels); }
        let mut map=self.devices.lock().await;let entry=map.get_mut(id).filter(|entry|entry.model==d.model).ok_or("设备已移除")?;
        entry.properties=properties;entry.actions=actions;entry.kind=kind_from_spec(&urn,&entry.model);entry.spec_urn=urn;entry.spec_status="ready".into();Ok(entry.clone())
    }
    pub fn plugin_instance(&self, device: &Device) -> Result<Value, String> {
        let file = self.cache.join("specs").join(format!("{}.json",hash_name(device.spec_urn.as_bytes())));
        let bytes = std::fs::read(file).map_err(|_| "公开设备规格尚未缓存，请刷新设备")?;
        if bytes.len() > 48 * 1024 { return Err("设备规格超过插件接口限制".into()); }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "公开设备规格缓存损坏")?;
        if value["type"].as_str() != Some(device.spec_urn.as_str()) || !value["services"].is_array() {
            return Err("设备规格与当前设备不一致".into());
        }
        Ok(value)
    }
    async fn load_labels(&self,urn:&str)->Option<Value> {
        if urn.is_empty() { return None; }
        {
            let labels=self.labels.lock().await;
            if let Some((at,value))=labels.get(urn) {
                if value.is_some() || at.elapsed()<Duration::from_secs(300) { return value.clone(); }
            }
        }
        let path=self.cache.join("spec-labels-zh").join(format!("{}.json",hash_name(urn.as_bytes())));
        let valid=|value:&Value|value["data"]["zh_cn"].as_object().is_some_and(|table|!table.is_empty());
        let cached=std::fs::read(&path).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok()).filter(&valid);
        let fresh=std::fs::metadata(&path).ok().and_then(|m|m.modified().ok()).and_then(|at|at.elapsed().ok()).is_some_and(|age|age.as_secs()<7*86400);
        let value=if fresh && cached.is_some() { cached } else {
            let fetched=async {
                let response=self.public.get("https://miot-spec.org/instance/v2/multiLanguage")
                    .query(&[("urn",urn)]).timeout(Duration::from_secs(3)).send().await.ok()?;
                if !response.status().is_success() { return None; }
                let value=response.json::<Value>().await.ok()?;
                valid(&value).then_some(value)
            }.await;
            if let Some(value)=fetched.as_ref() { let _=storage::atomic_write(&path,value.to_string().as_bytes()); }
            fetched.or(cached)
        };
        self.labels.lock().await.insert(urn.into(),(Instant::now(),value.clone()));
        value
    }

    async fn resolve_model(&self,model:&str)->Result<Option<String>,String>{
        {let map=self.catalog.lock().await;if !map.is_empty(){return Ok(map.get(model).cloned());}}
        let _gate=self.catalog_gate.lock().await;
        {let map=self.catalog.lock().await;if !map.is_empty(){return Ok(map.get(model).cloned());}}
        let path=self.cache.join("spec-catalog.json");
        let cached=std::fs::metadata(&path).ok().and_then(|m|m.modified().ok()).and_then(|t|t.elapsed().ok()).filter(|d|d.as_secs()<7*86400).and_then(|_|std::fs::read(&path).ok()).and_then(|b|serde_json::from_slice::<Value>(&b).ok());
        let data=match cached{Some(v)=>v,None=>{let r=self.public.get("https://miot-spec.org/miot-spec-v2/instances?status=all").send().await.map_err(network_error)?;if !r.status().is_success(){return Err("设备规格目录加载失败".into());}let v=r.json::<Value>().await.map_err(network_error)?;if !v["instances"].is_array(){return Err("设备规格目录格式无效".into());}let _=storage::atomic_write(&path,v.to_string().as_bytes());v}};
        let mut map=self.catalog.lock().await;
        for item in data["instances"].as_array().into_iter().flatten(){if let (Some(m),Some(t))=(item["model"].as_str(),item["type"].as_str()){map.insert(m.into(),t.into());}}
        Ok(map.get(model).cloned())
    }
    pub async fn read_device(&self,region:&str,id:&str)->Result<Device,String>{
        let gate=self.device_gate(id).await;
        let _operation=gate.lock().await;
        self.read_device_locked(region,id).await
    }
    async fn device_gate(&self,id:&str)->Arc<Mutex<()>> {
        self.device_gates.lock().await.entry(id.into()).or_insert_with(||Arc::new(Mutex::new(()))).clone()
    }
    async fn fetch_state(&self,region:&str,id:&str,targets:&[(u32,u32)],fresh:bool)->Result<Value,String> {
        tokio::time::timeout(Duration::from_secs(6),self.api(region,"/miotspec/prop/get",crate::state_sync::request(id,targets,fresh)))
            .await.map_err(|_|"状态读取超时".to_string())?
    }
    // Caller holds the per-device operation lock: old reads cannot race confirmed writes.
    async fn read_device_locked(&self,region:&str,id:&str)->Result<Device,String>{
        let generation=self.epoch.load(Ordering::SeqCst);
        let _permit=self.read_gate.acquire().await.map_err(|_|"状态读取已停止")?;
        let d=self.load_spec(id).await?;if !d.online||d.spec_status!="ready"{return Ok(d);}
        let readable:Vec<_>=d.properties.iter().filter(|p|p.readable).map(|p|(p.siid,p.piid)).collect();
        for batch in readable.chunks(12){
            let fresh=self.fetch_state(region,id,batch,true).await;
            if let Err(error)=&fresh {if error.starts_with("AUTH_REQUIRED:"){return Err(error.clone());}}
            let missing=crate::state_sync::missing(id,batch,&fresh);
            let cached=if missing.is_empty(){Ok(json!([]))}else{self.fetch_state(region,id,&missing,false).await};
            if let Err(error)=&cached {if error.starts_with("AUTH_REQUIRED:"){return Err(error.clone());}}
            let mut map=self.devices.lock().await;
            if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
            let current=map.get_mut(id).filter(|entry|entry.model==d.model).ok_or("设备已移除")?;
            crate::state_sync::apply(current,batch,&fresh,&cached,now_ms());
        }
        self.device(id).await
    }
    pub async fn write(&self,region:&str,id:&str,siid:u32,piid:u32,value:Value)->Result<Device,String>{
        let generation=self.epoch.load(Ordering::SeqCst);
        self.write_inner(region,id,siid,piid,value,generation,None).await
    }
    /// Reuse the native SPEC validation and direct readback while retaining the
    /// plugin's original account/device lease across lock waits and every request.
    pub(crate) async fn plugin_write(&self,region:&str,lease:&Device,expected_epoch:u64,
        target:&crate::plugin_bridge_extensions::PropertyTarget,value:Value,writes_allowed:bool)->Result<Device,&'static str>{
        if !writes_allowed{return Err("READ_ONLY_SESSION");}
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let property=crate::plugin_bridge_extensions::checked_property(lease,target,true)?;
        if !property.readable{return Err("READBACK_UNSUPPORTED");}
        validate_property(property,&value).map_err(|_|"INVALID_PROPERTY_VALUE")?;
        self.write_inner(region,&lease.id,target.siid,target.piid,value,expected_epoch,Some(lease)).await.map_err(|error|{
            if self.epoch.load(Ordering::SeqCst)!=expected_epoch || error.starts_with("AUTH_REQUIRED:") {"SESSION_REVOKED"}
            else {match error.as_str(){
                "DEVICE_CHANGED"=>"DEVICE_CHANGED","SPEC_UNAVAILABLE"=>"SPEC_UNAVAILABLE",
                "UNKNOWN_PROPERTY"=>"UNKNOWN_PROPERTY","AMBIGUOUS_PROPERTY"=>"AMBIGUOUS_PROPERTY",
                "PERMISSION_DENIED"=>"PERMISSION_DENIED","READBACK_UNSUPPORTED"=>"READBACK_UNSUPPORTED",
                _=>"WRITE_UNCONFIRMED",
            }}
        })
    }
    async fn write_inner(&self,region:&str,id:&str,siid:u32,piid:u32,value:Value,generation:u64,lease:Option<&Device>)->Result<Device,String>{
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
        let gate=self.device_gate(id).await;
        let _operation=gate.lock().await;
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
        // Plugin devices already loaded their SPEC before host launch. Do not
        // resolve a replacement account's model while validating an old lease.
        let d=if lease.is_some(){self.device(id).await?}else{self.load_spec(id).await?};
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
        if lease.is_some_and(|expected|expected.id!=id || expected.model!=d.model || expected.spec_urn!=d.spec_urn){return Err("DEVICE_CHANGED".into());}
        if !d.online{return Err("设备已离线，无法控制".into());}
        let prop=if lease.is_some(){
            let target=crate::plugin_bridge_extensions::PropertyTarget{siid,piid,miid:None};
            let property=crate::plugin_bridge_extensions::checked_property(&d,&target,true).map_err(str::to_owned)?;
            if !property.readable{return Err("READBACK_UNSUPPORTED".into());}property
        }else{d.properties.iter().find(|p|p.siid==siid&&p.piid==piid).ok_or("设备没有这个属性")?};
        validate_property(prop,&value)?;
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
        let target=crate::control::PropertyTarget{did:id,siid,piid,time_period:crate::time_range::recognizes(prop)};
        let write=self.api_request_at(region,"/miotspec/prop/set",json!({"params":[{"did":id,"siid":siid,"piid":piid,"value":value}]}),true,Some(generation));
        let confirmed=crate::control::write_and_confirm(&target,&value,prop.readable,write,||async {
            let _permit=self.read_gate.acquire().await.map_err(|_|"状态读取已停止")?;
            if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
            let current=self.device(id).await?;
            if current.model!=d.model || current.spec_urn!=d.spec_urn{return Err("设备功能已改变，请刷新".into());}
            self.api_request_at(region,"/miotspec/prop/get",target.fresh_request(),true,Some(generation)).await
        },Duration::from_secs(8),&[160,400,800,1200,1800]).await?;
        let mut map=self.devices.lock().await;
        if self.epoch.load(Ordering::SeqCst)!=generation{return Err("账号状态已改变，请重新刷新".into());}
        let current=map.get_mut(id).filter(|entry|entry.model==d.model&&entry.spec_urn==d.spec_urn).ok_or("设备已移除或功能已改变")?;
        let property=current.properties.iter_mut().find(|p|p.siid==siid&&p.piid==piid).ok_or("设备属性已改变，请刷新")?;
        property.value=Some(confirmed);property.error=None;property.checked_at=Some(now_ms());property.source=StateSource::Device;current.updated_at=property.checked_at;
        Ok(current.clone())
    }
    pub async fn action(&self,region:&str,id:&str,siid:u32,aiid:u32)->Result<Device,String>{
        let gate=self.device_gate(id).await;
        let _operation=gate.lock().await;
        let d=self.load_spec(id).await?;if !d.online{return Err("设备已离线".into());}if !d.actions.iter().any(|a|a.siid==siid&&a.aiid==aiid){return Err("此操作不在日常控制列表中".into());}
        let result=self.api(region,"/miotspec/action",json!({"params":{"did":id,"siid":siid,"aiid":aiid,"in":[]}})).await?;
        if result["code"].as_i64().unwrap_or(-1)!=0{return Err("设备未接受这个操作".into());}
        // Allow the device to change state; subsequent UI polls never resend the action.
        tokio::time::sleep(Duration::from_millis(350)).await;
        self.read_device_locked(region,id).await
    }
    /// Restricted phone-plugin action transport. The caller supplies a device
    /// lease and its account generation; no plugin-provided identifier or URL is
    /// accepted. Preserve and validate the original action output before replying.
    pub(crate) async fn plugin_action(&self,region:&str,lease:&Device,expected_epoch:u64,
        target:&crate::plugin_bridge_extensions::ActionTarget,inputs:&[Value],writes_allowed:bool)->Result<Value,&'static str>{
        use crate::plugin_bridge_extensions::{checked_action,action_reply};
        if !writes_allowed{return Err("READ_ONLY_SESSION");}
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let gate=self.device_gate(&lease.id).await;
        let _operation=gate.lock().await;
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let current=self.device(&lease.id).await.map_err(|_|"DEVICE_UNAVAILABLE")?;
        if current.model!=lease.model || current.spec_urn!=lease.spec_urn{return Err("DEVICE_CHANGED");}
        let instance=self.plugin_instance(&current).map_err(|_|"SPEC_UNAVAILABLE")?;
        checked_action(&current,&instance,target,inputs,writes_allowed)?;
        let result=self.api_request_at(region,"/miotspec/action",
            json!({"params":{"did":lease.id,"siid":target.siid,"aiid":target.aiid,"in":inputs}}),
            true,Some(expected_epoch)).await.map_err(|error|{
                if self.epoch.load(Ordering::SeqCst)!=expected_epoch || error.starts_with("AUTH_REQUIRED:") {"SESSION_REVOKED"}
                else {"ACTION_REQUEST_FAILED"}
            })?;
        if self.epoch.load(Ordering::SeqCst)!=expected_epoch{return Err("SESSION_REVOKED");}
        let latest=self.device(&lease.id).await.map_err(|_|"DEVICE_UNAVAILABLE")?;
        if latest.model!=lease.model || latest.spec_urn!=lease.spec_urn{return Err("DEVICE_CHANGED");}
        checked_action(&latest,&instance,target,inputs,writes_allowed)?;
        // The output is a command acknowledgment, never a fabricated state
        // confirmation. Normal subscriptions obtain subsequent device values.
        action_reply(&result,target,&lease.id)
    }
    pub async fn image(&self,id:&str,approved:&[ApprovedImage])->Result<Option<String>,String>{
        let d=self.device(id).await?;let Some(source)=approved.iter().find(|v|v.model==d.model&&!v.permission_url.trim().is_empty())else{return Ok(None);};
        let url=official_url(&source.url)?;
        let permission=url::Url::parse(&source.permission_url).map_err(|_|"图片授权依据地址无效")?;if permission.scheme()!="https"{return Err("图片缺少有效授权依据".into());}
        let key=hash_name(source.url.as_bytes());let path=self.cache.join("images").join(format!("{key}.json"));
        if let Ok(bytes)=std::fs::read(&path){if let Ok(uri)=serde_json::from_slice::<String>(&bytes){return Ok(Some(uri));}}
        // Dedicated client has no cookie jar and never sends account credentials.
        let response=self.public.get(url).timeout(Duration::from_secs(10)).send().await.map_err(network_error)?;
        if !response.status().is_success(){return Err("设备图片加载失败".into());}if response.content_length().unwrap_or(0)>2_000_000{return Err("设备图片过大".into());}
        let mime=response.headers().get("content-type").and_then(|v|v.to_str().ok()).unwrap_or_default().split(';').next().unwrap_or_default().to_string();
        if !matches!(mime.as_str(),"image/png"|"image/jpeg"|"image/webp"){return Err("设备图片类型不受支持".into());}
        let bytes=response.bytes().await.map_err(network_error)?;if bytes.len()>2_000_000{return Err("设备图片过大".into());}
        let data=format!("data:{mime};base64,{}",B64.encode(bytes));
        let total:u64=std::fs::read_dir(self.cache.join("images")).into_iter().flatten().flatten().filter_map(|e|e.metadata().ok()).map(|m|m.len()).sum();
        if total<32_000_000{let _=storage::atomic_write(&path,&serde_json::to_vec(&data).unwrap_or_default());}Ok(Some(data))
    }
}
fn parse_home(value:&Value,owner:&str)->Home{
    Home{id:string_id(&value["id"]),name:value["name"].as_str().unwrap_or("我的家").into(),owner:owner.into(),rooms:value["roomlist"].as_array().into_iter().flatten().map(|r|Room{id:string_id(&r["id"]),name:r["name"].as_str().unwrap_or("未命名房间").into(),dids:r["dids"].as_array().into_iter().flatten().map(string_id).collect()}).collect()}
}
fn hash_name(bytes:&[u8])->String{use sha2::{Digest,Sha256};format!("{:x}",Sha256::digest(bytes))}
#[cfg(test)]
mod plugin_action_tests {
    use super::*;
    use crate::plugin_bridge_extensions::{ActionTarget,PropertyTarget};

    fn absent_account() -> (Arc<Cloud>,PathBuf) {
        let root=std::env::temp_dir().join(format!("HomePanel-absent-action-test-{}-{}",std::process::id(),now_ms()));
        assert!(!root.exists());
        (Arc::new(Cloud::new(root.clone(),root.join("cache")).unwrap()),root)
    }
    fn lease() -> Device {
        Device{id:"fixture-device".into(),model:"homepanel.test.speaker".into(),
            spec_urn:"urn:miot-spec-v2:device:speaker:fixture".into(),online:true,spec_status:"ready".into(),..Default::default()}
    }
    fn write_lease() -> Device {
        let mut device=lease();
        device.properties.push(Property{siid:2,piid:1,format:"bool".into(),readable:true,writable:true,
            value:Some(json!(false)),..Default::default()});device
    }
    #[test]
    fn plugin_property_writes_retain_read_only_epoch_and_device_identity_guards() {
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let (cloud,root)=absent_account();let device=write_lease();let target=PropertyTarget{siid:2,piid:1,miid:None};
            cloud.devices.lock().await.insert(device.id.clone(),device.clone());
            assert_eq!(cloud.plugin_write("cn",&device,0,&target,json!(true),false).await.err(),Some("READ_ONLY_SESSION"));
            cloud.epoch.store(1,Ordering::SeqCst);
            // Even if the replacement account has the same device identifiers,
            // an old host must not adopt its generation and send a new request.
            assert_eq!(cloud.plugin_write("cn",&device,0,&target,json!(true),true).await.err(),Some("SESSION_REVOKED"));
            let mut changed=device.clone();changed.spec_urn="urn:miot-spec-v2:device:speaker:changed".into();
            cloud.devices.lock().await.insert(device.id.clone(),changed);
            assert_eq!(cloud.plugin_write("cn",&device,1,&target,json!(true),true).await.err(),Some("DEVICE_CHANGED"));
            assert_eq!(cloud.device(&device.id).await.unwrap().properties[0].value,Some(json!(false)));
            assert!(!root.exists());
        });
    }
    #[test]
    fn plugin_property_write_waiting_for_a_gate_cannot_switch_to_the_new_account() {
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let (cloud,root)=absent_account();let device=write_lease();let target=PropertyTarget{siid:2,piid:1,miid:None};
            cloud.devices.lock().await.insert(device.id.clone(),device.clone());
            let gate=cloud.device_gate(&device.id).await;let operation=gate.lock().await;
            let mut pending=Box::pin(cloud.plugin_write("cn",&device,0,&target,json!(true),true));
            assert!(tokio::time::timeout(Duration::from_millis(5),&mut pending).await.is_err());
            cloud.epoch.fetch_add(1,Ordering::SeqCst);drop(operation);
            assert_eq!(pending.await.err(),Some("SESSION_REVOKED"));
            assert_eq!(cloud.device(&device.id).await.unwrap().properties[0].value,Some(json!(false)));
            assert!(!root.exists());
        });
    }
    #[test]
    fn an_expected_epoch_is_rechecked_after_the_session_snapshot_lock() {
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let (cloud,root)=absent_account();let mut replacement=cloud.session.write().await;
            // This path is deliberately invalid, so even a broken generation
            // guard cannot send an HTTP request during the offline regression.
            let mut pending=Box::pin(cloud.api_request_at("cn","/offline?must-not-send",json!({}),true,Some(0)));
            assert!(tokio::time::timeout(Duration::from_millis(5),&mut pending).await.is_err());
            *replacement=Some(Session{user_id:"synthetic-only".into(),ssecurity:"invalid-offline-fixture".into(),
                service_token:"synthetic-only".into(),agent:"fixture".into()});
            cloud.epoch.fetch_add(1,Ordering::SeqCst);drop(replacement);
            assert_eq!(pending.await.err().as_deref(),Some("账号状态已改变，请重新刷新"));
            assert!(!root.exists());
        });
    }
    #[test]
    fn plugin_actions_reject_read_only_expired_and_changed_leases_before_network() {
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let (cloud,root)=absent_account();let device=lease();let target=ActionTarget{siid:3,aiid:1,miid:None};
            assert_eq!(cloud.plugin_action("cn",&device,0,&target,&[],false).await,Err("READ_ONLY_SESSION"));
            cloud.epoch.store(1,Ordering::SeqCst);
            assert_eq!(cloud.plugin_action("cn",&device,0,&target,&[],true).await,Err("SESSION_REVOKED"));
            let mut changed=device.clone();changed.model="another.fixture".into();
            cloud.devices.lock().await.insert(device.id.clone(),changed);
            assert_eq!(cloud.plugin_action("cn",&device,1,&target,&[],true).await,Err("DEVICE_CHANGED"));
            assert!(!root.exists());
        });
    }
    #[test]
    fn plugin_action_waiting_for_the_device_gate_rechecks_account_generation() {
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let (cloud,root)=absent_account();let device=lease();let target=ActionTarget{siid:3,aiid:1,miid:None};
            cloud.devices.lock().await.insert(device.id.clone(),device.clone());
            let gate=cloud.device_gate(&device.id).await;let operation=gate.lock().await;
            let mut pending=Box::pin(cloud.plugin_action("cn",&device,0,&target,&[],true));
            assert!(tokio::time::timeout(Duration::from_millis(5),&mut pending).await.is_err());
            cloud.epoch.fetch_add(1,Ordering::SeqCst);drop(operation);
            assert_eq!(pending.await,Err("SESSION_REVOKED"));
            assert!(!root.exists());
        });
    }
}
#[cfg(test)]
mod speaker_auth_tests {
    use super::*;
    #[test] fn speaker_qr_exchange_signs_the_exact_64_bit_nonce_and_encodes_the_signature() {
        for (nonce,expected) in [
            (json!(9007199254740993u64),"GtL+wrSL96Wg2H5spcQ/vSPoG54="),
            (json!("9007199254740993"),"GtL+wrSL96Wg2H5spcQ/vSPoG54="),
            (json!(9223372036854775807i64),"BljsvgDndjMbW0WWMIfr7/CAEMY="),
            (json!(-9223372036854775808i64),"jEhVWajNCy8fRJtQe6dcFq4c2Co="),
        ] {
            let raw=format!("&&&START&&&{}",json!({"location":"https://api2.mina.mi.com/sts?existing=keep%2Bvalue","nonce":nonce,"ssecurity":"synthetic-session-secret"}));
            let data=parse_account(&raw).unwrap();
            let url=session_exchange_url(&data,LoginService::Speaker).unwrap();
            let fields:Vec<_>=url.query_pairs().collect();
            assert_eq!(fields.len(),2);assert_eq!(fields[0].1,"keep+value");
            assert_eq!(fields[1].0,"clientSign");assert_eq!(fields[1].1,expected);
            assert!(url.as_str().contains("%3D"));assert!(!url.as_str().contains("synthetic-session-secret"));
        }
    }
    #[test] fn speaker_exchange_rejects_missing_or_lossy_nonces_and_untrusted_callbacks() {
        for nonce in [Value::Null,json!(true),json!(1.5),json!(""),json!("9.007199254740993e15"),json!(" 123"),json!({})] {
            let data=json!({"location":"https://api2.mina.mi.com/sts","nonce":nonce,"ssecurity":"synthetic-session-secret"});
            let error=session_exchange_url(&data,LoginService::Speaker).unwrap_err();
            assert!(!error.contains("synthetic-session-secret"));
        }
        let data=json!({"location":"https://attacker.example/sts?private=synthetic-only","nonce":1,"ssecurity":"synthetic-session-secret"});
        let error=session_exchange_url(&data,LoginService::Speaker).unwrap_err();
        assert!(!error.contains("private")&&!error.contains("synthetic"));
    }
    #[test] fn signed_callbacks_are_not_signed_twice_and_mihome_keeps_its_existing_flow() {
        let raw="https://api2.mina.mi.com/sts?clientSign=synthetic%2Bsignature%3D";
        assert_eq!(session_exchange_url(&json!({"location":raw}),LoginService::Speaker).unwrap().as_str(),raw);
        for suffix in ["clientSign=","clientSign=one&clientSign=two"] {
            assert!(session_exchange_url(&json!({"location":format!("https://api2.mina.mi.com/sts?{suffix}")}),LoginService::Speaker).is_err());
        }
        let raw="https://sts.api.io.mi.com/sts?existing=unchanged";
        assert_eq!(session_exchange_url(&json!({"location":raw}),LoginService::MiHome).unwrap().as_str(),raw);
    }
    #[test] fn exchange_diagnostics_distinguish_http_failure_stages_without_private_data() {
        for (status,reason) in [(302,"回调跳转"),(401,"授权未通过"),(403,"授权未通过"),(500,"服务暂不可用"),(400,"会话交换失败")] {
            let error=session_exchange_error(LoginService::Speaker,reqwest::StatusCode::from_u16(status).unwrap());
            assert!(error.contains(reason)&&error.contains(&format!("HTTP {status}")));
            assert!(!error.contains("https://")&&!error.contains("serviceToken")&&!error.contains("clientSign"));
        }
    }
    #[test] fn mico_and_mihome_logins_keep_separate_service_audiences() {
        assert_eq!(LoginService::Speaker.sid(),"micoapi");assert_eq!(LoginService::MiHome.sid(),"xiaomiio");
        assert!(login_url("https://api2.mina.xiaoaisound.com/sts",LoginService::Speaker).is_ok());
        assert!(login_url("https://api2.mina.xiaoaisound.com/sts",LoginService::MiHome).is_err());
    }
    #[test] fn the_additional_login_callback_cannot_redirect_credentials_to_a_lookalike() {
        for url in ["https://api2.mina.xiaoaisound.com.evil.com/sts","https://user@api2.mina.xiaoaisound.com/sts","http://api2.mina.xiaoaisound.com/sts","https://api2.mina.xiaoaisound.com:444/sts"] {
            assert!(login_url(url,LoginService::Speaker).is_err());
        }
    }
}
#[cfg(test)]mod tests{use super::*;#[test]fn reject_non_official_urls(){for u in ["http://api.io.mi.com/x","https://evilmi.com/x","https://mi.com.evil.com/x","file:///x","https://a:b@api.io.mi.com/x"]{assert!(official_url(u).is_err());}assert!(official_url("https://cdn.cnbj1.fds.api.mi-img.com/x.png").is_ok());}#[test]fn reject_unknown_regions(){assert!(region_url("../cn").is_err());assert_eq!(region_url("sg").unwrap(),"https://sg.api.io.mi.com/app");}}
