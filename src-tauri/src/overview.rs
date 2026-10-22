//! Home activity is based on recent reported state, never online presence alone.
use crate::model::{Device, Property, StateSource};
use serde_json::Value;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Activity { On, Off, Unknown, Offline, NotApplicable }

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum ActivityFilter { #[default] On, Offline, Unknown }

impl ActivityFilter {
    pub fn title(self)-> &'static str { match self { Self::On=>"已开启设备",Self::Offline=>"离线设备",Self::Unknown=>"状态待确认" } }
    pub fn includes(self, device:&Device, now:u64)->bool {
        activity(device,now)==match self {Self::On=>Activity::On,Self::Offline=>Activity::Offline,Self::Unknown=>Activity::Unknown}
    }
}

pub fn power_property(device:&Device)->Option<&Property> {
    let candidates=power_properties(device);
    // A multi-channel device gets separate controls, never an arbitrary master switch.
    (candidates.len()==1).then(||candidates[0])
}

pub fn power_properties(device:&Device)->Vec<&Property> {
    if matches!(device.kind.as_str(),"speaker"|"sensor") { return vec![]; }
    device.properties.iter().filter(|p|p.key=="on" && p.writable &&
        !matches!(p.service_key.as_str(),"indicator-light"|"clock"|"microphone"|"no-disturb") &&
        !["指示灯","闹钟","麦克风","勿扰"].iter().any(|name|p.service.contains(name))).collect()
}

fn recent(property:&Property,now:u64)->bool {
    property.error.is_none() && property.source==StateSource::Device &&
        property.checked_at.is_some_and(|at|now.saturating_sub(at)<=45_000)
}

pub fn activity(device:&Device,now:u64)->Activity {
    if !device.online { return Activity::Offline; }
    if device.kind=="speaker" {
        let Some(property)=device.properties.iter().find(|p|p.key=="playing-state") else {return Activity::Unknown;};
        if !recent(property,now) { return Activity::Unknown; }
        let label=property.value.as_ref().and_then(|v|property.options.iter().find(|o|o.value==*v))
            .map(|o|crate::presentation::option_label(&o.name));
        return match label.as_deref() {Some("播放中"|"播放"|"正在播放")=>Activity::On,Some("已暂停"|"已停止"|"暂停"|"停止")=>Activity::Off,_=>Activity::Unknown};
    }
    let power=power_properties(device);
    if !power.is_empty() {
        if power.iter().any(|p|recent(p,now) && p.value.as_ref().and_then(Value::as_bool)==Some(true)) {return Activity::On;}
        return if power.iter().all(|p|recent(p,now) && p.value.as_ref().and_then(Value::as_bool)==Some(false)) {Activity::Off}else{Activity::Unknown};
    }
    if device.spec_status!="ready" && matches!(device.kind.as_str(),"plug"|"switch"|"light"|"fan"|"climate"|"purifier"|"humidifier"|"vacuum") {
        Activity::Unknown
    } else { Activity::NotApplicable }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn plug()->Device {Device{online:true,kind:"plug".into(),spec_status:"ready".into(),properties:vec![
        Property{key:"on".into(),service_key:"indicator-light".into(),writable:true,value:Some(json!(true)),source:StateSource::Device,checked_at:Some(1000),..Default::default()},
        Property{key:"on".into(),service_key:"switch".into(),writable:true,value:Some(json!(false)),source:StateSource::Device,checked_at:Some(1000),..Default::default()}],..Default::default()}}
    #[test]
    fn indicator_is_not_power_and_online_does_not_mean_on() {
        let d=plug();
        assert_eq!(power_property(&d).unwrap().service_key,"switch");
        assert_eq!(activity(&d,1000),Activity::Off);
        assert!(!ActivityFilter::On.includes(&d,1000));
        let mut sensor=d.clone();sensor.kind="sensor".into();
        assert_eq!(activity(&sensor,1000),Activity::NotApplicable);
    }
    #[test]
    fn stale_failed_cached_and_missing_values_are_not_counted_as_currently_on() {
        let mut d=plug();d.properties[1].value=Some(json!(true));
        assert_eq!(activity(&d,1000),Activity::On);
        assert_eq!(activity(&d,47000),Activity::Unknown);
        d.properties[1].source=StateSource::Cloud;
        assert_eq!(activity(&d,1000),Activity::Unknown);
        d.properties[1].source=StateSource::Device;d.properties[1].error=Some("failed".into());
        assert_eq!(activity(&d,1000),Activity::Unknown);
        d.online=false;assert!(ActivityFilter::Offline.includes(&d,1000));
    }
    #[test]
    fn speaker_playing_enum_is_used_instead_of_alarm_or_numeric_guessing() {
        let mut d=plug();d.kind="speaker".into();
        assert!(power_property(&d).is_none());
        assert_eq!(activity(&d,1000),Activity::Unknown);
        d.properties.push(Property{key:"playing-state".into(),value:Some(json!(7)),source:StateSource::Device,checked_at:Some(1000),
            options:vec![crate::model::PropertyOption{value:json!(7),name:"Playing".into()}],..Default::default()});
        assert_eq!(activity(&d,1000),Activity::On);
        d.properties.last_mut().unwrap().options[0].name="播放".into();assert_eq!(activity(&d,1000),Activity::On);
        d.properties.last_mut().unwrap().options[0].name="暂停".into();assert_eq!(activity(&d,1000),Activity::Off);
        d.properties.last_mut().unwrap().value=Some(json!(1));assert_eq!(activity(&d,1000),Activity::Unknown);
    }
    #[test]
    fn any_active_channel_counts_once_without_an_arbitrary_master_power() {
        let mut d=plug();
        let mut second=d.properties[1].clone();second.siid=3;second.value=Some(json!(true));d.properties.push(second);
        assert_eq!(activity(&d,1000),Activity::On);
        assert!(power_property(&d).is_none());
        d.properties.last_mut().unwrap().value=Some(json!(false));
        assert_eq!(activity(&d,1000),Activity::Off);
        d.properties.last_mut().unwrap().error=Some("missing".into());
        assert_eq!(activity(&d,1000),Activity::Unknown);
    }
}
