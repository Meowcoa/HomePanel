//! Independent implementation of documented APK protocol facts. No phone code/assets.
use crate::{cloud::Cloud, model::Device, now_ms};
use rand::{distributions::Alphanumeric, Rng};
use serde_json::{json, Value};
use std::{collections::HashMap, future::Future, sync::{Arc,atomic::Ordering}, task::Poll, time::Duration};
use tokio::sync::Mutex;

const BASE: &str = "https://api2.mina.xiaoaisound.com";
pub fn eligible(device:&Device)->bool {device.kind=="speaker" && device.model.starts_with("xiaomi.wifispeaker.")}

async fn join_three<A,B,C>(a:A,b:B,c:C)->(A::Output,B::Output,C::Output)
where A:Future, B:Future, C:Future {
    let mut a=Box::pin(a);let mut b=Box::pin(b);let mut c=Box::pin(c);
    let mut ra=None;let mut rb=None;let mut rc=None;
    std::future::poll_fn(|cx|{
        if ra.is_none(){if let Poll::Ready(value)=a.as_mut().poll(cx){ra=Some(value);}}
        if rb.is_none(){if let Poll::Ready(value)=b.as_mut().poll(cx){rb=Some(value);}}
        if rc.is_none(){if let Poll::Ready(value)=c.as_mut().poll(cx){rc=Some(value);}}
        if ra.is_some()&&rb.is_some()&&rc.is_some(){Poll::Ready((ra.take().unwrap(),rb.take().unwrap(),rc.take().unwrap()))}else{Poll::Pending}
    }).await
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub at:u64, pub error:Option<String>, pub status:Option<i64>, pub volume:Option<u8>,
    pub title:String, pub artist:String, pub duration_ms:Option<u64>, pub position_ms:Option<u64>,
    pub loop_type:Option<u8>, pub sleeping:Option<bool>, pub bluetooth:Option<bool>,
    pub sleep_error:Option<String>, pub bluetooth_error:Option<String>,
    pub adapter:Option<PhoneAdapter>,
    pub player_at:u64, pub sleep_at:u64, pub bluetooth_at:u64,
    pub track_audio:Option<u64>, pub track_global:Option<u64>,
    pub online:Option<bool>,
}
impl Snapshot {
    fn fresh(at:u64)->bool {at>0 && now_ms().saturating_sub(at)<15_000}
    pub fn current(&self)->bool {self.error.is_none() && Self::fresh(self.player_at)}
    pub fn sleep_current(&self)->bool {self.sleep_error.is_none() && Self::fresh(self.sleep_at)}
    pub fn bluetooth_current(&self)->bool {self.bluetooth_error.is_none() && Self::fresh(self.bluetooth_at)}
    pub fn available(&self,device:&Device)->bool {self.online.unwrap_or(device.online)}
    pub fn play_label(&self)->&'static str {
        if self.status==Some(0) && self.track_audio.is_none() && self.track_global.is_none() && self.title.is_empty(){"播放音乐"}else{"继续播放"}
    }
    pub fn projected_position(&self)->Option<u64> {
        self.position_ms.map(|position|if self.current() && self.status==Some(1) {
            position.saturating_add(now_ms().saturating_sub(self.player_at)).min(self.duration_ms.unwrap_or(u64::MAX))
        }else{position})
    }
    pub fn retain_last(mut self,previous:&Self)->Self {
        let had_player=self.player_at!=0;
        if !had_player {
            self.status=previous.status;self.volume=previous.volume;self.title=previous.title.clone();self.artist=previous.artist.clone();
            self.duration_ms=previous.duration_ms;self.position_ms=previous.position_ms;self.loop_type=previous.loop_type;self.track_audio=previous.track_audio;self.track_global=previous.track_global;
        }
        if self.sleep_at==0 {self.sleeping=previous.sleeping;}
        if self.bluetooth_at==0 {self.bluetooth=previous.bluetooth;}
        if had_player && self.status==Some(1) && previous.status==Some(1) {
            let same_track=if let (Some(current),Some(old))=(self.track_audio,previous.track_audio) {
                current==old
            } else if let (Some(current),Some(old))=(self.track_global,previous.track_global) {
                current==old
            } else {
                !self.title.is_empty() && self.title==previous.title
            };
            if same_track {
                let old_position=previous.projected_position().or(previous.position_ms);
                if let (Some(old_position),Some(new_position))=(old_position,self.position_ms) {
                    // A stale server read must not pull a smoothly advancing
                    // slider backwards. Pauses, seeks and track changes reset
                    // the anchor through their changed status or identity.
                    if new_position.saturating_add(2_000)<old_position {
                        self.position_ms=Some(old_position.min(self.duration_ms.unwrap_or(u64::MAX)));
                        self.player_at=now_ms();
                    }
                }
            }
        }
        self
    }
    pub fn read_failure(previous:&Self,error:String)->Self {
        Self{at:now_ms(),error:Some(error.clone()),sleep_error:Some(error.clone()),bluetooth_error:Some(error),..Default::default()}.retain_last(previous)
    }
    pub fn pending_refresh(&self)->Self {
        let mut snapshot=self.clone();snapshot.at=now_ms();snapshot.player_at=0;snapshot.sleep_at=0;snapshot.bluetooth_at=0;snapshot
    }
    pub fn state_text(&self)->String {
        if let Some(error)=&self.error {return error.clone();}
        let label=match self.status {Some(0)=>"已停止".into(),Some(1)=>"播放中".into(),Some(2)=>"已暂停".into(),Some(code)=>format!("设备状态（{code}）"),None=>"未提供播放状态".into()};
        if self.current(){label}else{format!("上次读取：{label}")}
    }
}

#[cfg(test)]
#[path="speaker_regression_tests.rs"]
mod regression_tests;

