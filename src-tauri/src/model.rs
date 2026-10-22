use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Preferences {
    pub region: String,
    pub theme: String,
    pub glass: bool,
    #[serde(default)]
    pub glass_style: GlassStyle,
    pub autostart: bool,
    pub favorites: Vec<String>,
    #[serde(default)]
    pub hidden_devices: Vec<String>,
    pub approved_images: Vec<ApprovedImage>,
    #[serde(default = "default_native_panels")]
    pub native_panels: bool,
}
fn default_native_panels() -> bool { true }
impl Default for Preferences {
    fn default()->Self { Self { region:"cn".into(),theme:"system".into(),glass:true,glass_style:GlassStyle::Frosted,autostart:false,favorites:vec![],hidden_devices:vec![],approved_images:vec![],native_panels:true } }
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="kebab-case")]
pub enum GlassStyle { Glass, #[default] Frosted, Solid }

impl Preferences {
    pub fn material(&self, available: bool) -> GlassStyle {
        if self.glass && available { self.glass_style } else { GlassStyle::Solid }
    }
}

#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ApprovedImage { pub model:String, pub url:String, pub permission_url:String }

#[derive(Clone,Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct Home { pub id:String,pub name:String,pub owner:String,pub rooms:Vec<Room> }
#[derive(Clone,Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct Room { pub id:String,pub name:String,pub dids:Vec<String> }
#[derive(Clone,Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct Device {
    pub id:String,pub name:String,pub model:String,pub home_id:String,pub room:String,
    #[serde(default)]
    pub spec_urn:String,
    pub online:bool,pub kind:String,pub properties:Vec<Property>,pub actions:Vec<Action>,
    pub spec_status:String,pub updated_at:Option<u64>,pub image:Option<String>,
}
#[derive(Clone,Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct Property {
    pub siid:u32,pub piid:u32,pub key:String,pub name:String,pub service:String,pub format:String,
    #[serde(default)]
    pub service_key:String,
    pub unit:String,pub readable:bool,pub writable:bool,pub range:Option<Vec<f64>>,
    pub options:Vec<PropertyOption>,pub value:Option<Value>,pub error:Option<String>,
    /// Local completion time, never a device measurement timestamp.
    #[serde(default)]
    pub checked_at:Option<u64>,
    #[serde(default)]
    pub source:StateSource,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="kebab-case")]
