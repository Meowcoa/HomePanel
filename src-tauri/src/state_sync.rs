//! State provenance and bounded polling; no account data or network side effects.
use crate::model::{Device, Property, StateSource};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub type ReadResult = Result<Value, String>;

pub fn request(did: &str, targets: &[(u32, u32)], fresh: bool) -> Value {
    let params: Vec<_> = targets.iter().map(|(siid, piid)|
        serde_json::json!({"did":did,"siid":siid,"piid":piid})).collect();
    let mut payload = serde_json::json!({"params":params});
    payload["datasource"] = serde_json::json!(if fresh {2} else {3});
    payload
}

/// A missing, ambiguous, rejected or wrong-device row is never a fresh value.
fn value<'a>(result: &'a ReadResult, did: &str, siid: u32, piid: u32) -> Option<&'a Value> {
    let mut rows = result.as_ref().ok()?.as_array()?.iter().filter(|row|
        crate::model::string_id(&row["did"]) == did &&
        row["siid"].as_u64() == Some(siid as u64) && row["piid"].as_u64() == Some(piid as u64));
    let row = rows.next()?;
    if rows.next().is_some() || row["code"].as_i64() != Some(0) || row["value"].is_null() { return None; }
    Some(&row["value"])
}

pub fn missing(did: &str, targets: &[(u32, u32)], fresh: &ReadResult) -> Vec<(u32, u32)> {
    targets.iter().copied().filter(|(siid, piid)| value(fresh, did, *siid, *piid).is_none()).collect()
}

fn failure_reason(did:&str,siid:u32,piid:u32,fresh:&ReadResult,cached:&ReadResult)->String {
    for result in [fresh,cached] {
        if let Ok(Value::Array(rows))=result {
            let matches:Vec<_>=rows.iter().filter(|row|crate::model::string_id(&row["did"])==did && row["siid"].as_u64()==Some(siid as u64) && row["piid"].as_u64()==Some(piid as u64)).collect();
            if matches.len()==1 {
                if let Some(code)=matches[0]["code"].as_i64().filter(|code|*code!=0){return format!("设备未返回此属性的有效状态（返回码 {code}），控制指令与状态读取可能分别支持");}
            }
        }
    }
    if let Err(error)=fresh {return format!("设备直读失败：{error}");}
    "未获取到此属性的状态，请稍后刷新".into()
}

pub fn apply(device: &mut Device, targets: &[(u32, u32)], fresh: &ReadResult, cached: &ReadResult, at: u64) {
    for property in &mut device.properties {
        if !targets.contains(&(property.siid, property.piid)) { continue; }
        let direct = value(fresh, &device.id, property.siid, property.piid);
        let reported = value(cached, &device.id, property.siid, property.piid);
        property.checked_at = Some(at);
        if let Some((value, source)) = direct.map(|v| (v, StateSource::Device))
            .or_else(|| reported.map(|v| (v, StateSource::Cloud))) {
            property.value = Some(value.clone());
            property.error = None;
            property.source = source;
            device.updated_at = Some(at);
        } else {
            // Keep the old value for rollback only. Presentation hides it behind the error.
            property.source = StateSource::Unknown;
            property.error = Some(failure_reason(&device.id,property.siid,property.piid,fresh,cached));
        }
    }
}

/// Late UI messages must not replace a newer confirmed property with an old snapshot.
pub fn merge(current: &mut Device, mut incoming: Device) {
    if current.model == incoming.model {
        if incoming.spec_status != "ready" && current.spec_status == "ready" {
            incoming.properties = current.properties.clone();
            incoming.actions = current.actions.clone();
            incoming.spec_urn = current.spec_urn.clone();
            incoming.spec_status = current.spec_status.clone();
            incoming.kind = current.kind.clone();
        }
        for property in &mut incoming.properties {
            if let Some(newer) = current.properties.iter().find(|p|
                p.siid == property.siid && p.piid == property.piid && p.checked_at > property.checked_at) {
                *property = newer.clone();
            }
        }
        incoming.updated_at = incoming.updated_at.max(current.updated_at);
    }
    *current = incoming;
}

#[derive(Default)]
pub struct PollSchedule { completed: HashMap<String, (u64, bool)> }

