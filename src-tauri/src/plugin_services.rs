//! Narrow, selected-device services. No arbitrary path/RPC proxy is exposed.
use serde_json::{json,Value};

pub fn privacy_authorized(raw:&Value,expected_did:&str)->Result<bool,&'static str>{
    let selected=raw.as_object().and_then(|rows|rows.get(expected_did)).and_then(Value::as_object)
        .ok_or("PRIVACY_STATUS_UNAVAILABLE")?;
    let Some(value)=selected.get("prop.s_auth_config") else{return Ok(false);};
    let Some(text)=value.as_str() else{return Err("PRIVACY_STATUS_INVALID");};
    if text.len()>4096{return Err("PRIVACY_STATUS_INVALID");}
    if text.is_empty(){return Ok(false);}
    let config:Value=serde_json::from_str(text).map_err(|_|"PRIVACY_STATUS_INVALID")?;
    match config.get("privacyAuthed") {
        Some(Value::Bool(value))=>Ok(*value),None=>Ok(false),_=>Err("PRIVACY_STATUS_INVALID"),
    }
}

fn object(value:&Value)->Result<Value,&'static str>{
    let parsed=if let Some(text)=value.as_str(){if text.len()>64*1024{return Err("SPEAKER_RESPONSE_TOO_LARGE");}serde_json::from_str(text).map_err(|_|"SPEAKER_RESPONSE_INVALID")?}else{value.clone()};
    if !parsed.is_object(){return Err("SPEAKER_RESPONSE_INVALID");}Ok(parsed)
}
/// Preserve actual status fields while withholding identifiers, URLs and unknown payloads.
pub fn speaker_reply(raw:&Value)->Result<Value,&'static str>{
    let payload=object(raw)?;if payload["code"].as_i64()!=Some(0){return Err("SPEAKER_STATE_UNAVAILABLE");}
    let info=object(&payload["info"])?;
    if !info["status"].is_number()||!info["volume"].is_number(){return Err("SPEAKER_RESPONSE_INVALID");}
    let mut clean=serde_json::Map::new();
    for key in ["status","volume","loop_type","media_type"] {if let Some(v)=info.get(key){if !v.is_number(){return Err("SPEAKER_RESPONSE_INVALID");}clean.insert(key.into(),v.clone());}}
    if let Some(detail)=info.get("play_song_detail").and_then(Value::as_object){
        let mut song=serde_json::Map::new();
        for key in ["duration","position","audio_id","global_id"]{if let Some(v)=detail.get(key){
            if !(v.is_number()||v.as_str().is_some_and(|s|s.len()<=128&&s.bytes().all(|c|c.is_ascii_alphanumeric()||b"_-".contains(&c)))){return Err("SPEAKER_RESPONSE_INVALID");}song.insert(key.into(),v.clone());
        }}clean.insert("play_song_detail".into(),Value::Object(song));
    }
    if let Some(tracks)=info.get("track_list").and_then(Value::as_array){if tracks.len()>4096{return Err("SPEAKER_RESPONSE_TOO_LARGE");}
        let mut clean_tracks=Vec::new();for track in tracks{if !(track.is_number()||track.as_str().is_some_and(|s|s.len()<=128&&s.bytes().all(|c|c.is_ascii_alphanumeric()||b"_-".contains(&c)))){return Err("SPEAKER_RESPONSE_INVALID");}clean_tracks.push(track.clone());}
        clean.insert("track_list".into(),Value::Array(clean_tracks));
    }
    let text=serde_json::to_string(&clean).map_err(|_|"SPEAKER_RESPONSE_INVALID")?;
    Ok(json!({"code":0,"result":{"code":0,"info":text}}))
}