pub enum StateSource { #[default] Unknown, Device, Cloud }
#[derive(Clone,Serialize,Deserialize)]
pub struct PropertyOption { pub value:Value,pub name:String }
#[derive(Clone,Serialize,Deserialize,Default)]
pub struct Action { pub siid:u32,pub aiid:u32,pub name:String,pub key:String }

pub fn string_id(v:&Value)->String { v.as_str().map(str::to_owned).unwrap_or_else(||if v.is_number(){v.to_string()}else{String::new()}) }
pub fn type_key(v:&Value)->String { v.as_str().unwrap_or_default().split(':').nth(3).unwrap_or_default().into() }
pub fn kind(model:&str)->String {
    [ ("airpurifier","purifier"),("humidifier","humidifier"),("light","light"),("lamp","light"),
      ("plug","plug"),("outlet","plug"),("fan","fan"),("sensor","sensor"),("ht","sensor"),
      ("vacuum","vacuum"),("aircondition","climate"),("camera","camera"),("speaker","speaker"),
      ("switch","switch"),("mosq","mosquito") ]
        .into_iter().find(|(key,_)|model.contains(key)).map(|(_,v)|v).unwrap_or("device").into()
}
/// Standard device type identifies artwork only; capabilities still come from SPEC.
pub fn kind_from_spec(urn:&str,model:&str)->String {
    let key=urn.strip_prefix("urn:miot-spec-v2:device:").and_then(|tail|tail.split(':').next());
    match key.unwrap_or_default() {
        "light" | "ceiling-light" => "light",
        "outlet" | "power-strip" => "plug",
        "switch" | "wireless-switch" => "switch",
        "speaker" => "speaker",
        "mosquito-dispeller" => "mosquito",
        "fan" => "fan",
        "temperature-humidity-sensor" | "temperature-sensor" | "humidity-sensor" | "sensor" => "sensor",
        "air-purifier" => "purifier",
        "humidifier" => "humidifier",
        "vacuum" => "vacuum",
        "air-conditioner" => "climate",
        "camera" => "camera",
        _ => return kind(model),
    }.into()
}
pub fn chinese(key:&str,description:&str)->String {
    // Manufacturer Chinese descriptions distinguish channels and vendor features.
    if has_chinese(description) { return description.trim().into(); }
    match key {
        "on"=>"电源", "brightness"=>"亮度", "color-temperature"=>"色温", "color"=>"颜色",
        "mode"=>"模式", "fan-level"=>"风速", "speed-level"=>"档位", "horizontal-swing"=>"左右摆风",
        "vertical-swing"=>"上下摆风", "temperature"=>"温度", "relative-humidity"=>"湿度",
        "target-temperature"=>"目标温度", "pm2.5-density"=>"PM2.5", "battery-level"=>"电量",
        "default-power-on-state" | "power-on-state"=>"来电状态", "power-loss-memory"=>"断电记忆",
        "electric-current"=>"电流", "voltage"=>"电压", "electric-power"=>"功率", "power-consumption"=>"用电量",
        "illumination"=>"照度", "co2-density"=>"二氧化碳", "tvoc-density"=>"TVOC", "filter-life-level"=>"滤芯余量",
        "surge-power"=>"过载功率", "button"=>"按键", "switch"=>"开关",
        "wireless-switch"=>"无线开关", "outlet"=>"插座",
        "fault"=>"故障状态", "status"=>"运行状态", "child-lock"=>"童锁", "air-purifier"=>"空气净化器",
        "air-conditioner"=>"空调", "light"=>"灯光", "fan"=>"风扇", "environment"=>"环境",
        "indicator-light"=>"指示灯", "alarm"=>"提示音", "start"=>"启动", "stop"=>"停止", "pause"=>"暂停",
        "speaker"=>"扬声器", "play-control"=>"播放控制", "microphone"=>"麦克风", "intelligent-speaker"=>"智能音箱",
        "clock"=>"闹钟", "volume"=>"音量", "mute"=>"静音", "playing-state"=>"播放状态", "audio-id"=>"音频编号",
        "sleep-mode"=>"睡眠模式", "no-disturb"=>"勿扰模式", "enable-time-period"=>"启用时段", "play-loop-mode"=>"循环模式",
        "previous"=>"上一首", "next"=>"下一首",
        "play"=>"播放", "start-sweep"=>"开始清扫", "stop-sweeping"=>"停止清扫",
        _=>"厂商属性（暂无中文）",
    }.into()
}
pub fn has_chinese(text: &str) -> bool {
    text.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
}

/// Public MIoT metadata uses value-list indexes, not the protocol enum values.
pub fn apply_spec_labels(properties: &mut [Property], actions: &mut [Action], labels: &Value) {
    let Some(table) = labels["data"]["zh_cn"].as_object() else { return; };
    // Parse numeric components so padded and unpadded metadata keys both work.
    let lookup = |siid: u32, kind: &str, iid: u32, index: Option<usize>| -> Option<String> {
        table.iter().find_map(|(key, value)| {
            let parts: Vec<_> = key.split(':').collect();
            if parts.first() != Some(&"service") || parts.get(1)?.parse::<u32>().ok()? != siid { return None; }
            let matches = if kind.is_empty() { parts.len() == 2 } else {
                parts.get(2) == Some(&kind) && parts.get(3)?.parse::<u32>().ok()? == iid &&
                    match index { Some(index) => parts.len() == 6 && parts.get(4) == Some(&"valuelist") && parts.get(5)?.parse::<usize>().ok()? == index, None => parts.len() == 4 }
            };
            let text = value.as_str()?.trim();
            (matches && has_chinese(text)).then(|| text.to_owned())
        })
    };
    for property in properties {
        if let Some(name) = lookup(property.siid, "", 0, None) { property.service = name; }
        if let Some(name) = lookup(property.siid, "property", property.piid, None) { property.name = name; }
        for (index, option) in property.options.iter_mut().enumerate() {
            if let Some(name) = lookup(property.siid, "property", property.piid, Some(index)) { option.name = name; }
        }
    }
    for action in actions {
        if let Some(name) = lookup(action.siid, "action", action.aiid, None) { action.name = name; }
    }
}

pub fn parse_spec(value:&Value)->(Vec<Property>,Vec<Action>) {
    let mut props=vec![]; let mut actions=vec![];
    for service in value["services"].as_array().into_iter().flatten() {
        let siid=service["iid"].as_u64().unwrap_or(0) as u32;
        let skey=type_key(&service["type"]);
        if skey=="device-information" { continue; }
        let service_name=chinese(&skey,service["description"].as_str().unwrap_or_default());
        for p in service["properties"].as_array().into_iter().flatten() {
            let key=type_key(&p["type"]); let access=p["access"].as_array().cloned().unwrap_or_default();
            if !access.iter().any(|v|v=="read" || v=="write") {continue;}
            let options=p["value-list"].as_array().into_iter().flatten().map(|o|PropertyOption{value:o["value"].clone(),name:o["description"].as_str().unwrap_or_default().into()}).collect();
            let range=p["value-range"].as_array().and_then(|r|r.iter().map(Value::as_f64).collect::<Option<Vec<_>>>());
            props.push(Property{siid,piid:p["iid"].as_u64().unwrap_or(0) as u32,key:key.clone(),name:chinese(&key,p["description"].as_str().unwrap_or_default()),service:service_name.clone(),service_key:skey.clone(),format:p["format"].as_str().unwrap_or("unknown").into(),unit:p["unit"].as_str().unwrap_or_default().into(),readable:access.iter().any(|v|v=="read"),writable:access.iter().any(|v|v=="write"),range,options,value:None,error:None,..Default::default()});
        }
        // Only daily zero-input actions are exposed. Reset, OTA and provisioning
        // operations are intentionally not exposed as generic action buttons.
        for a in service["actions"].as_array().into_iter().flatten() {
            let key=type_key(&a["type"]);
            if !matches!(key.as_str(),"start"|"stop"|"pause"|"play"|"previous"|"next"|"start-sweep"|"stop-sweeping") || a["in"].as_array().is_some_and(|v|!v.is_empty()) {continue;}
            actions.push(Action{siid,aiid:a["iid"].as_u64().unwrap_or(0) as u32,name:chinese(&key,a["description"].as_str().unwrap_or_default()),key});
        }
    }
    (props,actions)
}
pub fn validate_property(p:&Property,value:&Value)->Result<(),String> {
    if !p.writable {return Err("这个属性只能读取".into());}
    if p.format=="bool" {if !value.is_boolean(){return Err("开关值必须为布尔值".into());}return Ok(());}
    if !p.options.is_empty() {if !p.options.iter().any(|v|v.value==*value){return Err("不支持的选项".into());}return Ok(());}
    if p.format.starts_with("uint") || p.format.starts_with("int") || matches!(p.format.as_str(),"float"|"double") {
        let v=value.as_f64().filter(|v|v.is_finite()).ok_or("请输入有效数字")?;
        if (p.format.starts_with("uint") && v<0.0) || ((p.format.starts_with("uint")||p.format.starts_with("int")) && v.fract()!=0.0) {return Err("数值类型不匹配".into());}
        if let Some(r)=&p.range { if r.len()>=3 { if v<r[0] || v>r[1] {return Err("数值超出设备范围".into());}if r[2]>0.0 {let steps=(v-r[0])/r[2];if (steps-steps.round()).abs()>1e-6{return Err("数值不符合设备步进".into());}} } }
    } else if p.format=="string" {
        if !value.is_string() || value.as_str().unwrap_or_default().len()>1024{return Err("请输入有效文本".into());}
        if crate::time_range::recognizes(p) {
            let raw=value.as_str().unwrap_or_default();
            if !crate::time_range::TimeRange::parse(raw).is_some_and(|period|period.wire()==raw) {return Err("请选择有效的开始和结束时间".into());}
        }
    } else {return Err("暂不支持写入此数据类型".into());}
    Ok(())
}
#[cfg(test)] mod tests {
    use super::*; use serde_json::json;
    #[test]
    fn schedules_require_a_valid_wire_format_and_transport_actions_require_no_inputs() {
        let p=Property {key:"enable-time-period".into(),format:"string".into(),writable:true,..Default::default()};
        assert!(validate_property(&p,&json!("22:00:00-06:00:00")).is_ok());
        for value in ["22:00-06:00","24:00:00-06:00:00","22:00:99-06:00:00"] {assert!(validate_property(&p,&json!(value)).is_err());}
        let (_,actions)=parse_spec(&json!({"services":[{"iid":3,"actions":[
            {"iid":1,"type":"urn:miot-spec-v2:action:next:1","in":[]},
            {"iid":2,"type":"urn:miot-spec-v2:action:previous:1","in":[]},
            {"iid":3,"type":"urn:miot-spec-v2:action:next:1","in":[7]},
            {"iid":4,"type":"urn:miot-spec-v2:action:reset:1","in":[]}
        ]}]}));
        assert_eq!(actions.iter().map(|a|a.key.as_str()).collect::<Vec<_>>(),vec!["next","previous"]);
    }
    #[test]
    fn standard_device_types_override_model_hints_without_inventing_controls() {
        for (urn, model, expected) in [
            ("urn:miot-spec-v2:device:mosquito-dispeller:0000A065:cokit-coq5s:1", "cokit.mosq.coq5s", "mosquito"),
            ("urn:miot-spec-v2:device:outlet:0000A002:chuangmi-212a01:3", "chuangmi.plug.212a01", "plug"),
            ("urn:miot-spec-v2:device:speaker:0000A015:xiaomi-lx06:2", "xiaomi.wifispeaker.lx06", "speaker"),
            ("urn:miot-spec-v2:device:switch:0000A01A:vendor:1", "vendor.light.switch", "switch"),
            ("", "vendor.switch.v1", "switch"),
            ("urn:other:device:outlet:1", "vendor.unknown", "device"),
            ("urn:miot-spec-v2:device:unknown:1", "vendor.unknown", "device"),
        ] { assert_eq!(kind_from_spec(urn, model), expected); }
        assert_eq!(kind_from_spec("urn:miot-spec-v2:device:outlet:1", "vendor.sensor"), "plug");
    }
    #[test] fn ranges_and_step_are_enforced() {let p=Property{writable:true,format:"uint16".into(),range:Some(vec![2700.0,6500.0,100.0]),..Default::default()};assert!(validate_property(&p,&json!(4000)).is_ok());assert!(validate_property(&p,&json!(4050)).is_err());assert!(validate_property(&p,&json!(7000)).is_err());assert!(validate_property(&p,&json!("4000")).is_err());}
    #[test] fn read_only_cannot_be_written() {assert!(validate_property(&Property{format:"bool".into(),..Default::default()},&json!(true)).is_err());}
    #[test] fn sensor_has_no_synthetic_power() {let (props,_)=parse_spec(&json!({"services":[{"iid":2,"type":"urn:miot-spec-v2:service:environment:1","properties":[{"iid":1,"type":"urn:miot-spec-v2:property:temperature:1","access":["read"],"format":"float"}]}]})); assert_eq!(props.len(),1);assert!(!props[0].writable);assert_eq!(props[0].name,"温度");}
    #[test] fn reset_action_not_exposed() {let (_,a)=parse_spec(&json!({"services":[{"iid":2,"actions":[{"iid":1,"type":"urn:miot-spec-v2:action:reset:1","in":[]}]}]}));assert!(a.is_empty());}
    #[test] fn old_preferences_load_with_no_hidden_devices() {let old=r#"{"region":"cn","theme":"system","glass":true,"autostart":false,"favorites":[],"approvedImages":[]}"#;let preferences:Preferences=serde_json::from_str(old).unwrap();assert!(preferences.hidden_devices.is_empty());assert!(preferences.native_panels);}
    #[test]
    fn public_chinese_metadata_uses_enum_index_and_preserves_values() {
        let mut properties = vec![Property { siid: 2, piid: 1, name: "Mode".into(), service: "Switch".into(),
            value: Some(json!(7)), options: vec![PropertyOption { value: json!(7), name: "Wired And Wireless".into() },
                PropertyOption { value: json!(19), name: "Wireless".into() }], ..Default::default() }];
        let mut actions = vec![Action { siid: 2, aiid: 3, name: "Start".into(), ..Default::default() }];
        apply_spec_labels(&mut properties, &mut actions, &json!({"data":{"zh_cn":{
            "service:002":"左键", "service:2:property:1":"工作模式",
            "service:002:property:001:valuelist:000":"有线与无线", "service:2:property:1:valuelist:1":"无线",
            "service:2:action:3":"启动"
        }}}));
        assert_eq!(properties[0].service,"左键");
        assert_eq!(properties[0].name,"工作模式");
        assert_eq!(properties[0].options[0].name,"有线与无线");
        assert_eq!(properties[0].options[1].name,"无线");
        assert_eq!(properties[0].options[1].value,json!(19));
        assert_eq!(properties[0].value,Some(json!(7)));
        assert_eq!(actions[0].name,"启动");
        apply_spec_labels(&mut properties,&mut actions,&json!({"data":{"en":{"service:002":"Switch"}}}));
        assert_eq!(properties[0].service,"左键");
    }

    #[test]
    fn old_glass_setting_and_system_fallback_survive_migration() {
        let old=r#"{"region":"cn","theme":"system","glass":false,"autostart":false,"favorites":[],"approvedImages":[]}"#;
        let mut preferences:Preferences=serde_json::from_str(old).unwrap();
        assert_eq!(preferences.material(true),GlassStyle::Solid);
        preferences.glass=true;
        assert_eq!(preferences.material(true),GlassStyle::Frosted);
        preferences.glass_style=GlassStyle::Glass;
        assert_eq!(preferences.material(false),GlassStyle::Solid);
        let restored:Preferences=serde_json::from_slice(&serde_json::to_vec(&preferences).unwrap()).unwrap();
        assert_eq!(restored.material(true),GlassStyle::Glass);
    }

}
