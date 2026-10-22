//! Capability and data contracts shared by the independently implemented phone
//! SDK adapter and the selected-device Rust broker. No network proxy is exposed.
use crate::model::{Device, Property, StateSource};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

pub const MAX_TARGETS: usize = 12;
pub const MAX_STATE_AGE_MS: u64 = 15_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyTarget {
    pub siid: u32,
    pub piid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub miid: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionTarget {
    pub siid: u32,
    pub aiid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub miid: Option<u32>,
}

pub fn checked_property<'a>(device: &'a Device, target: &PropertyTarget, write: bool) -> Result<&'a Property, &'static str> {
    if target.miid.is_some() { return Err("SPEC_V3_UNSUPPORTED"); }
    if target.siid == 0 || target.piid == 0 { return Err("INVALID_PROPERTY_TARGET"); }
    if device.spec_status != "ready" { return Err("SPEC_UNAVAILABLE"); }
    let mut matches = device.properties.iter().filter(|p| p.siid == target.siid && p.piid == target.piid);
    let property = matches.next().ok_or("UNKNOWN_PROPERTY")?;
    if matches.next().is_some() { return Err("AMBIGUOUS_PROPERTY"); }
    if if write { !property.writable } else { !property.readable } { return Err("PERMISSION_DENIED"); }
    Ok(property)
}

pub fn validate_targets(device: &Device, targets: &[PropertyTarget]) -> Result<(), &'static str> {
    if targets.is_empty() || targets.len() > MAX_TARGETS { return Err("INVALID_TARGET_COUNT"); }
    let mut seen = HashSet::new();
    for target in targets {
        checked_property(device, target, false)?;
        if !seen.insert((target.siid, target.piid)) { return Err("DUPLICATE_TARGET"); }
    }
    Ok(())
}

fn read_value_matches_format(property: &Property, value: &Value) -> bool {
    match property.format.as_str() {
        "bool" => value.is_boolean(),
        "string" => value.as_str().is_some_and(|text| text.len() <= 1024),
        "float" | "double" => value.as_f64().is_some_and(f64::is_finite),
        "uint8" | "uint16" | "uint32" | "uint64" => value.as_u64().is_some(),
        "int8" | "int16" | "int32" | "int64" => value.as_i64().is_some(),
        _ => false,
    }
}

/// Only recent direct device values can cross into the plugin as available.
/// Cached, failed, future-dated and malformed values remain explicitly unknown.
pub fn direct_value_at(property: &Property, now: u64) -> Value {
    let recent = property.checked_at.is_some_and(|at| at != 0 && at <= now && now - at <= MAX_STATE_AGE_MS);
    let direct = property.source == StateSource::Device && property.error.is_none();
    let typed = property.value.as_ref().is_some_and(|value| read_value_matches_format(property, value));
    let available = property.readable && recent && direct && typed;
    let error = if available { None } else if !property.readable { Some("PERMISSION_DENIED") }
        else if direct && typed && !recent { Some("STALE_DEVICE_STATE") }
        else if direct && recent && property.value.is_some() && !typed { Some("READ_VALUE_TYPE_MISMATCH") }
        else { Some("DIRECT_STATE_UNAVAILABLE") };
    json!({"siid":property.siid,"piid":property.piid,"available":available,
        "value":if available {property.value.clone()} else {None},
        "source":property.source,"checkedAt":property.checked_at,"error":error})
}

fn daily_action(key: &str) -> bool {
    matches!(key, "start" | "stop" | "pause" | "play" | "previous" | "next" | "start-sweep" | "stop-sweeping")
}

/// This extension deliberately supports only the zero-input daily actions that
/// the native application already exposes. Public metadata is not a grant to
/// execute reset, provisioning, update or an arbitrary manufacturer operation.
pub fn checked_action(device: &Device, instance: &Value, target: &ActionTarget, inputs: &[Value], writes_allowed: bool) -> Result<(), &'static str> {
    if !writes_allowed { return Err("READ_ONLY_SESSION"); }
    if target.miid.is_some() { return Err("SPEC_V3_UNSUPPORTED"); }
    if target.siid == 0 || target.aiid == 0 { return Err("INVALID_ACTION_TARGET"); }
    if device.spec_status != "ready" || device.spec_urn.is_empty() { return Err("SPEC_UNAVAILABLE"); }
    if !device.online { return Err("DEVICE_OFFLINE"); }
    if !inputs.is_empty() { return Err("ACTION_INPUT_UNSUPPORTED"); }
    let mut allowed = device.actions.iter().filter(|a| a.siid == target.siid && a.aiid == target.aiid);
    let selected = allowed.next().ok_or("UNKNOWN_ACTION")?;
    if allowed.next().is_some() { return Err("AMBIGUOUS_ACTION"); }
    if !daily_action(&selected.key) { return Err("ACTION_NOT_ALLOWED"); }
    if instance["type"].as_str() != Some(device.spec_urn.as_str()) { return Err("SPEC_DEVICE_MISMATCH"); }
    let services = instance["services"].as_array().ok_or("SPEC_UNAVAILABLE")?;
    let mut services = services.iter().filter(|s| s["iid"].as_u64() == Some(target.siid as u64));
    let service = services.next().ok_or("UNKNOWN_ACTION")?;
    if services.next().is_some() { return Err("AMBIGUOUS_SERVICE"); }
    let actions = service["actions"].as_array().ok_or("UNKNOWN_ACTION")?;
    let mut actions = actions.iter().filter(|a| a["iid"].as_u64() == Some(target.aiid as u64));
    let action = actions.next().ok_or("UNKNOWN_ACTION")?;
    if actions.next().is_some() { return Err("AMBIGUOUS_ACTION"); }
    if crate::model::type_key(&action["type"]) != selected.key { return Err("SPEC_ACTION_MISMATCH"); }
    if !action["in"].as_array().is_some_and(Vec::is_empty) { return Err("ACTION_INPUT_UNSUPPORTED"); }
    if !action["out"].as_array().is_some_and(Vec::is_empty) { return Err("ACTION_OUTPUT_UNSUPPORTED"); }
    Ok(())
}