impl PollSchedule {
    pub fn completed(&mut self, id: &str, now: u64, direct: bool) { self.completed.insert(id.into(), (now, direct)); }
    pub fn forget(&mut self) { self.completed.clear(); }
    pub fn due(&self, devices: &[Device], selected: Option<&str>, reading: &HashSet<String>, busy: &HashSet<String>, now: u64) -> Vec<String> {
        let due = |id: &str, active: bool| {
            if reading.contains(id) || busy.contains(id) { return false; }
            match self.completed.get(id) {
                None => true,
                Some((at, direct)) => now.saturating_sub(*at) >= if active && *direct { 3_000 } else { 15_000 },
            }
        };
        let mut ids = Vec::new();
        // Selection is independent of dashboard sorting/filtering and the background quota.
        if let Some(id) = selected.filter(|id| due(id, true)) { ids.push(id.into()); }
        let mut background: Vec<_> = devices.iter().filter(|d|
            d.online && Some(d.id.as_str()) != selected && due(&d.id, false)).collect();
        background.sort_by_key(|d| self.completed.get(&d.id).map(|(at, _)| *at).unwrap_or(0));
        let capacity = 2usize.saturating_sub(reading.iter().filter(|id| Some(id.as_str()) != selected).count());
        ids.extend(background.into_iter().take(capacity).map(|d| d.id.clone()));
        ids
    }
}

pub fn direct(device: &Device) -> bool {
    let mut readable = device.properties.iter().filter(|p| p.readable).peekable();
    device.online && readable.peek().is_some() && readable.all(|p| p.error.is_none() && p.source == StateSource::Device)
}

pub fn property_provenance(property: &Property, now: u64) -> String {
    if !property.readable { return "此属性仅支持设置，不提供状态读取".into(); }
    if property.error.is_some() { return "本次读取失败；旧值不能代表当前状态".into(); }
    match (property.source, property.checked_at) {
        (StateSource::Device, Some(at)) => format!("设备回读 · {} 秒前（轮询，非实时推送）", now.saturating_sub(at) / 1000),
        (StateSource::Cloud, _) => "云端上报缓存 · 设备未返回直读状态，上报时间未知".into(),
        _ => "等待首次读取，尚未确认当前状态".into(),
    }
}