/// Convert the verified `/scene/list` object to the small read-only shape the
/// phone plugin expects. IDs, device aliases and arbitrary scene actions never
/// cross the broker; `remove`/`save` are deliberately unavailable in JS.
pub fn timers_reply(raw:&Value,expected_did:&str)->Result<Value,&'static str>{
    let _=expected_did;
    let source=if let Some(array)=raw.as_array(){array.clone()}else if let Some(object)=raw.as_object(){
        object.values().cloned().collect()
    }else{return Err("TIMER_RESPONSE_INVALID");};
    if source.len()>512{return Err("TIMER_RESPONSE_TOO_LARGE");}
    let mut rows=Vec::with_capacity(source.len());
    for item in source {
        let object=item.as_object().ok_or("TIMER_RESPONSE_INVALID")?;
        let mut row=serde_json::Map::new();
        for key in ["name","setting","identify","title","enabled","enable_timer","timer_type"] {
            if let Some(value)=object.get(key) {
                if key=="name"||key=="title" {if value.as_str().is_none_or(|s|s.len()>256||s.chars().any(char::is_control)){return Err("TIMER_RESPONSE_INVALID");}}
                if key=="setting" && !value.is_object(){return Err("TIMER_RESPONSE_INVALID");}
                if key=="identify" && value.as_str().is_none_or(|s|s.len()>128||!s.bytes().all(|c|c.is_ascii_alphanumeric()||b"_-".contains(&c))){return Err("TIMER_RESPONSE_INVALID");}
                row.insert(key.into(),value.clone());
            }
        }
        rows.push(Value::Object(row));
    }
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn privacy_requires_the_selected_devices_explicit_cloud_record(){
        assert_eq!(privacy_authorized(&json!({"selected":{"prop.s_auth_config":"{\"privacyAuthed\":true}"}}),"selected"),Ok(true));
        for text in ["", "{}", "{\"privacyAuthed\":false}"] {
            assert_eq!(privacy_authorized(&json!({"selected":{"prop.s_auth_config":text}}),"selected"),Ok(false));
        }
        assert!(privacy_authorized(&json!({"other":{"prop.s_auth_config":"{\"privacyAuthed\":true}"}}),"selected").is_err());
        assert!(privacy_authorized(&json!({"selected":{"prop.s_auth_config":"{\"privacyAuthed\":\"true\"}"}}),"selected").is_err());
    }
    #[test]
    fn speaker_shape_preserves_only_real_bounded_status() {
        let raw=json!({"code":0,"info":"{\"status\":1,\"volume\":42,\"play_song_detail\":{\"duration\":100,\"position\":5,\"audio_id\":\"a_1\"},\"secret\":\"x\"}"});
        let value=speaker_reply(&raw).unwrap();let text=value.to_string();assert!(text.contains("status"));assert!(!text.contains("secret"));
        assert!(speaker_reply(&json!({"code":0,"info":"{\"status\":1}"})).is_err());
    }
    #[test]
    fn timer_shape_drops_ids_and_rejects_bad_rows() {
        let value=timers_reply(&json!({"abc":{"name":"定时","setting":{"enable_timer":"0"},"us_id":"private"}}),"selected").unwrap();
        assert_eq!(value.as_array().unwrap().len(),1);assert!(!value.to_string().contains("us_id"));
        assert!(timers_reply(&json!([{"name":"\n"}]),"selected").is_err());
    }
}

#[cfg(debug_assertions)]
pub fn run(args:&[String])->Option<i32>{
    let timers=args.get(1).map(String::as_str)==Some("--plugin-timers-read-probe");
    let privacy=args.get(1).map(String::as_str)==Some("--plugin-privacy-read-probe");
    if !timers&&!privacy&&args.get(1).map(String::as_str)!=Some("--plugin-speaker-read-probe"){return None;}
    let result=(||{
        let (root,cache)=crate::app_directories(false)?;let cloud=crate::cloud::Cloud::new(root.clone(),cache)?;
        let region=crate::load_preferences(&root).region;
        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_|"runtime")?;
        runtime.block_on(async{
            cloud.discover(&region).await?;
            let epoch=cloud.epoch.load(std::sync::atomic::Ordering::SeqCst);
            let model=if timers{args.get(2).map(String::as_str).unwrap_or("chuangmi.plug.212a01")}else{"xiaomi.wifispeaker.l05c"};
            let devices=cloud.devices.lock().await.values().filter(|d|d.model==model).map(|d|d.id.clone()).collect::<Vec<_>>();
            let mut results=Vec::new();
            fn shape(v:&Value)->Value {match v {Value::Object(m)=>Value::Object(m.iter().map(|(k,v)|(k.clone(),shape(v))).collect()),Value::Array(a)=>json!({"array":true,"count":a.len(),"sample":a.first().map(shape)}),Value::String(s)=>{
                if let Ok(v)=serde_json::from_str::<Value>(s){json!({"jsonString":true,"shape":shape(&v)})}else{json!({"string":true,"nonempty":!s.is_empty()})}
            },Value::Number(_)=>json!({"number":true}),Value::Bool(_)=>json!({"bool":true}),Value::Null=>Value::Null}}
            for id in devices {
                let device=cloud.load_spec(&id).await?;
                if privacy {
                    results.push(match cloud.plugin_privacy_status(&region,&device,epoch).await {
                        Ok(authorized)=>json!({"ok":true,"authorized":authorized}),Err(code)=>json!({"error":code}),
                    });continue;
                }
                let result=if timers{cloud.plugin_timers(&region,&device,epoch).await}else{cloud.plugin_speaker_status(&region,&device,epoch).await};
                results.push(match result {Ok(v)=>if timers{
                    let rows=match &v{Value::Object(m)=>m.values().cloned().collect::<Vec<_>>(),Value::Array(a)=>a.clone(),_=>vec![]};
                    json!({"ok":true,"isObject":v.is_object(),"isArray":v.is_array(),"count":rows.len(),"sample":rows.first().map(|row|{let mut out=serde_json::Map::new();for key in ["us_id","name","did","st_id","setting","enable","create_time","update_time"]{if let Some(value)=row.get(key){out.insert(key.into(),shape(value));}}Value::Object(out)})})
                }else{json!({"ok":true,"shape":shape(&v),"adapterValid":speaker_reply(&v).is_ok()})},Err(code)=>json!({"error":code})});
            }
            Ok::<_,String>(json!({"schema":1,"deviceWrites":false,"results":results}))
        })
    })();
    match result {Ok(report)=>{println!("{report}");Some(0)},Err(_)=>{println!("{}",json!({"error":"speaker_probe_failed","deviceWrites":false}));Some(2)}}
}