/// Validate the original action result before producing a credential-free reply.
/// An accepted action has no universal state predicate, so confirmed stays false.
pub fn action_reply(result: &Value, target: &ActionTarget, expected_device_id: &str) -> Result<Value, &'static str> {
    if result["code"].as_i64().ok_or("INVALID_ACTION_RESPONSE")? != 0 { return Err("ACTION_REJECTED"); }
    if result["siid"].as_u64() != Some(target.siid as u64) || result["aiid"].as_u64() != Some(target.aiid as u64) {
        return Err("ACTION_RESPONSE_TARGET_MISMATCH");
    }
    if let Some(did) = result.get("did") {
        if crate::model::string_id(did) != expected_device_id { return Err("ACTION_RESPONSE_DEVICE_MISMATCH"); }
    }
    if !result["out"].as_array().is_some_and(Vec::is_empty) { return Err("INVALID_ACTION_OUTPUT"); }
    Ok(json!({"siid":target.siid,"aiid":target.aiid,"code":0,"out":[],"accepted":true,"confirmed":false}))
}

/// Describe actual broker capabilities rather than advertising the full phone SDK.
pub fn capabilities(device: &Device, instance: Option<&Value>, writes_allowed: bool) -> Value {
    let ready = device.spec_status == "ready";
    let properties: Vec<_> = device.properties.iter().map(|property| {
        let target = PropertyTarget{siid:property.siid,piid:property.piid,miid:None};
        let read = checked_property(device, &target, false).is_ok();
        // The native write contract requires readback, never only cloud acceptance.
        let write = writes_allowed && property.readable && checked_property(device, &target, true).is_ok();
        json!({"siid":property.siid,"piid":property.piid,"read":read,"write":write,"notify":read,"transport":"polling"})
    }).collect();
    let actions: Vec<_> = device.actions.iter().map(|action| {
        let target = ActionTarget{siid:action.siid,aiid:action.aiid,miid:None};
        let permission = instance.ok_or("SPEC_UNAVAILABLE").and_then(|instance| checked_action(device, instance, &target, &[], writes_allowed));
        json!({"siid":action.siid,"aiid":action.aiid,"enabled":permission.is_ok(),"error":permission.err(),
            "inputSupported":false,"outputSupported":false,"confirmation":"accepted-only"})
    }).collect();
    json!({"protocol":2,"selectedDeviceOnly":true,"readOnly":!writes_allowed,"specReady":ready,
        "stateTransport":"polling","cloudPush":false,"maxTargets":MAX_TARGETS,"maxStateAgeMs":MAX_STATE_AGE_MS,
        "properties":properties,"actions":actions,"unsupported":["spec-v3","arbitrary-rpc","arbitrary-cloud-api","cross-device","device-removal"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, StateSource};

    fn device() -> Device {
        Device{spec_status:"ready".into(),spec_urn:"urn:miot-spec-v2:device:speaker:fixture".into(),online:true,
            properties:vec![Property{siid:2,piid:1,format:"bool".into(),readable:true,writable:true,value:Some(json!(true)),
                source:StateSource::Device,checked_at:Some(100_000),..Default::default()}],
            actions:vec![Action{siid:3,aiid:1,key:"play".into(),..Default::default()}],..Default::default()}
    }
    fn instance() -> Value {
        json!({"type":"urn:miot-spec-v2:device:speaker:fixture","services":[{"iid":3,"actions":[
            {"iid":1,"type":"urn:miot-spec-v2:action:play:fixture","in":[],"out":[]}]}]})
    }
    fn action() -> ActionTarget {ActionTarget{siid:3,aiid:1,miid:None}}

    #[test]
    fn extension_targets_reject_implicit_cross_device_or_extra_fields() {
        for raw in [r#"{"siid":2,"piid":1,"did":"other"}"#,r#"{"siid":2,"piid":1,"token":"test"}"#] {
            assert!(serde_json::from_str::<PropertyTarget>(raw).is_err());
        }
        assert!(serde_json::from_str::<ActionTarget>(r#"{"siid":3,"aiid":1,"method":"reset"}"#).is_err());
        let d=device();
        assert!(checked_property(&d,&PropertyTarget{siid:0,piid:1,miid:None},false).is_err());
        assert!(checked_property(&d,&PropertyTarget{siid:2,piid:1,miid:Some(0)},false).is_err());
        assert!(validate_targets(&d,&[]).is_err());
        assert!(validate_targets(&d,&vec![PropertyTarget{siid:2,piid:1,miid:None};13]).is_err());
        assert!(validate_targets(&d,&vec![PropertyTarget{siid:2,piid:1,miid:None};2]).is_err());
    }

    #[test]
    fn stale_future_failed_cached_and_malformed_property_values_do_not_become_available() {
        let d=device();let property=d.properties[0].clone();
        assert_eq!(direct_value_at(&property,100_100)["value"],true);
        for at in [None,Some(0),Some(80_000),Some(100_101)] {
            let mut p=property.clone();p.checked_at=at;
            assert_eq!(direct_value_at(&p,100_100)["available"],false);
        }
        for (source,error,value) in [(StateSource::Cloud,None,json!(true)),
            (StateSource::Device,Some("fixture-failure".into()),json!(true)),
            (StateSource::Device,None,json!("true"))] {
            let mut p=property.clone();p.source=source;p.error=error;p.value=Some(value);
            let row=direct_value_at(&p,100_100);assert_eq!(row["available"],false);assert!(row["value"].is_null());
        }
    }

    #[test]
    fn actions_require_a_unique_current_public_daily_zero_input_output_contract() {
        let d=device();let spec=instance();let t=action();
        assert!(checked_action(&d,&spec,&t,&[],true).is_ok());
        assert_eq!(checked_action(&d,&spec,&t,&[],false),Err("READ_ONLY_SESSION"));
        assert_eq!(checked_action(&d,&spec,&t,&[json!(1)],true),Err("ACTION_INPUT_UNSUPPORTED"));
        for (field,value,error) in [("in",json!([1]),"ACTION_INPUT_UNSUPPORTED"),("out",json!([1]),"ACTION_OUTPUT_UNSUPPORTED"),
            ("in",Value::Null,"ACTION_INPUT_UNSUPPORTED"),("out",Value::Null,"ACTION_OUTPUT_UNSUPPORTED")] {
            let mut s=spec.clone();s["services"][0]["actions"][0][field]=value;
            assert_eq!(checked_action(&d,&s,&t,&[],true),Err(error));
        }
        let mut destructive=d.clone();destructive.actions[0].key="reset".into();
        assert_eq!(checked_action(&destructive,&spec,&t,&[],true),Err("ACTION_NOT_ALLOWED"));
        let mut ambiguous=d.clone();ambiguous.actions.push(d.actions[0].clone());
        assert_eq!(checked_action(&ambiguous,&spec,&t,&[],true),Err("AMBIGUOUS_ACTION"));
        let mut mismatched=spec.clone();mismatched["type"]=json!("urn:miot-spec-v2:device:outlet:other");
        assert_eq!(checked_action(&d,&mismatched,&t,&[],true),Err("SPEC_DEVICE_MISMATCH"));
        let mut offline=d.clone();offline.online=false;
        assert_eq!(checked_action(&offline,&spec,&t,&[],true),Err("DEVICE_OFFLINE"));
    }

    #[test]
    fn accepted_action_reply_never_fabricates_a_confirmed_state_or_unknown_outputs() {
        let t=action();let result=json!({"did":"fixture-device","siid":3,"aiid":1,"code":0,"out":[]});
        let reply=action_reply(&result,&t,"fixture-device").unwrap();
        assert_eq!(reply["accepted"],true);assert_eq!(reply["confirmed"],false);assert!(reply.get("did").is_none());
        for (field,value) in [("did",json!("other")),("siid",json!(9)),("aiid",json!(9)),("code",json!(-1)),
            ("out",json!([1])),("out",Value::Null)] {
            let mut wrong=result.clone();wrong[field]=value;assert!(action_reply(&wrong,&t,"fixture-device").is_err());
        }
    }

    #[test]
    fn capability_report_disables_read_only_writes_and_never_claims_cloud_push() {
        let d=device();let spec=instance();let report=capabilities(&d,Some(&spec),false);
        assert_eq!(report["selectedDeviceOnly"],true);assert_eq!(report["cloudPush"],false);
        assert_eq!(report["properties"][0]["read"],true);assert_eq!(report["properties"][0]["write"],false);
        assert_eq!(report["actions"][0]["enabled"],false);
        let writable=capabilities(&d,Some(&spec),true);
        assert_eq!(writable["properties"][0]["write"],true);assert_eq!(writable["actions"][0]["enabled"],true);
    }
}