/// A separately validated UBUS contract, never an extension of MIoT permissions.
/// Created only from the exact account-bound target and a loaded speaker SPEC.
#[derive(Clone)]
pub struct PhoneAdapter {
    did:String, model:String, screen:Option<bool>, night:bool, grouped:bool, sleep_shortcut:bool,
}
fn hardware_screen(raw:&str)->Option<bool> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "L05B"|"L05C"|"L06A"|"LX05"|"LX05A"|"LX06"|"L15A"|"L16A"|"L16S"|"L17A"|
        "L09A"|"L09B"|"L07A"|"L07A_BLUE"|"L07A_GREEN"|"LX01"=>Some(false),
        "LX04"|"X10A"|"X06A"|"X08A"|"X08B"|"X08C"|"X08E"|"X8S"|"X6A"=>Some(true),
        _=>None,
    }
}
impl PhoneAdapter {
    fn resolve(device:&Device,target:&Target)->Option<Self> {
        let hardware=target.hardware.trim().to_ascii_uppercase();
        let screen=hardware_screen(&hardware);
        // Identity is the already verified unique miotDID mapping. Hardware
        // codes and MIoT model suffixes are different namespaces, not aliases.
        if !eligible(device) || device.spec_status!="ready" ||
            !device.spec_urn.starts_with("urn:miot-spec-v2:device:speaker:") {return None;}
        // Player commands use the common phone contract. Model-specific
        // shortcuts still require their own positively known capabilities.
        Some(Self{did:device.id.clone(),model:device.model.clone(),screen,night:target.night,grouped:target.grouped,sleep_shortcut:!matches!(hardware.as_str(),"LX04"|"T646")})
    }
    fn validate(&self,device:&Device,command:&Command)->Result<(),String> {
        if self.did!=device.id || self.model!=device.model {return Err("音箱适配信息已变化，请刷新".into());}
        match command {
            Command::Play("play"|"pause"|"prev"|"next"|"stop")|Command::Volume(0..=100)|Command::Loop(0..=3)|Command::Seek(_)=>Ok(()),
            Command::Sleep(_) if self.night && !self.grouped && self.sleep_shortcut=>Ok(()),
            Command::Sleep(_)=>Err(if self.grouped {"组合音箱的睡眠切换尚未确认，请在手机上操作"}else{"音箱未声明手机睡眠控制能力"}.into()),
            Command::Bluetooth(_) if self.screen==Some(false) && !self.grouped=>Ok(()),
            Command::Bluetooth(_)=>Err(if self.grouped {"组合音箱的蓝牙切换尚未确认，请在手机上操作"}else if self.screen==Some(true) {"此带屏型号未适配手机蓝牙快捷开关"}else{"此型号的蓝牙快捷开关能力尚未确认"}.into()),
            _=>Err("此操作仍须符合公开规格权限".into()),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Command {Play(&'static str), Volume(u8), Seek(u64), Loop(u8), Sleep(bool), Bluetooth(bool)}
fn loop_mode_label(name:&str)->Option<u8> {
    let lower=name.to_lowercase();
    let words:Vec<_>=lower.split(|c:char|!c.is_ascii_alphanumeric()).filter(|word|!word.is_empty()).collect();
    let aliases:[&[&str];4]=[&["single","单曲"],&["list","列表"],&["sequence","sequential","order","顺序"],&["random","shuffle","随机"]];
    let matched:Vec<_>=aliases.iter().enumerate().filter(|(_,keys)|keys.iter().any(|key|if key.is_ascii(){words.contains(key)}else{lower.contains(key)})).map(|(index,_)|index as u8).collect();
    (matched.len()==1).then(||matched[0])
}
impl Command {
    fn confirmation_delays(&self)->&'static [u64] {
        // The phone reconciles loop and volume state only after three seconds.
        // Wait past that stale-cache window without ever resending the write.
        if matches!(self,Self::Loop(_)|Self::Volume(_)){&[3250,600,1000,3000]}else{&[250,600,1000]}
    }
    pub fn validate(&self,device:&Device,state:&Snapshot)->Result<(),String> {
        self.wire()?;
        if device.kind!="speaker" || device.spec_status!="ready" {return Err("请先获取音箱公开功能规格".into());}
        if !state.available(device) {return Err("小爱音箱离线，请先检查设备连接".into());}
        if state.adapter.as_ref().is_some_and(|adapter|adapter.grouped) && matches!(self,Self::Play(_)|Self::Volume(_)|Self::Loop(_)|Self::Seek(_)) {
            return Err("组合音箱需要按主音箱路由控制，当前绑定尚未确认，请在手机上操作".into());
        }
        let current=match self {Self::Sleep(_)=>state.sleep_current(),Self::Bluetooth(_)=>state.bluetooth_current(),_=>state.current()};
        if !current {return Err(match self {
            Self::Sleep(_)=>state.sleep_error.clone().unwrap_or("睡眠状态已过期，请刷新".into()),
            Self::Bluetooth(_)=>state.bluetooth_error.clone().unwrap_or("蓝牙状态已过期，请刷新".into()),
            _=>state.error.clone().unwrap_or("播放状态已过期，请刷新".into()),
        });}
        match self {
            Self::Loop(_) if state.loop_type.is_none()=>return Err("尚未读到有效循环状态，请刷新".into()),
            Self::Sleep(_) if state.sleeping.is_none()=>return Err(state.sleep_error.clone().unwrap_or("尚未读到睡眠状态".into())),
            Self::Bluetooth(_) if state.bluetooth.is_none()=>return Err(state.bluetooth_error.clone().unwrap_or("尚未读到蓝牙快捷开关状态".into())),
            Self::Seek(ms) if state.position_ms.is_none() || state.duration_ms.is_none_or(|end|*ms>end)=>return Err("当前音源未提供可调节的进度范围".into()),
            Self::Volume(_) if state.volume.is_none()=>return Err("尚未读到音量状态".into()),
            _=>{},
        }
        // These are phone shortcuts with different wire semantics from MIoT.
        // A public property cannot bypass their per-device phone contract.
        if matches!(self,Self::Sleep(_)|Self::Bluetooth(_)) {
            return state.adapter.as_ref().ok_or("此音箱的手机快捷控制方式尚未确认")?.validate(device,self);
        }
        let spec=self.validate_spec(device);
        if spec.is_ok(){return spec;}
        // The phone protocol has its own enum/units and capability declarations.
        // Never modify the public Property/action list to manufacture permission.
        match &state.adapter {
            Some(adapter)=>adapter.validate(device,self),
            None=>spec,
        }
    }
    fn validate_spec(&self,device:&Device)->Result<(),String> {
        if device.kind!="speaker" || device.spec_status!="ready" {return Err("请先获取音箱公开功能规格".into());}
        if let Self::Play(action)=self {
            let key=if *action=="prev" {"previous"}else{action};
            return if device.actions.iter().any(|a|a.key==key) {Ok(())}else{Err("此操作未在当前音箱的公开规格中声明".into())};
        }
        let key=match self {Self::Volume(_)=>"volume",Self::Seek(_)=>"position",Self::Loop(_)=>"play-loop-mode",Self::Sleep(_)=>"sleep-mode",Self::Bluetooth(_)=>"bluetooth",_=>return Err("未适配的音箱操作".into())};
        let property=device.properties.iter().find(|p|p.readable&&p.writable &&
            (p.key==key || (key=="position"&&p.key=="playback-position") ||
            (key=="bluetooth"&&p.service_key=="bluetooth"&&p.key=="on")))
            .ok_or("此项目未声明可确认的控制权限，当前只展示状态")?;
        let value=match self {
            Self::Volume(v)=>json!(v),Self::Sleep(v)|Self::Bluetooth(v)=>json!(v),
            Self::Seek(ms)=>match property.unit.as_str(){"ms"|"milliseconds"=>json!(ms),"s"|"seconds"=>json!(ms/1000),_=>return Err("公开规格未确认进度单位，当前只展示进度".into())},
            Self::Loop(mode)=>{
                if *mode>3{return Err("未知循环模式".into());}
                let options:Vec<_>=property.options.iter().filter(|o|loop_mode_label(&o.name)==Some(*mode)).collect();
                if options.is_empty(){return Err("公开规格未提供此循环选项，暂不能修改".into());}
                if options.len()!=1{return Err("此循环选项的对应关系不明确，暂不能修改".into());}options[0].value.clone()
            },_=>return Err("未适配的音箱操作".into()),
        };
        crate::model::validate_property(property,&value)
    }
    fn wire(&self)->Result<(&'static str,&'static str,Value),String> {
        Ok(match self {
            Self::Play(action) if ["play","pause","prev","next","stop"].contains(action)=>
                ("mediaplayer","player_play_operation",json!({"action":action,"media":"app_android"})),
            Self::Volume(value) if *value<=100=>("mediaplayer","player_set_volume",json!({"volume":value,"media":"app_android"})),
            Self::Seek(ms)=>("mediaplayer","player_set_positon",json!({"position":ms,"media":"app_android"})),
            Self::Loop(mode) if *mode<=3=>("mediaplayer","player_set_loop",json!({"type":mode,"media":"app_android"})),
            Self::Sleep(on)=>("nightmode","sleep_mode",json!({"value":if *on {"night"}else{"normal"}})),
            Self::Bluetooth(on)=>("mibt","enable",json!({"btmode":"classic","connect":if *on {1}else{0},"discover":0})),
            _=>return Err("此音箱操作不在已适配范围内".into()),
        })
    }
    fn wire_for(&self,before:&Snapshot)->Result<(&'static str,&'static str,Value),String> {
        // An empty idle queue cannot be resumed. The phone's fixed fallback
        // starts music without speaking; only an explicit Play click uses it.
        if matches!(self,Self::Play("play")) && before.adapter.is_some() && before.play_label()=="播放音乐" {
            return Ok(("mibrain","ai_service",json!({"nlp":1,"tts":0,"nlp_execute":1,"nlp_text":"播放音乐","tts_play":0,"caller":"app_android"})));
        }
        self.wire()
    }
    fn confirmed(&self,before:&Snapshot,after:&Snapshot)->bool {
        match self {
            Self::Volume(v)=>after.volume==Some(*v), Self::Loop(v)=>after.loop_type==Some(*v),
            Self::Sleep(v)=>after.sleeping==Some(*v),Self::Bluetooth(v)=>after.bluetooth==Some(*v),
            Self::Seek(ms)=>after.position_ms.is_some_and(|at|at.abs_diff(*ms)<=2000),
            Self::Play("play")=>after.status==Some(1),Self::Play("pause")=>after.status==Some(2),
            Self::Play("stop")=>after.status==Some(0),
            Self::Play("next"|"prev")=>match (before.track_audio,after.track_audio,before.track_global,after.track_global) {
                (Some(old),Some(new),_,_)=>old!=new,
                (_,_,Some(old),Some(new))=>old!=new,
                (None,None,None,None)=>!after.title.is_empty() && after.title!=before.title,
                _=>false,
            },
            _=>false,
        }
    }
}

#[derive(Clone)]
struct Target {id:String, sn:String, hardware:String, profile:String, is_v3:bool, night:bool, grouped:bool, online:Option<bool>}

pub struct Service {
    pub auth:Arc<Cloud>, client:reqwest::Client,
    gates:Mutex<HashMap<String,Arc<Mutex<()>>>>,
    targets:Mutex<HashMap<String,(String,u64,u64,std::time::Instant,Target)>>,
    #[cfg(test)]
    test_base:String,
}
impl Service {
    pub fn new(root:&std::path::Path)->Result<Self,String> {
        let dir=root.join("speaker-service");
        Ok(Self{auth:Arc::new(Cloud::new_speaker(dir.clone(),dir.join("cache"))?),
            client:reqwest::Client::builder().timeout(Duration::from_secs(6)).redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("HomePanel/",env!("CARGO_PKG_VERSION"))).build().map_err(|_|"无法初始化音箱服务")?,gates:Mutex::new(HashMap::new()),targets:Mutex::new(HashMap::new()),
            #[cfg(test)] test_base:BASE.into()})
    }
    fn endpoint(&self,path:&str)->String {
        #[cfg(test)] let base=&self.test_base;
        #[cfg(not(test))] let base=BASE;
        format!("{base}{path}")
    }
    async fn identity(&self,main:&Cloud)->Result<crate::storage::Session,String> {
        let session=self.auth.session.read().await.clone().ok_or("请先连接小爱音箱服务")?;
        let same=main.session.read().await.as_ref().is_some_and(|s|s.user_id==session.user_id);
        if !same {return Err("音箱服务与米家账号不一致，请用同一账号重新连接".into());}
        Ok(session)
    }
    async fn request(&self, main:&Cloud,path:&str,target:Option<&Target>,form:Option<Vec<(&str,String)>>)->Result<Value,String> {
        self.request_inner(main,path,target,form,true).await
    }
    async fn request_inner(&self, main:&Cloud,path:&str,target:Option<&Target>,form:Option<Vec<(&str,String)>>,expire:bool)->Result<Value,String> {
        // No arbitrary hosts, caller-supplied cookies, voice uploads or private API paths.
        if !["/admin/v3/device_list","/remote/ubus"].contains(&path){return Err("未适配的音箱接口".into());}
        let epoch=self.auth.epoch.load(Ordering::SeqCst);
        let session=self.identity(main).await?;
        let mut cookie=format!("userId={}; serviceToken={}",session.user_id,session.service_token);
        if let Some(t)=target {
            for value in [&t.id,&t.sn,&t.hardware,&t.profile] {if value.contains([';','\r','\n']) {return Err("音箱身份格式无效".into());}}
            cookie.push_str(&format!("; deviceId={}",t.id));
            for (name,value) in [("sn",&t.sn),("hardware",&t.hardware),("deviceSNProfile",&t.profile)] {cookie.push_str(&format!("; {name}={value}"));}
            cookie.push_str(if t.is_v3 {"; isV3Protocol=1"}else{"; isV3Protocol=0"});
        }
        let request_id:String=rand::thread_rng().sample_iter(&Alphanumeric).take(20).map(char::from).collect();
        let request=if let Some(form)=form {self.client.post(self.endpoint(path)).form(&form)}else{self.client.get(self.endpoint(path))};
        let response=request.query(&[("timestamp",now_ms().to_string()),("requestId",request_id)])
            .header("Cookie",cookie).send().await.map_err(crate::cloud::network_error)?;
        if matches!(response.status().as_u16(),401|403) {if expire{self.auth.expire_session(epoch).await?;}return Err("音箱登录已失效，请重新连接".into());}
        if !response.status().is_success(){return Err(format!("音箱服务请求失败（HTTP {}）",response.status().as_u16()));}
        if response.content_length().unwrap_or(0)>1024*1024{return Err("音箱服务响应过大".into());}
        let bytes=response.bytes().await.map_err(crate::cloud::network_error)?;
        if bytes.len()>1024*1024{return Err("音箱服务响应过大".into());}
        self.identity(main).await?;
        if self.auth.epoch.load(Ordering::SeqCst)!=epoch{return Err("音箱账号已改变，请重新读取".into());}
        let value:Value=serde_json::from_slice(&bytes).map_err(|_|"音箱服务返回了无法识别的数据")?;
        parse_response(&value)
    }
    async fn target(&self,main:&Cloud,device:&Device)->Result<Target,String> {
        let main_epoch=main.epoch.load(Ordering::SeqCst);let epoch=self.auth.epoch.load(Ordering::SeqCst);
        if let Some((model,old_main,old_epoch,at,target))=self.targets.lock().await.get(&device.id) {
            if model==&device.model && *old_main==main_epoch && *old_epoch==epoch && target.online!=Some(false) && at.elapsed()<Duration::from_secs(60){return Ok(target.clone());}
        }
        let data=self.request(main,"/admin/v3/device_list",None,None).await?;
        let target=select_target(&data,&device.id)?;
        self.targets.lock().await.insert(device.id.clone(),(device.model.clone(),main_epoch,epoch,std::time::Instant::now(),target.clone()));
        Ok(target)
    }
    async fn ubus(&self,main:&Cloud,target:&Target,path:&str,method:&str,message:Value)->Result<Value,String> {
        let data=self.request(main,"/remote/ubus",Some(target),Some(vec![
            ("deviceId",target.id.clone()),("path",path.into()),("method",method.into()),("message",message.to_string())])).await?;
        parse_ubus(&data)
    }
    async fn player_value(&self,main:&Cloud,target:&Target)->Result<Value,String> {
        let mut errors=Vec::new();
        // MiNA deployments differ on the media namespace used for reads.
        // Keep this fallback read-only; writes still use one exact contract.
        for media in ["app_ios","app_android"] {
            match self.ubus(main,target,"mediaplayer","player_get_play_status",json!({"media":media})).await {
                Ok(value) if parse_player(&value).is_ok() => return Ok(value),
                Ok(_) => errors.push(format!("{media}: 音箱状态格式无效")),
                Err(error) => errors.push(format!("{media}: {error}")),
            }
        }
        Err(errors.join("；"))
    }
    async fn state(&self,main:&Cloud,target:&Target,device:&Device)->Result<Snapshot,String> {
        let player=self.player_value(main,target);
        let sleep=self.read_sleep(main,target);
        let bluetooth=self.read_bluetooth(main,target);
        let (player,sleep,bluetooth)=join_three(player,sleep,bluetooth).await;
        let mut state=match player.and_then(|value|parse_player(&value)) {
            Ok(state)=>state,Err(error)=>Snapshot{error:Some(format!("播放读取：{error}")),..Default::default()},
        };
        // Optional firmware features run in parallel and never serialize the
        // player/song path. Unsupported or stalled replies remain unknown.
        match sleep {Ok(value)=>{state.sleeping=Some(value);state.sleep_at=now_ms();},Err(error)=>state.sleep_error=Some(error)}
        match bluetooth {Ok(value)=>{state.bluetooth=Some(value);state.bluetooth_at=now_ms();},Err(error)=>state.bluetooth_error=Some(error)}
        state.adapter=PhoneAdapter::resolve(device,target);
        state.online=target.online;
        state.at=now_ms();Ok(state)
    }
    async fn read_sleep(&self,main:&Cloud,target:&Target)->Result<bool,String> {
        let value=tokio::time::timeout(Duration::from_secs(2),self.ubus(main,target,"nightmode","status",json!({}))).await
            .map_err(|_|"睡眠状态读取超时，可稍后刷新")??;
        parse_sleep(&value)
    }
    async fn read_bluetooth(&self,main:&Cloud,target:&Target)->Result<bool,String> {
        let value=tokio::time::timeout(Duration::from_secs(2),self.ubus(main,target,"mibt","status",json!({"btmode":"classic"}))).await
            .map_err(|_|"蓝牙状态读取超时，可稍后刷新")??;
        parse_bluetooth(&value)
    }
    async fn confirm(&self,main:&Cloud,target:&Target,before:&Snapshot,command:&Command)->Result<Snapshot,String> {
        // Poll only the field being changed. Unrelated optional endpoints must
        // not delay confirmation or turn an accepted write into a false failure.
        let mut after=before.clone();
        match command {
            Command::Sleep(_)=>{after.sleeping=Some(self.read_sleep(main,target).await?);after.sleep_error=None;after.sleep_at=now_ms();},
            Command::Bluetooth(_)=>{after.bluetooth=Some(self.read_bluetooth(main,target).await?);after.bluetooth_error=None;after.bluetooth_at=now_ms();},
            _=>{
                let player=self.player_value(main,target).await?;
                let state=parse_player(&player)?;
                after.status=state.status;after.volume=state.volume;after.title=state.title;after.artist=state.artist;
                after.duration_ms=state.duration_ms;after.position_ms=state.position_ms;after.loop_type=state.loop_type;
                after.player_at=state.player_at;after.error=None;after.track_audio=state.track_audio;after.track_global=state.track_global;
            },
        }
        // Each field group keeps its own read time; untouched data stays stale.
        after.at=now_ms();
        Ok(after)
    }
    async fn gate(&self,id:&str)->Arc<Mutex<()>> {self.gates.lock().await.entry(id.into()).or_insert_with(||Arc::new(Mutex::new(()))).clone()}
    pub async fn read(&self,main:&Cloud,device:&Device)->Result<Snapshot,String> {
        let gate=self.gate(&device.id).await;let _lock=gate.lock().await;
        let target=self.target(main,device).await?;self.state(main,&target,device).await
    }
    #[cfg(debug_assertions)]
    pub async fn diagnostic(&self,main:&Cloud)->Vec<crate::speaker_diagnostics::DiagnosticStep> {
        use crate::speaker_diagnostics::{DiagnosticStep as Step,Stage,Code,Flag};
        fn code(error:&str)->Code {
            if error.contains("403"){Code::Forbidden}
            else if error.contains("失效")||error.contains("账号不一致"){Code::Unauthorized}
            else if error.contains("超时"){Code::Timeout}
            else if error.contains("无法连接")||error.contains("网络"){Code::Network}
            else if error.contains("拒绝")||error.contains("未接受"){Code::Rejected}
            else if error.contains("过大"){Code::ResponseTooLarge}
            else {Code::InvalidResponse}
        }
        let main_present=main.session.read().await.is_some();let speaker_present=self.auth.session.read().await.is_some();
        let mut steps=vec![Step::new(Stage::MainSession,if main_present{Code::Ok}else{Code::Unavailable}).flag(Flag::SessionPresent,main_present),
            Step::new(Stage::SpeakerSession,if speaker_present{Code::Ok}else{Code::Unavailable}).flag(Flag::SessionPresent,speaker_present)];
        if !main_present || !speaker_present {return steps;}
        if self.identity(main).await.is_err(){steps.push(Step::new(Stage::Identity,Code::Unauthorized).flag(Flag::IdentityMatches,false));return steps;}
        steps.push(Step::new(Stage::Identity,Code::Ok).flag(Flag::IdentityMatches,true));
        // Read-only request mode never expires/clears a saved session. Raw
        // responses and identities stay in this stack, never in the report.
        let data=match self.request_inner(main,"/admin/v3/device_list",None,None,false).await {
            Ok(data)=>data,Err(error)=>{steps.push(Step::new(Stage::DeviceList,code(&error)));return steps;},
        };
        let list=data.get("deviceList").or_else(||data.get("devices")).unwrap_or(&data).as_array();
        steps.push(Step::new(Stage::DeviceList,if list.is_some(){Code::Ok}else{Code::InvalidResponse}).flag(Flag::DeviceListPresent,data.get("deviceList").is_some()||data.get("devices").is_some()||data.is_array()).flag(Flag::DeviceListParsed,list.is_some()).flag(Flag::EnvelopeParsed,true));
        let target=list.and_then(|list|list.iter().find_map(|row|{
            let did=crate::model::string_id(&row["miotDID"]);select_target(&data,&did).ok().map(|target|(target,row["capabilities"].is_object()))
        }));
        let Some((target,capabilities_present))=target else{steps.push(Step::new(Stage::Target,Code::Unavailable));return steps;};
        let valid_fields=[&target.id,&target.sn,&target.hardware,&target.profile].iter().all(|value|!value.contains([';','\r','\n']));
        steps.push(Step::new(Stage::Target,Code::Ok).flag(Flag::TargetMatched,true).flag(Flag::TargetFieldsValid,valid_fields).flag(Flag::HardwareRecognized,hardware_screen(&target.hardware).is_some()).flag(Flag::CapabilitiesPresent,capabilities_present).flag(Flag::IsV3Present,true));
        for (stage,path,method,message) in [(Stage::Player,"mediaplayer","player_get_play_status",json!({"media":"app_ios"})),(Stage::Sleep,"nightmode","status",json!({})),(Stage::Bluetooth,"mibt","status",json!({"btmode":"classic"}))] {
            let result=if stage==Stage::Player {
                self.player_value(main,&target).await
            } else {
                self.request_inner(main,"/remote/ubus",Some(&target),Some(vec![("deviceId",target.id.clone()),("path",path.into()),("method",method.into()),("message",message.to_string())]),false).await.and_then(|value|parse_ubus(&value))
            };
            let step=match result {
                Err(error)=>Step::new(stage,code(&error)),
                Ok(value)=>match stage {
                    Stage::Player=>Step::new(stage,if parse_player(&value).is_ok(){Code::Ok}else{Code::InvalidResponse}).flag(Flag::InfoPresent,!value.is_null()).flag(Flag::InfoParsed,value.is_object())
                        .flag(Flag::StatusPresent,value.get("status").is_some()).flag(Flag::StatusParsed,value["status"].as_i64().is_some())
                        .flag(Flag::VolumePresent,value.get("volume").is_some()).flag(Flag::VolumeParsed,value["volume"].as_u64().is_some_and(|v|v<=100))
                        .flag(Flag::PositionPresent,value["play_song_detail"].get("position").is_some())
                        .flag(Flag::PositionParsed,value["play_song_detail"]["position"].as_u64().is_some())
                        .flag(Flag::DurationPresent,value["play_song_detail"].get("duration").is_some()).flag(Flag::DurationParsed,value["play_song_detail"]["duration"].as_u64().is_some())
                        .flag(Flag::LoopPresent,value.get("loop_type").is_some()).flag(Flag::LoopParsed,value["loop_type"].as_u64().is_some_and(|v|v<=3)),
                    Stage::Sleep=>Step::new(stage,if parse_sleep(&value).is_ok(){Code::Ok}else{Code::InvalidResponse}).flag(Flag::SleepPresent,value.get("sleep_mode").is_some()).flag(Flag::SleepParsed,parse_sleep(&value).is_ok()),
                    Stage::Bluetooth=>Step::new(stage,if parse_bluetooth(&value).is_ok(){Code::Ok}else{Code::InvalidResponse}).flag(Flag::BluetoothPresent,value.get("connect").is_some()).flag(Flag::BluetoothParsed,parse_bluetooth(&value).is_ok()),
                    _=>unreachable!(),
                },
            };
            steps.push(step);
        }
        steps
    }
    pub async fn control(&self,main:&Cloud,device:&Device,command:Command)->Result<Snapshot,String> {
        let gate=self.gate(&device.id).await;let _lock=gate.lock().await;
        let current=main.load_spec(&device.id).await?;
        if current.model!=device.model {return Err("音箱型号已变化，请重新打开设备".into());}
        self.control_loaded(main,&current,command).await
    }
    async fn control_loaded(&self,main:&Cloud,current:&Device,command:Command)->Result<Snapshot,String> {
        let target=self.target(main,&current).await?;
        let before=self.state(main,&target,&current).await?;
        command.validate(&current,&before)?;
        self.execute(main,&target,&before,&command).await
    }
    async fn execute(&self,main:&Cloud,target:&Target,before:&Snapshot,command:&Command)->Result<Snapshot,String> {
        let (path,method,message)=command.wire_for(before)?;
        self.ubus(main,&target,path,method,message).await?; // Exactly one write; never retry it.
        for &delay in command.confirmation_delays() {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            if let Ok(after)=self.confirm(main,&target,&before,&command).await {if command.confirmed(&before,&after){return Ok(after);}}
        }
        Err("UNCERTAIN: 音箱指令已发送，但未读到对应变化；请刷新确认，不会重复发送".into())
    }
    #[cfg(debug_assertions)]
    pub async fn probe_loop(&self,main:&Cloud,device:&Device)->Vec<crate::speaker_diagnostics::DiagnosticStep> {
        use crate::speaker_diagnostics::{DiagnosticStep as Step,Stage,Code};
        let gate=self.gate(&device.id).await;let _lock=gate.lock().await;
        match main.load_spec(&device.id).await {
            Ok(current) if current.model==device.model=>self.probe_loop_loaded(main,&current).await,
            _=>vec![Step::new(Stage::Target,Code::InvalidResponse)],
        }
    }
    #[cfg(debug_assertions)]
    async fn probe_loop_loaded(&self,main:&Cloud,device:&Device)->Vec<crate::speaker_diagnostics::DiagnosticStep> {
        use crate::speaker_diagnostics::{DiagnosticStep as Step,Stage,Code,Flag};
        let main_epoch=main.epoch.load(Ordering::SeqCst);let epoch=self.auth.epoch.load(Ordering::SeqCst);
        let target=match self.target(main,device).await {Ok(target)=>target,Err(_)=>return vec![Step::new(Stage::Target,Code::Unavailable)]};
        let before=match self.state(main,&target,device).await {Ok(state)=>state,Err(_)=>return vec![Step::new(Stage::Player,Code::Unavailable)]};
        let Some(original)=before.loop_type else{return vec![Step::new(Stage::Player,Code::InvalidResponse)];};
        let change=Command::Loop(if original==0{1}else{0});let restore=Command::Loop(original);
        let change_allowed=change.validate(device,&before).is_ok();let restore_allowed=restore.validate(device,&before).is_ok();
        let preflight=Step::new(Stage::Preflight,if change_allowed && restore_allowed{Code::Ok}else{Code::Rejected})
                .flag(Flag::EligibleSpeaker,eligible(device)).flag(Flag::SpecReady,device.spec_status=="ready")
                .flag(Flag::SpeakerSpec,device.spec_urn.starts_with("urn:miot-spec-v2:device:speaker:"))
                .flag(Flag::AdapterResolved,before.adapter.is_some()).flag(Flag::PlayerCurrent,before.current())
                .flag(Flag::LoopChangeAllowed,change_allowed).flag(Flag::LoopRestoreAllowed,restore_allowed);
        if !change_allowed || !restore_allowed {
            return vec![preflight];
        }
        // Explicitly authorized, non-audible probe. Validate both commands
        // before changing anything. Always attempt the original value after
        // the first write, even if that write/readback returned an error.
        let outcome=|result:&Result<Snapshot,String>|match result {
            Ok(_)=>Code::Ok,Err(error) if error.starts_with("UNCERTAIN:")=>Code::Unconfirmed,Err(_)=>Code::Rejected,
        };
        let changed=self.execute_loop_probe(main,&target,&before,&change).await;
        let mut steps=vec![Step::new(Stage::LoopChange,outcome(&changed)),preflight];
        if main.epoch.load(Ordering::SeqCst)!=main_epoch || self.auth.epoch.load(Ordering::SeqCst)!=epoch {
            steps.push(Step::new(Stage::LoopRestore,Code::SessionChanged));return steps;
        }
        let latest=changed.as_ref().unwrap_or(&before);
        let restored=self.execute_loop_probe(main,&target,latest,&restore).await;
        steps.push(Step::new(Stage::LoopRestore,outcome(&restored)));
        steps
    }
    #[cfg(debug_assertions)]
    async fn execute_loop_probe(&self,main:&Cloud,target:&Target,before:&Snapshot,command:&Command)->Result<Snapshot,String> {
        if !matches!(command,Command::Loop(_)){return Err("探测仅允许循环模式".into());}
        let send=|path:&str,method:&str,message:Value|vec![("deviceId",target.id.clone()),("path",path.to_owned()),("method",method.to_owned()),("message",message.to_string())];
        let (path,method,message)=command.wire()?;
        // The explicit diagnostic probe must not clear account files, even if
        // the server refuses a write or a read while restoration is pending.
        let data=self.request_inner(main,"/remote/ubus",Some(target),Some(send(path,method,message)),false).await?;
        parse_ubus(&data)?;
        for &delay in command.confirmation_delays() {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            let read=self.player_value(main,target).await.and_then(|value|parse_player(&value));
            if let Ok(after)=read {
                if command.confirmed(before,&after){
                    // A probe must also observe a stable result: an accepted
                    // delayed change must not be mistaken for a completed restore.
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    let stable=self.player_value(main,target).await.and_then(|value|parse_player(&value))?;
                    return if command.confirmed(before,&stable){Ok(stable)}else{Err("UNCERTAIN: 循环模式未稳定回读确认".into())};
                }
            }
        }
        Err("UNCERTAIN: 循环模式未回读确认".into())
    }
}

fn parse_response(value:&Value)->Result<Value,String> {
    // BaseResponse declares code/status; MinaResponse declares data/result.
    // Only these explicit aliases are accepted. Conflicting fields are errors.
    let primary=value.get("code");let alternate=value.get("status");
    if primary.is_some() && alternate.is_some() && primary!=alternate {return Err("音箱服务返回了冲突的状态码".into());}
    let code=primary.or(alternate).and_then(Value::as_i64).ok_or("音箱服务响应格式无效")?;
    if code!=0{return Err(format!("音箱服务拒绝请求（{code}）"));}
    let primary=value.get("data").filter(|v|!v.is_null());let alternate=value.get("result").filter(|v|!v.is_null());
    if primary.is_some() && alternate.is_some() && primary!=alternate {return Err("音箱服务返回了冲突的结果".into());}
    primary.or(alternate).cloned().ok_or("音箱服务未提供结果".into())
}
fn select_target(data:&Value,did:&str)->Result<Target,String> {
    // /admin/v3/device_list returns DeviceListResponseV3.deviceList, alongside
    // groupInfo. Groups must never be flattened or mistaken for device IDs.
    let list=if let Some(list)=data.as_array(){list}else{
        let primary=data.get("deviceList");let alternate=data.get("devices");
        if primary.is_some() && alternate.is_some() && primary!=alternate {return Err("音箱设备列表存在冲突，请刷新重试".into());}
        primary.or(alternate).and_then(Value::as_array).ok_or("音箱设备列表格式无效")?
    };
    if list.is_empty(){return Err("小爱账号下没有可用音箱，请检查设备绑定".into());}
    if did.is_empty(){return Err("米家设备身份缺失，请重新获取设备列表".into());}
    let matches:Vec<_>=list.iter().filter(|row|crate::model::string_id(&row["miotDID"])==did).collect();
    if matches.len()!=1{return Err("小爱服务未唯一匹配这台米家音箱，请检查账号和设备绑定".into());}
    let row=matches[0];let id=crate::model::string_id(&row["deviceID"]);
    if id.is_empty(){return Err("小爱服务未返回有效设备身份".into());}
    let capability=|key:&str|row["capabilities"][key].as_i64().is_some_and(|value|value>0);
    let grouped=in_group(data,&id) ||
        row.get("stereoJson").is_some_and(|value|!value.is_null() && value.as_str().is_none_or(|text|!matches!(text.trim(),""|"null"|"{}"|"[]")));
    let online=match row["presence"].as_str().map(|text|text.trim().to_ascii_lowercase()).as_deref(){Some("online")=>Some(true),Some("offline")=>Some(false),_=>None};
    Ok(Target{id,sn:row["serialNumber"].as_str().unwrap_or_default().into(),hardware:row["hardware"].as_str().unwrap_or_default().into(),profile:row["deviceSNProfile"].as_str().unwrap_or_default().into(),is_v3:capability("ai_protocol_3_0"),night:capability("night_mode")||capability("night_mode_v2"),grouped,online})
}
fn in_group(data:&Value,id:&str)->bool {
    let Some(groups)=data.get("groupInfo").filter(|v|!v.is_null())else{return false;};
    let Some(groups)=groups.as_array()else{return true;};
    groups.iter().any(|group|{
        let Some(master)=group["masterDeviceID"].as_str().filter(|s|!s.is_empty())else{return true;};
        if master==id {return true;}
        let Some(members)=group["wirelessDeviceList"].as_array()else{return true;};
        members.iter().any(|member|member["deviceId"].as_str().is_none_or(|member_id|member_id==id || member_id.is_empty()))
    })
}
fn parse_sleep(value:&Value)->Result<bool,String> {
    match value["sleep_mode"].as_str(){Some("night")=>Ok(true),Some("normal")=>Ok(false),_=>Err("音箱未提供有效睡眠状态".into())}
}
fn parse_bluetooth(value:&Value)->Result<bool,String> {
    // Phone QuickToggle follows BluetoothResponse.connect, not the radio's
    // enable field. Missing/invalid connect is unknown, never default false.
    match value["connect"].as_i64(){Some(0)=>Ok(false),Some(1)=>Ok(true),_=>Err("音箱未提供有效蓝牙快捷开关状态".into())}
}
fn parse_ubus(data:&Value)->Result<Value,String> {
    let object:Value=if let Some(text)=data.as_str(){serde_json::from_str(text).map_err(|_|"音箱命令结果格式无效")?}else{data.clone()};
    if object["code"].as_i64()!=Some(0){return Err(format!("音箱未接受此功能（{}）",object["code"].as_i64().unwrap_or(-1)));}
    match object.get("info") {
        Some(Value::String(text)) if !text.is_empty()=>serde_json::from_str(text).map_err(|_|"音箱状态格式无效".into()),
        Some(Value::Object(_))=>Ok(object["info"].clone()),
        None|Some(Value::Null)=>Ok(Value::Null),
        Some(Value::String(_))=>Ok(Value::Null),
        _=>Err("音箱状态格式无效".into()),
    }
}
fn number_i64(value:&Value)->Option<i64> {
    value.as_i64().or_else(||value.as_str().and_then(|text|text.trim().parse::<i64>().ok()))
}
fn number_u64(value:&Value)->Option<u64> {
    value.as_u64().or_else(||value.as_str().and_then(|text|text.trim().parse::<u64>().ok()))
}
fn parse_player(value:&Value)->Result<Snapshot,String> {
    let status=number_i64(&value["status"]).ok_or("音箱未提供播放状态")?;
    let detail=&value["play_song_detail"];
    let text=|key:&str|detail[key].as_str().unwrap_or_default().chars().take(180).collect();
    let positive_id=|key:&str|detail[key].as_i64().or_else(||detail[key].as_str().and_then(|text|text.parse::<i64>().ok())).filter(|value|*value>0).map(|value|value as u64);
    Ok(Snapshot{at:now_ms(),player_at:now_ms(),track_audio:positive_id("audio_id"),track_global:positive_id("global_id"),status:Some(status),volume:number_u64(&value["volume"]).filter(|v|*v<=100).map(|v|v as u8),
        title:text("title"),artist:text("artist"),loop_type:number_u64(&value["loop_type"]).filter(|v|*v<=3).map(|v|v as u8),
        duration_ms:number_u64(&detail["duration"]).filter(|v|*v>0),position_ms:number_u64(&detail["position"]),..Default::default()})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn phone_device()->Device {
        Device{id:"fixture-miot".into(),model:"xiaomi.wifispeaker.l05c".into(),kind:"speaker".into(),
            spec_status:"ready".into(),spec_urn:"urn:miot-spec-v2:device:speaker:0000A015:fixture:1".into(),online:true,
            properties:vec![crate::model::Property{key:"play-loop-mode".into(),format:"uint8".into(),readable:true,writable:true,
                options:vec![crate::model::PropertyOption{value:json!(1),name:"Single Cycle".into()}],..Default::default()}],..Default::default()}
    }
    fn phone_state(device:&Device,row:Value)->Snapshot {
        let target=select_target(&json!({"deviceList":[row],"groupInfo":[]}),&device.id).unwrap();
        Snapshot{at:now_ms(),player_at:now_ms(),sleep_at:now_ms(),bluetooth_at:now_ms(),volume:Some(47),status:Some(1),loop_type:Some(1),sleeping:Some(false),bluetooth:Some(false),
            duration_ms:Some(120000),position_ms:Some(30000),adapter:PhoneAdapter::resolve(device,&target),..Default::default()}
    }
    fn phone_row()->Value {json!({"miotDID":"fixture-miot","deviceID":"fixture-mina","hardware":"L05C","capabilities":{"night_mode":1,"ai_protocol_3_0":1}})}
    #[test] fn phone_adapter_adds_separate_loop_seek_sleep_and_bluetooth_without_changing_spec() {
        let device=phone_device();let state=phone_state(&device,phone_row());
        assert!(Command::Loop(3).validate_spec(&device).is_err());
        for mode in 0..=3 {assert!(Command::Loop(mode).validate(&device,&state).is_ok());}
        for command in [Command::Seek(60000),Command::Sleep(true),Command::Bluetooth(true),Command::Volume(30),Command::Play("play")] {assert!(command.validate(&device,&state).is_ok());}
        for command in [Command::Seek(120001),Command::Loop(4),Command::Volume(101),Command::Play("remove")] {
            assert!(command.validate(&device,&state).is_err());
        }
        assert_eq!(device.properties.len(),1);assert_eq!(device.properties[0].options.len(),1);assert!(device.actions.is_empty());
    }
    #[test] fn phone_adapter_requires_exact_model_spec_and_fresh_runtime_state() {
        for (model,hardware,urn,ready) in [
            ("vendor.wifispeaker.l05c","L05C","urn:miot-spec-v2:device:speaker:fixture",true),
            ("xiaomi.wifispeaker.l05c","L05C","urn:miot-spec-v2:device:outlet:fixture",true),
            ("xiaomi.wifispeaker.l05c","L05C","",true),
            ("xiaomi.wifispeaker.l05c","L05C","urn:miot-spec-v2:device:speaker:fixture",false),
        ] {
            let mut device=phone_device();device.model=model.into();device.spec_urn=urn.into();if !ready{device.spec_status="unloaded".into();}
            let mut row=phone_row();row["hardware"]=json!(hardware);
            let state=phone_state(&device,row);assert!(state.adapter.is_none());assert!(Command::Loop(3).validate(&device,&state).is_err());
        }
        let mut device=phone_device();let mut state=phone_state(&device,phone_row());
        device.id="other-device".into();assert!(Command::Loop(3).validate(&device,&state).is_err());device=phone_device();
        state.player_at=now_ms().saturating_sub(16000);assert!(Command::Loop(3).validate(&device,&state).is_err());
        state.player_at=now_ms();state.bluetooth=None;assert!(Command::Bluetooth(true).validate(&device,&state).is_err());
        state.loop_type=None;assert!(Command::Loop(3).validate(&device,&state).is_err());
        state.sleeping=None;assert!(Command::Sleep(true).validate(&device,&state).is_err());
        device.online=false;assert!(Command::Seek(30000).validate(&device,&state).is_err());
    }
    #[test] fn sleep_requires_positive_phone_capability_and_private_toggles_respect_group_and_screen_limits() {
        let mut device=phone_device();
        for value in [Value::Null,json!(0),json!(-1),json!(true),json!("1")] {
            let mut row=phone_row();row["capabilities"]["night_mode"]=value;
            assert!(Command::Sleep(true).validate(&device,&phone_state(&device,row)).is_err());
        }
        let mut row=phone_row();row["capabilities"]=json!({"night_mode_v2":1});
        assert!(Command::Sleep(true).validate(&device,&phone_state(&device,row.clone())).is_ok());
        row["stereoJson"]=json!("fixture-group");let state=phone_state(&device,row);
        assert!(Command::Sleep(true).validate(&device,&state).is_err());assert!(Command::Bluetooth(true).validate(&device,&state).is_err());
        device.model="xiaomi.wifispeaker.x10a".into();let mut row=phone_row();row["hardware"]=json!("X10A");
        let state=phone_state(&device,row);assert!(Command::Loop(3).validate(&device,&state).is_ok());assert!(Command::Bluetooth(true).validate(&device,&state).is_err());
        let target=select_target(&json!({"deviceList":[phone_row()],"groupInfo":[{}]}),"fixture-miot").unwrap();assert!(target.grouped);
    }
    #[test] fn public_shortcut_properties_cannot_bypass_the_phone_wire_contract() {
        let mut device=phone_device();
        for key in ["sleep-mode","bluetooth"] {
            device.properties.push(crate::model::Property{key:key.into(),format:"bool".into(),readable:true,writable:true,..Default::default()});
        }
        assert!(Command::Sleep(true).validate_spec(&device).is_ok());
        assert!(Command::Bluetooth(true).validate_spec(&device).is_ok());
        for hardware in ["LX04","T646"] {
            let mut row=phone_row();row["hardware"]=json!(hardware);
            assert!(Command::Sleep(true).validate(&device,&phone_state(&device,row)).is_err());
        }
        let mut no_night=phone_row();no_night["capabilities"]=json!({});
        assert!(Command::Sleep(true).validate(&device,&phone_state(&device,no_night)).is_err());
        for hardware in ["UNKNOWN","X10A"] {
            let mut row=phone_row();row["hardware"]=json!(hardware);
            assert!(Command::Bluetooth(true).validate(&device,&phone_state(&device,row)).is_err());
        }
        let mut grouped=phone_row();grouped["stereoJson"]=json!("fixture-group");
        let state=phone_state(&device,grouped);
        for command in [Command::Sleep(true),Command::Bluetooth(true),Command::Play("play"),Command::Volume(40),Command::Loop(0),Command::Seek(60_000)] {
            assert!(command.validate(&device,&state).is_err());
        }
        let valid=phone_state(&device,phone_row());
        assert!(Command::Sleep(true).validate(&device,&valid).is_ok());
        assert!(Command::Bluetooth(true).validate(&device,&valid).is_ok());
        let without_adapter=Snapshot{adapter:None,..valid};
        assert!(Command::Sleep(true).validate(&device,&without_adapter).is_err());
        assert!(Command::Bluetooth(true).validate(&device,&without_adapter).is_err());
    }
    #[test] fn another_speakers_group_does_not_disable_the_current_speaker() {
        let group=json!({"masterDeviceID":"fixture-other","wirelessDeviceList":[{"deviceId":"fixture-member"}]});
        let data=json!({"deviceList":[phone_row()],"groupInfo":[group.clone()]});
        assert!(!select_target(&data,"fixture-miot").unwrap().grouped);
        assert!(in_group(&data,"fixture-other"));assert!(in_group(&data,"fixture-member"));
        assert!(in_group(&json!({"groupInfo":[{}]}),"fixture-miot"));
        assert!(in_group(&json!({"groupInfo":{"unknown":true}}),"fixture-miot"));
        let mut data=data;data["groupInfo"][0]["wirelessDeviceList"]=json!([{}]);assert!(in_group(&data,"fixture-miot"));
    }
    #[test] fn bluetooth_quick_toggle_uses_connect_instead_of_radio_enable_and_never_defaults_missing_state() {
        assert_eq!(parse_bluetooth(&json!({"connect":1,"enable":false})).unwrap(),true);
        assert_eq!(parse_bluetooth(&json!({"connect":0,"enable":true})).unwrap(),false);
        for value in [json!({"enable":true}),json!({"connect":null}),json!({"connect":true}),json!({"connect":"1"}),json!({"connect":2}),Value::Null] {
            assert!(parse_bluetooth(&value).is_err());
        }
        assert!(parse_sleep(&json!({})).is_err());assert!(parse_sleep(&json!({"sleep_mode":"unknown"})).is_err());
        assert_eq!(parse_sleep(&json!({"sleep_mode":"night"})).unwrap(),true);
    }
    fn exercise_phone_control(command:Command,confirm_change:bool,optional_fail:bool,invalid_capability:bool)->Result<Snapshot,String> {
        use std::{io::{BufRead,BufReader,Read,Write},net::TcpListener,time::Instant};
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();let address=listener.local_addr().unwrap();
        let expected=match command {
            Command::Loop(3)=>("mediaplayer","player_set_loop",json!({"type":3,"media":"app_android"})),
            Command::Seek(60000)=>("mediaplayer","player_set_positon",json!({"position":60000,"media":"app_android"})),
            Command::Sleep(true)=>("nightmode","sleep_mode",json!({"value":"night"})),
            Command::Bluetooth(true)=>("mibt","enable",json!({"btmode":"classic","connect":1,"discover":0})),
            _=>panic!("fixture scenario is not defined"),
        };
        let expected_reads=if invalid_capability{4}else if confirm_change{6}else{8};
        let server=std::thread::spawn(move || {
            let deadline=Instant::now()+Duration::from_secs(9);let mut reads=0;let mut writes=0;
            while reads<expected_reads && Instant::now()<deadline {
                let (mut stream,_)=match listener.accept(){Ok(pair)=>pair,Err(error) if error.kind()==std::io::ErrorKind::WouldBlock=>{std::thread::sleep(Duration::from_millis(5));continue;},Err(error)=>panic!("fixture accept: {error}")};
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut reader=BufReader::new(stream.try_clone().unwrap());let mut first=String::new();reader.read_line(&mut first).unwrap();let mut length=0;
                loop {let mut line=String::new();reader.read_line(&mut line).unwrap();if line=="\r\n"{break;}assert!(!line.is_empty());
                    let (key,value)=line.split_once(':').unwrap();if key.eq_ignore_ascii_case("content-length"){length=value.trim().parse::<usize>().unwrap();}}
                assert!(length<16384);let mut body=vec![0;length];reader.read_exact(&mut body).unwrap();reads+=1;
                let data=if first.starts_with("GET /admin/v3/device_list?") {
                    let mut row=phone_row();if invalid_capability{row["capabilities"]=json!({});}json!({"deviceList":[row],"groupInfo":[]})
                }else{
                    assert!(first.starts_with("POST /remote/ubus?"));let fields:HashMap<String,String>=url::form_urlencoded::parse(&body).into_owned().collect();
                    assert_eq!(fields["deviceId"],"fixture-mina");let message:Value=serde_json::from_str(&fields["message"]).unwrap();
                    let method=fields["method"].as_str();let path=fields["path"].as_str();let changed=writes>0 && confirm_change;
                    match (path,method) {
                        ("mediaplayer","player_get_play_status")=>json!({"code":0,"info":json!({"status":1,"volume":47,"loop_type":if changed{3}else{1},"play_song_detail":{"title":"fixture song","duration":120000,"position":if changed{60000}else{30000}}}).to_string()}),
                        ("nightmode","status") if !optional_fail=>json!({"code":0,"info":json!({"sleep_mode":if changed{"night"}else{"normal"}}).to_string()}),
                        ("mibt","status") if !optional_fail=>{assert_eq!(message,json!({"btmode":"classic"}));json!({"code":0,"info":json!({"connect":if changed{1}else{0},"enable":false}).to_string()})},
                        ("nightmode"|"mibt","status")=>json!({"code":-1}),
                        _=>{assert_eq!((path,method), (expected.0,expected.1));assert_eq!(message,expected.2);writes+=1;assert_eq!(writes,1,"a device write must never be retried");json!({"code":0,"info":"{}"})},
                    }
                };
                let bytes=serde_json::to_vec(&json!({"code":0,"data":data})).unwrap();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();stream.write_all(&bytes).unwrap();
            }
            assert_eq!(reads,expected_reads);assert_eq!(writes,if invalid_capability{0}else{1});
        });
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let root=std::env::temp_dir().join(format!("homepanel-phone-control-fixture-{}-{}",std::process::id(),now_ms()));assert!(!root.exists());
        let main=Cloud::new(root.join("main"),root.join("cache")).unwrap();let mut service=Service::new(&root).unwrap();
        service.test_base=format!("http://{address}");service.client=reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(2)).redirect(reqwest::redirect::Policy::none()).build().unwrap();
        let session=crate::storage::Session{user_id:"fixture-account".into(),ssecurity:"synthetic-test-only".into(),service_token:"synthetic-test-only".into(),agent:"test".into()};
        let result=rt.block_on(async {
            *main.session.write().await=Some(session.clone());*service.auth.session.write().await=Some(session);
            // This fixture begins after load_spec; no public internet requests.
            service.control_loaded(&main,&phone_device(),command).await
        });
        server.join().unwrap();assert!(!root.exists());result
    }
    #[test] fn phone_writes_use_dedicated_contract_and_confirm_only_the_changed_field_over_http() {
        for command in [Command::Loop(3),Command::Seek(60000),Command::Sleep(true),Command::Bluetooth(true)] {
            let result=exercise_phone_control(command.clone(),true,false,false).unwrap();
            assert!(command.confirmed(&Snapshot::default(),&result));
        }
    }
    #[test] fn optional_read_failures_do_not_block_loop_confirmation_and_missing_capability_never_writes() {
        assert_eq!(exercise_phone_control(Command::Loop(3),true,true,false).unwrap().loop_type,Some(3));
        assert!(exercise_phone_control(Command::Sleep(true),true,false,true).err().unwrap().contains("睡眠控制能力"));
    }
    #[test] fn stale_live_progress_reads_do_not_pull_the_projected_position_backwards() {
        let previous=Snapshot{player_at:now_ms().saturating_sub(2_000),status:Some(1),track_audio:Some(101),duration_ms:Some(120_000),position_ms:Some(30_000),..Default::default()};
        let projected=previous.projected_position().unwrap();
        let current=Snapshot{player_at:now_ms(),status:Some(1),track_audio:Some(101),duration_ms:Some(120_000),position_ms:Some(27_000),..Default::default()};
        let merged=current.retain_last(&previous);
        assert!(merged.position_ms.unwrap_or(0)>=projected.saturating_sub(100));
        assert_eq!(merged.track_audio,Some(101));
    }
    #[test] fn accepted_but_unchanged_private_write_is_uncertain_and_sent_once() {
        let error=exercise_phone_control(Command::Bluetooth(true),false,false,false).err().unwrap();assert!(error.starts_with("UNCERTAIN:"));
    }
    #[test] fn authorized_loop_probe_always_restores_original_even_after_rejected_or_unconfirmed_change() {
        use std::{io::{BufRead,BufReader,Read,Write},net::TcpListener,time::Instant};
        for scenario in ["confirmed","rejected","unchanged"] {
            let listener=TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();let address=listener.local_addr().unwrap();
            let expected_reads=match scenario{"confirmed"=>10,"rejected"=>8,_=>12};
            let server=std::thread::spawn(move ||{
                let deadline=Instant::now()+Duration::from_secs(25);let mut calls=0;let mut mode=1;let mut writes=Vec::new();let mut written_at=None;
                while calls<expected_reads && Instant::now()<deadline {
                    let (mut stream,_)=match listener.accept(){Ok(pair)=>pair,Err(error) if error.kind()==std::io::ErrorKind::WouldBlock=>{std::thread::sleep(Duration::from_millis(5));continue;},Err(error)=>panic!("fixture accept: {error}")};
                    stream.set_nonblocking(false).unwrap();stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut reader=BufReader::new(stream.try_clone().unwrap());let mut first=String::new();reader.read_line(&mut first).unwrap();let mut length=0;let mut cookie=String::new();
                    loop {let mut line=String::new();reader.read_line(&mut line).unwrap();if line=="\r\n"{break;}assert!(!line.is_empty());let (key,value)=line.split_once(':').unwrap();
                        if key.eq_ignore_ascii_case("content-length"){length=value.trim().parse::<usize>().unwrap();}if key.eq_ignore_ascii_case("cookie"){cookie=value.to_owned();}}
                    assert!(length<16384);let mut body=vec![0;length];reader.read_exact(&mut body).unwrap();calls+=1;
                    let data=if first.starts_with("GET /admin/v3/device_list?"){json!({"deviceList":[phone_row()],"groupInfo":[]})}else{
                        assert!(first.starts_with("POST /remote/ubus?"));assert!(cookie.contains("isV3Protocol=1"));
                        let fields:HashMap<String,String>=url::form_urlencoded::parse(&body).into_owned().collect();let message:Value=serde_json::from_str(&fields["message"]).unwrap();
                        match (fields["path"].as_str(),fields["method"].as_str()) {
                            ("mediaplayer","player_get_play_status")=>{
                                if let Some(at)=written_at {assert!(Instant::now().duration_since(at)>=Duration::from_secs(3));}
                                json!({"code":0,"info":json!({"status":1,"loop_type":mode,"volume":47}).to_string()})
                            },
                            ("nightmode","status")=>json!({"code":0,"info":"{\"sleep_mode\":\"normal\"}"}),
                            ("mibt","status")=>json!({"code":0,"info":"{\"connect\":0}"}),
                            ("mediaplayer","player_set_loop")=>{
                                assert_eq!(message["media"],"app_android");let value=message["type"].as_u64().unwrap();writes.push(value);
                                assert_eq!(value,if writes.len()==1{0}else{1});assert!(writes.len()<=2);written_at=Some(Instant::now());
                                if writes.len()==1 && scenario=="rejected"{json!({"code":-1})}else{
                                    if writes.len()==2 || scenario=="confirmed"{mode=value;}
                                    json!({"code":0,"info":"{}"})
                                }
                            },
                            _=>panic!("probe sent an operation other than status or loop"),
                        }
                    };
                    let bytes=serde_json::to_vec(&json!({"code":0,"data":data})).unwrap();write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();stream.write_all(&bytes).unwrap();
                }
                assert_eq!(calls,expected_reads);assert_eq!(writes,vec![0,1]);assert_eq!(mode,1);
            });
            let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();let root=std::env::temp_dir().join(format!("homepanel-loop-probe-{}-{}",std::process::id(),now_ms()));assert!(!root.exists());
            let main=Cloud::new(root.join("main"),root.join("cache")).unwrap();let mut service=Service::new(&root).unwrap();service.test_base=format!("http://{address}");service.client=reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(2)).build().unwrap();
            let session=crate::storage::Session{user_id:"fixture-account".into(),ssecurity:"synthetic-test-only".into(),service_token:"synthetic-test-only".into(),agent:"test".into()};
            let result=rt.block_on(async{*main.session.write().await=Some(session.clone());*service.auth.session.write().await=Some(session);service.probe_loop_loaded(&main,&phone_device()).await});
            let report=serde_json::to_value(result).unwrap();assert_eq!(report[0]["code"],match scenario{"confirmed"=>"ok","unchanged"=>"unconfirmed",_=>"rejected"});assert_eq!(report[1]["stage"],"preflight");assert_eq!(report[2]["stage"],"loop_restore");assert_eq!(report[2]["code"],"ok");
            server.join().unwrap();assert!(!root.exists());
        }
    }
    #[test] fn loop_choices_follow_declared_options_instead_of_inventing_phone_capabilities() {
        let mut device=Device{kind:"speaker".into(),spec_status:"ready".into(),..Default::default()};
        device.properties.push(crate::model::Property{key:"play-loop-mode".into(),format:"uint8".into(),readable:true,writable:true,
            options:vec![crate::model::PropertyOption{value:json!(1),name:"Single Cycle".into()}],..Default::default()});
        assert!(Command::Loop(0).validate_spec(&device).is_ok());
        for mode in [1,2,3]{assert!(Command::Loop(mode).validate_spec(&device).is_err());}
        device.properties[0].options.push(crate::model::PropertyOption{value:json!(8),name:"Disorder".into()});
        assert!(Command::Loop(2).validate_spec(&device).is_err());
        device.properties[0].options.push(crate::model::PropertyOption{value:json!(7),name:"Order".into()});
        assert!(Command::Loop(2).validate_spec(&device).is_ok());
        assert_eq!(loop_mode_label("Random Order"),None);
        device.properties[0].writable=false;assert!(Command::Loop(0).validate_spec(&device).is_err());
    }
    #[test] fn v3_device_list_uses_the_declared_device_list_field_and_never_group_info() {
        let row=json!({"miotDID":"fixture-miot","deviceID":"fixture-mina","serialNumber":"fixture-sn","hardware":"fixture-hardware"});
        let data=parse_response(&json!({"code":0,"data":{"deviceList":[row],"groupInfo":[{"miotDID":"fixture-miot","deviceID":"wrong-group"}]}})).unwrap();
        let target=select_target(&data,"fixture-miot").unwrap();
        assert_eq!(target.id,"fixture-mina");assert_eq!(target.sn,"fixture-sn");assert_eq!(target.hardware,"fixture-hardware");assert!(!target.is_v3);
        let target=select_target(&json!({"devices":[row]}),"fixture-miot").unwrap();assert_eq!(target.id,"fixture-mina");
    }
    #[test] fn mina_envelope_accepts_declared_aliases_and_rejects_errors_or_ambiguous_results() {
        let data=json!({"deviceList":[{"miotDID":"fixture-miot","deviceID":"fixture-mina"}]});
        for value in [json!({"code":0,"data":data}),json!({"status":0,"result":data}),json!({"code":0,"data":null,"result":data})] {
            assert_eq!(parse_response(&value).unwrap(),data);
        }
        for value in [json!({"data":data}),json!({"code":"0","data":data}),json!({"code":401,"data":data}),
            json!({"code":0,"status":401,"data":data}),json!({"code":0,"data":data,"result":[]}),json!({"code":0,"data":null})] {
            assert!(parse_response(&value).is_err());
        }
    }
    #[test] fn missing_empty_malformed_and_conflicting_device_lists_stay_unavailable() {
        let row=json!({"miotDID":"fixture-miot","deviceID":"fixture-mina"});
        for data in [Value::Null,json!({}),json!({"groupInfo":[row]}),json!({"deviceList":null}),
            json!({"deviceList":"[]"}),json!({"deviceList":{},"devices":[row]}),
            json!({"deviceList":[row],"devices":[]})] {
            assert!(select_target(&data,"fixture-miot").is_err());
        }
        assert!(select_target(&json!({"deviceList":[]}),"fixture-miot").err().unwrap().contains("没有可用音箱"));
        assert!(select_target(&json!({"deviceList":[row.clone(),row]}),"fixture-miot").is_err());
    }
    #[test] fn full_read_uses_v3_mapping_and_nested_ubus_over_an_isolated_http_fixture() {
        use std::{io::{BufRead,BufReader,Read,Write},net::TcpListener,time::Instant};
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address=listener.local_addr().unwrap();
        let server=std::thread::spawn(move || {
            let deadline=Instant::now()+Duration::from_secs(5);let mut calls=Vec::new();
            while calls.len()<4 && Instant::now()<deadline {
                let (mut stream,_)=match listener.accept(){Ok(pair)=>pair,Err(error) if error.kind()==std::io::ErrorKind::WouldBlock=>{std::thread::sleep(Duration::from_millis(5));continue;},Err(error)=>panic!("fixture accept: {error}")};
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                let mut reader=BufReader::new(stream.try_clone().unwrap());let mut first=String::new();reader.read_line(&mut first).unwrap();
                let path=first.split_whitespace().nth(1).unwrap().split('?').next().unwrap().to_owned();
                let mut length=0;let mut cookie=String::new();
                loop {
                    let mut line=String::new();reader.read_line(&mut line).unwrap();
                    if line=="\r\n"{break;}assert!(!line.is_empty());
                    let (key,value)=line.split_once(':').unwrap();
                    if key.eq_ignore_ascii_case("content-length"){length=value.trim().parse::<usize>().unwrap();}
                    if key.eq_ignore_ascii_case("cookie"){cookie=value.trim().into();}
                }
                assert!(length<16384);let mut body=vec![0;length];reader.read_exact(&mut body).unwrap();
                let data=if path=="/admin/v3/device_list" {
                    assert!(first.starts_with("GET "));
                    calls.push("device_list".to_owned());
                    json!({"deviceList":[{"miotDID":"fixture-miot","deviceID":"fixture-mina","hardware":"fixture-hardware"}],"groupInfo":[]})
                }else{
                    assert_eq!(path,"/remote/ubus");assert!(first.starts_with("POST "));
                    let fields:HashMap<String,String>=url::form_urlencoded::parse(&body).into_owned().collect();
                    assert_eq!(fields["deviceId"],"fixture-mina");assert!(cookie.contains("deviceId=fixture-mina"));
                    assert!(cookie.contains("isV3Protocol=0"));
                    let method=&fields["method"];calls.push(method.clone());
                    match method.as_str() {
                        "player_get_play_status"=>{
                            assert_eq!(fields["path"],"mediaplayer");assert_eq!(serde_json::from_str::<Value>(&fields["message"]).unwrap(),json!({"media":"app_ios"}));
                            json!({"code":0,"info":json!({"status":1,"volume":47,"loop_type":1,"play_song_detail":{"title":"fixture song","artist":"fixture artist","duration":120000,"position":30000}}).to_string()})
                        },
                        "status"=>{assert!(matches!(fields["path"].as_str(),"nightmode"|"mibt"));json!({"code":-1})},
                        _=>panic!("fixture must never receive a write"),
                    }
                };
                let bytes=serde_json::to_vec(&json!({"code":0,"data":data})).unwrap();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
                stream.write_all(&bytes).unwrap();
            }
            assert_eq!(calls,vec!["device_list","player_get_play_status","status","status"]);
        });
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let root=std::env::temp_dir().join(format!("homepanel-speaker-http-fixture-{}-{}",std::process::id(),now_ms()));assert!(!root.exists());
        let main=Cloud::new(root.join("main"),root.join("cache")).unwrap();let mut service=Service::new(&root).unwrap();
        // Transport override exists only in the test binary, never Release/config.
        service.test_base=format!("http://{address}");service.client=reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(3)).redirect(reqwest::redirect::Policy::none()).build().unwrap();
        let fixture=crate::storage::Session{user_id:"fixture-account".into(),ssecurity:"synthetic-test-only".into(),service_token:"synthetic-test-only".into(),agent:"test".into()};
        rt.block_on(async {
            *main.session.write().await=Some(fixture.clone());*service.auth.session.write().await=Some(fixture);
            let device=Device{id:"fixture-miot".into(),model:"fixture.speaker".into(),kind:"speaker".into(),online:true,..Default::default()};
            let state=service.read(&main,&device).await.unwrap();
            assert!(state.current());assert_eq!(state.status,Some(1));assert_eq!(state.volume,Some(47));
            assert_eq!(state.title,"fixture song");assert_eq!(state.artist,"fixture artist");
            assert_eq!(state.duration_ms,Some(120000));assert_eq!(state.position_ms,Some(30000));
            assert_eq!(state.sleeping,None);assert_eq!(state.bluetooth,None);
            assert!(state.sleep_error.as_deref().is_some_and(|error|error.contains("-1")));
            assert!(state.bluetooth_error.as_deref().is_some_and(|error|error.contains("-1")));
        });
        server.join().unwrap();assert!(!root.exists());
    }
    #[test] fn ubus_requires_both_codes_and_parses_nested_info_without_fake_success() {
        assert_eq!(parse_ubus(&json!({"code":0,"info":"{\"status\":1}"})).unwrap()["status"],1);
        for value in [json!({"info":"{}"}),json!({"code":-1,"info":"{}"}),json!({"code":0,"info":"broken"})]{assert!(parse_ubus(&value).is_err());}
        assert!(parse_player(&Value::Null).is_err());
    }
    #[test] fn mapping_uses_exact_miot_id_never_device_name_or_a_guessed_id() {
        let a=json!({"miotDID":"one","deviceID":"mina-one","name":"同名"});let b=json!({"miotDID":"two","deviceID":"mina-two","name":"同名"});
        assert_eq!(select_target(&json!([a,b]),"two").unwrap().id,"mina-two");
        assert!(select_target(&json!([a.clone(),a]),"one").is_err());
        assert!(select_target(&json!([{"name":"同名","deviceID":"one"}]),"one").is_err());
    }
    #[test] fn observed_player_enums_and_invalid_values_are_preserved() {
        for (code,label) in [(0,"已停止"),(1,"播放中"),(2,"已暂停")] {assert_eq!(parse_player(&json!({"status":code})).unwrap().state_text(),label);}
        let s=parse_player(&json!({"status":99,"volume":999,"loop_type":99})).unwrap();
        assert!(s.state_text().contains("99"));assert_eq!(s.volume,None);assert_eq!(s.loop_type,None);
    }
    #[test] fn command_shapes_preserve_milliseconds_and_original_protocol_spelling() {
        let (_,method,payload)=Command::Seek(30000).wire().unwrap();assert_eq!(method,"player_set_positon");assert_eq!(payload["position"],30000);
        assert!(Command::Volume(101).wire().is_err());assert!(Command::Loop(4).wire().is_err());assert!(Command::Play("remove").wire().is_err());
        assert_eq!(Command::Sleep(true).wire().unwrap().2["value"],"night");
    }
    #[test] fn acceptance_or_an_old_song_cannot_confirm_a_command() {
        let before=Snapshot{title:"old".into(),..Default::default()};
        assert!(!Command::Play("next").confirmed(&before,&before));
        assert!(!Command::Volume(30).confirmed(&before,&Snapshot{volume:Some(29),..Default::default()}));
        assert!(Command::Play("next").confirmed(&before,&Snapshot{title:"new".into(),..Default::default()}));
    }
    #[test] fn private_status_does_not_invent_spec_write_permissions() {
        let mut device=Device{kind:"speaker".into(),spec_status:"ready".into(),..Default::default()};
        assert!(Command::Bluetooth(true).validate_spec(&device).is_err());
        assert!(Command::Seek(30000).validate_spec(&device).is_err());
        device.properties.push(crate::model::Property{key:"volume".into(),format:"uint8".into(),readable:true,writable:true,range:Some(vec![0.,100.,1.]),..Default::default()});
        assert!(Command::Volume(30).validate_spec(&device).is_ok());
        assert!(Command::Volume(101).validate_spec(&device).is_err());
    }
    #[test] fn different_service_accounts_are_rejected_before_any_network_request() {
        let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let root=std::env::temp_dir().join(format!("homepanel-speaker-absent-test-{}-{}",std::process::id(),now_ms()));
        assert!(!root.exists());
        let main=Cloud::new(root.join("main"),root.join("cache")).unwrap();
        let service=Service::new(&root).unwrap();
        let fixture=|id:&str|crate::storage::Session{user_id:id.into(),ssecurity:"synthetic-test-only".into(),service_token:"synthetic-test-only".into(),agent:"test".into()};
        rt.block_on(async {
            *main.session.write().await=Some(fixture("test-account-a"));
            *service.auth.session.write().await=Some(fixture("test-account-b"));
            assert!(service.identity(&main).await.is_err());
            *service.auth.session.write().await=Some(fixture("test-account-a"));
            assert!(service.identity(&main).await.is_ok());
        });
        assert!(!root.exists());
    }
}