pub fn device_status(device: &Device, reading: bool, now: u64) -> String {
    if reading { return "正在读取设备状态…".into(); }
    if !device.online { return "云端列表显示离线 · 等待重新查询".into(); }
    let properties: Vec<_> = device.properties.iter().filter(|p| p.readable).collect();
    if properties.is_empty() { return "此设备未提供可轮询的状态".into(); }
    if properties.iter().all(|p| p.checked_at.is_none()) { return "等待首次读取".into(); }
    let failed = properties.iter().any(|p| p.error.is_some() || p.checked_at.is_none());
    let cached = properties.iter().any(|p| p.source == StateSource::Cloud);
    let age = properties.iter().filter_map(|p| p.checked_at).min().map(|at| now.saturating_sub(at) / 1000).unwrap_or(0);
    if failed { "部分状态未获取 · 可点击刷新".into() }
    else if cached { "含云端上报缓存 · 上报时间未知".into() }
    else if age >= 15 { format!("上次设备回读 {age} 秒前 · 等待刷新") }
    else { format!("设备回读 {age} 秒前 · 每 3 秒轮询") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn device() -> Device { Device { id:"test-device".into(), model:"test.model".into(), online:true,
        properties: vec![Property { siid:2, piid:1, readable:true, value:Some(json!(44)), ..Default::default() },
            Property { siid:2, piid:2, readable:true, value:Some(json!(false)), ..Default::default() }], ..Default::default() } }
    fn row(did: &str, piid:u32, value:Value) -> Value { json!({"did":did,"siid":2,"piid":piid,"code":0,"value":value}) }
    #[test]
    fn property_rejection_keeps_its_code_without_using_another_devices_error() {
        let mut d=device();
        apply(&mut d,&[(2,1)],&Ok(json!([{"did":"test-device","siid":2,"piid":1,"code":-4008}])),&Ok(json!([])),1000);
        assert!(d.properties[0].error.as_ref().unwrap().contains("-4008"));
        apply(&mut d,&[(2,1)],&Ok(json!([{"did":"other","siid":2,"piid":1,"code":-999}])),&Ok(json!([])),2000);
        assert!(!d.properties[0].error.as_ref().unwrap().contains("-999"));
    }
    #[test]
    fn fresh_request_uses_direct_rpc_and_fallback_reads_cache_only() {
        let payload=request("test-device",&[(2,1)],true);
        assert_eq!(payload["datasource"],2);
        assert_eq!(payload["params"][0]["did"],"test-device");
        assert_eq!(request("test-device",&[(2,1)],false)["datasource"],3);
    }
    #[test]
    fn missing_wrong_device_and_duplicate_rows_never_refresh_old_values() {
        for rows in [json!([]),json!([row("other",1,json!(99))]),
            json!([row("test-device",1,json!(1)),row("test-device",1,json!(2))]),
            json!([{"did":"test-device","siid":2,"piid":1,"code":-1,"value":0}])] {
            let mut d=device();
            apply(&mut d,&[(2,1)],&Ok(rows),&Ok(json!([])),1000);
            assert!(d.properties[0].error.is_some());
            assert_eq!(d.properties[0].value,Some(json!(44)));
            assert_eq!(d.updated_at,None);
        }
    }
    #[test]
    fn partial_fallback_marks_cache_and_does_not_overwrite_direct_or_unrequested_values() {
        let mut d=device();
        let fresh=Ok(json!([row("test-device",1,json!(55))]));
        assert_eq!(missing(&d.id,&[(2,1),(2,2)],&fresh),vec![(2,2)]);
        apply(&mut d,&[(2,1),(2,2)],&fresh,&Ok(json!([row("test-device",1,json!(1)),row("test-device",2,json!(true))])),1000);
        assert_eq!(d.properties[0].value,Some(json!(55)));
        assert_eq!(d.properties[0].source,StateSource::Device);
        assert_eq!(d.properties[1].source,StateSource::Cloud);
        assert!(property_provenance(&d.properties[1],2000).contains("时间未知"));
        apply(&mut d,&[(2,1)],&Err("timeout".into()),&Err("timeout".into()),3000);
        assert!(d.properties[0].error.is_some());
        assert_eq!(d.properties[1].checked_at,Some(1000));
        assert!(device_status(&d,false,3000).contains("未获取"));
    }
    #[test]
    fn late_read_snapshot_does_not_undo_confirmed_write() {
        let mut current=device();
        let mut old=current.clone();
        current.properties[0].checked_at=Some(2000);
        current.properties[0].value=Some(json!(100));
        old.properties[0].checked_at=Some(1000);
        old.properties[1].checked_at=Some(3000);
        old.properties[1].value=Some(json!(true));
        merge(&mut current,old);
        assert_eq!(current.properties[0].value,Some(json!(100)));
        assert_eq!(current.properties[1].value,Some(json!(true)));
    }
    #[test]
    fn discovery_preserves_loaded_properties_but_keeps_new_online_metadata() {
        let mut current=device();
        current.spec_status="ready".into();
        let incoming=Device{id:current.id.clone(),model:current.model.clone(),online:false,spec_status:"unloaded".into(),..Default::default()};
        merge(&mut current,incoming);
        assert!(!current.online);
        assert_eq!(current.properties.len(),2);
        assert_eq!(current.spec_status,"ready");
    }
    #[test]
    fn stale_cache_and_unreadable_statuses_never_claim_live_measurements() {
        let mut d=device();
        assert_eq!(device_status(&d,false,20_000),"等待首次读取");
        apply(&mut d,&[(2,1),(2,2)],&Ok(json!([row("test-device",1,json!(44)),row("test-device",2,json!(false))])),&Ok(json!([])),1000);
        assert!(device_status(&d,false,21_000).contains("等待刷新"));
        assert!(device_status(&d,true,21_000).contains("正在读取"));
        d.properties[0].source=StateSource::Cloud;
        assert!(device_status(&d,false,21_000).contains("时间未知"));
        assert!(property_provenance(&Property::default(),1000).contains("不提供状态读取"));
    }
    #[test]
    fn detail_beyond_first_twelve_has_priority_and_background_eventually_covers_every_device() {
        let devices:Vec<_>=(0..30).map(|i|Device{id:format!("test-{i}"),online:true,..Default::default()}).collect();
        let mut schedule=PollSchedule::default();
        let empty=HashSet::new();
        let selected=Some("test-29");
        assert_eq!(schedule.due(&devices,selected,&empty,&empty,0)[0],"test-29");
        let mut seen=HashSet::new();
        for tick in 0..16 {
            for id in schedule.due(&devices,selected,&empty,&empty,tick*100) {
                schedule.completed(&id,tick*100,true);seen.insert(id);
            }
        }
        assert_eq!(seen.len(),30);
        assert_eq!(schedule.due(&devices,selected,&empty,&empty,3000),vec!["test-29"]);
    }
    #[test]
    fn in_flight_reads_are_deduplicated_and_failed_reads_back_off_without_blocking_other_devices() {
        let devices=vec![device()];
        let mut schedule=PollSchedule::default();
        let empty=HashSet::new();
        schedule.completed("test-device",0,false);
        assert!(schedule.due(&devices,Some("test-device"),&empty,&empty,3000).is_empty());
        let reading=HashSet::from(["test-device".into()]);
        assert!(schedule.due(&devices,Some("test-device"),&reading,&empty,16000).is_empty());
        assert_eq!(schedule.due(&devices,Some("test-device"),&empty,&empty,16000),vec!["test-device"]);
    }
}
