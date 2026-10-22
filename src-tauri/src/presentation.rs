//! Human-facing MIoT labels. Device values and protocol identifiers are never changed.
use crate::model::{self, Device, Property, PropertyOption};
use serde_json::Value;

pub fn property_name(property: &Property) -> String {
    match property.key.as_str() {
        "electric-power" => "功率".into(), "power-consumption" => "用电量".into(),
        "electric-current" => "电流".into(), "voltage" => "电压".into(),
        "enable-time-period" => "启用时段".into(),
        "playing-state" => "播放状态".into(), "audio-id" => "音频编号".into(),
        "on" if property.service.contains("闹钟") || property.service.eq_ignore_ascii_case("clock") => "闹钟开关".into(),
        _ => model::chinese(&property.key, &property.name),
    }
}

/// Repeated properties are identified by their service, never guessed as left/right.
pub fn property_title(device: &Device, property: &Property) -> String {
    if device.kind=="plug" && property.key=="temperature" {return "插座温度".into();}
    if property.key=="on" && (property.service_key=="indicator-light" || property.service.contains("指示灯")) {return "指示灯开关".into();}
    let name = property_name(property);
    if property.key=="mute" && (property.service.contains("麦克风") || property.service.eq_ignore_ascii_case("microphone")) {return format!("麦克风 · {name}");}
    let peers: Vec<_> = device.properties.iter().filter(|p| p.key == property.key).collect();
    if peers.len() < 2 { return name; }
    if !property.service.is_empty() && !peers.iter().any(|p| p.siid != property.siid && p.service == property.service) {
        return format!("{} · {name}", property.service);
    }
    let mut channels: Vec<_> = peers.iter().map(|p| p.siid).collect();
    channels.sort_unstable(); channels.dedup();
    let channel = channels.iter().position(|id| *id == property.siid).unwrap_or(0) + 1;
    format!("通道 {channel} · {name}")
}

pub fn option_label(label: &str) -> String {
    if model::has_chinese(label) { return label.trim().into(); }
    let key = label.trim().to_ascii_lowercase().replace([' ', '_'], "-");
    match key.as_str() {
        "wired-and-wireless" | "wired-wireless" => "有线与无线",
        "wireless" | "wireless-only" => "无线",
        "wired" | "wired-only" => "有线",
        "reverse" | "reversed" | "inverse" => "反向",
        "forward" | "positive" => "正向",
        "always-on" | "always-on-mode" => "常亮",
        "always-off" | "always-off-mode" => "常灭",
        "momentary" | "inching" | "jog" => "点动",
        "toggle" | "latching" => "切换",
        "no-beep" | "without-beep" => "静音",
        "beep" | "with-beep" => "提示音",
        "click" | "single-click" => "单击", "double-click" => "双击", "long-press" => "长按",
        "idle" | "standby" => "待机", "working" | "running" => "运行中",
        "playing" | "play" => "播放中", "pause" | "paused" => "已暂停", "stop" | "stopped" => "已停止",
        "charging" => "充电中", "completed" | "finished" => "已完成",
        "open" | "opened" => "打开", "close" | "closed" => "关闭",
        "none" | "no-fault" | "no-faults" => "正常",
        "normal" | "standard" => "标准",
        "off" | "false" | "disabled" => "关闭",
        "on" | "true" | "enabled" => "开启",
        "auto" | "automatic" => "自动",
        "manual" => "手动",
        "silent" | "quiet" | "sleep" => "睡眠",
        "night" | "night-light" => "夜灯",
        "reading" | "read" => "阅读",
        "movie" => "观影",
        "previous" | "last" | "memory" | "last-state" | "restore" | "restore-previous-state" => "记忆上次状态",
        "low" => "低档",
        "medium" => "中档",
        "high" => "高档",
        "natural" | "nature" => "自然风",
        "cool" => "制冷",
        "heat" => "制热",
        "dry" => "除湿",
        "fan" => "送风",
        "eco" | "energy-saving" => "节能",
        "favorite" | "favourite" => "喜好",
        "single-cycle" => "单曲循环", "list-cycle" | "loop" => "列表循环", "random" | "shuffle" => "随机播放",
        _ => "未提供中文",
    }.into()
}

pub fn option_text(property: &Property, option: &PropertyOption) -> String {
    if setting_group(property)==1 {
        match option.name.trim().to_ascii_lowercase().replace([' ','_'],"-").as_str() {
            "follow-switch"|"follow-relay"|"same-as-switch"|"on-when-on"=>return "跟随开关".into(),
            "inverse-switch"|"opposite-to-switch"|"on-when-off"=>return "与开关相反".into(),
            _=>{}
        }
    }
    let label = option_label(&option.name);
    if label != "未提供中文" { return label; }
    let index = property.options.iter().position(|item| item.value == option.value).unwrap_or(0) + 1;
    format!("厂商选项 {index}")
}

pub fn option_tooltip(property: &Property, option: &PropertyOption) -> String {
    if option_text(property,option).starts_with("厂商选项 ") { format!("含义尚未确认，请对照米家设置。厂商原文：{}；设备值：{}", option.name,option.value) }
    else { option_text(property, option) }
}

fn unit(property: &Property) -> &str {
    let unit = property.unit.trim();
    match unit.to_ascii_lowercase().as_str() {
        "percentage" | "percent" | "%" => "%",
        "celsius" | "℃" | "°c" => "°C",
        "kelvin" | "k" => "K",
        "fahrenheit" | "℉" | "°f" => "°F",
        "lux" => "lx",
        "seconds" | "second" => "秒",
        "minutes" | "minute" => "分钟",
        "hours" | "hour" => "小时",
        "watt" | "watts" => "W",
        "kilowatt-hour" | "kilowatt-hours" | "kwh" => "kWh",
        "watt-hour" | "watt-hours" | "wh" => "Wh",
        "milliwatt-hour" | "mwh" => "mWh",
        "milliampere" | "milliamperes" | "ma" => "mA",
        "volt" | "volts" | "v" => "V",
        "ampere" | "amperes" | "a" => "A",
        "micrograms-per-cubic-meter" | "μg/m3" | "ug/m3" => "μg/m³",
        "none" | "" => match property.key.as_str() {
            "brightness" | "battery-level" | "relative-humidity" => "%",
            "color-temperature" => "K",
            _ => "",
        },
        _ => unit,
    }
}

pub fn value_text(property: &Property) -> String {
    if property.error.is_some() { return "读取失败".into(); }
    if !property.readable && property.writable && property.value.is_none() { return "仅支持设置".into(); }
    let Some(value) = property.value.as_ref() else { return "暂无读数".into(); };
    if let Some(option) = property.options.iter().find(|option| option.value == *value) {
        return option_text(property, option);
    }
    if property.key=="playing-state" && value.is_number() {return format!("未知状态（{value}）");}
    let raw = match value {
        Value::Bool(value) => return if *value { "开启" } else { "关闭" }.into(),
        Value::Number(number) => match number.as_f64() {
            Some(value) if value.fract() == 0.0 => format!("{value:.0}"),
            Some(value) => format!("{value:.1}"),
            None => "暂无读数".into(),
        },
        Value::String(value) if value.trim().is_empty() => return if property.key=="audio-id" {"暂无音频"} else {"暂无内容"}.into(),
        Value::String(value) if matches!(property.key.as_str(),"status"|"mode"|"playing-state") => option_label(value),
        Value::String(value) => value.clone(),
        _ => return "暂无读数".into(),
    };
    match unit(property) {
        "" => raw,
        "%" | "°C" | "°F" => format!("{raw}{}", unit(property)),
        suffix => format!("{raw} {suffix}"),
    }
}

pub fn reported_text(property:&Property,text:&str,now:u64)->String {
    if property.error.is_some() {return "状态未获取".into();}
    if property.value.is_none() {return text.into();}
    match property.source {
        crate::model::StateSource::Cloud=>format!("{text} · 上报"),
        crate::model::StateSource::Device if property.checked_at.is_some_and(|at|now.saturating_sub(at)<=45_000)=>text.into(),
        crate::model::StateSource::Device=>format!("上次{text}"),
        crate::model::StateSource::Unknown=>format!("{text} · 待确认"),
    }
}

pub struct Reading {
    pub value: String,
    pub unit: String,
    pub note: Option<&'static str>,
    pub valid: bool,
}

impl Reading {
    pub fn text(&self) -> String {
        if !self.valid || self.unit.is_empty() { return self.value.clone(); }
        match self.unit.as_str() {
            "%" | "°C" | "°F" => format!("{}{}", self.value, self.unit),
            _ => format!("{} {}", self.value, self.unit),
        }
    }
}

fn number_text(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').into() } else { text }
}

pub fn electrical_order(property: &Property) -> u8 {
    match property.key.as_str() {
        "electric-power" => 0, "power-consumption" => 1, "electric-current" => 2, "voltage" => 3, _ => 4,
    }
}

/// Display-only correction for this exact public SPEC family. Protocol values stay intact.
/// Energy is reported in mWh, not kWh and not an app-calculated hourly estimate.
pub fn reading(device: &Device, property: &Property) -> Reading {
    let known_plug = device.model == "chuangmi.plug.212a01" && matches!(device.spec_urn.as_str(),
        "urn:miot-spec-v2:device:outlet:0000A002:chuangmi-212a01:1" |
        "urn:miot-spec-v2:device:outlet:0000A002:chuangmi-212a01:2" |
        "urn:miot-spec-v2:device:outlet:0000A002:chuangmi-212a01:3");
    let correction = if known_plug && property.siid == 5 {
        match (property.piid, property.key.as_str()) {
            (1, "power-consumption") => Some((0.001, "Wh", 3)),
            (2, "electric-current") => Some((1.0, "mA", 0)),
            (3, "voltage") => Some((1.0, "V", 1)),
            (6, "electric-power") => Some((0.01, "W", 2)),
            _ => None,
        }
    } else { None };
    let suffix = correction.map(|(_, unit, _)| unit).unwrap_or_else(|| unit(property));
    let valid = property.error.is_none() && property.value.is_some();
    let numeric = property.value.as_ref().filter(|_| property.options.is_empty()).and_then(Value::as_f64).filter(|value| value.is_finite());
    let value = if !valid { value_text(property) } else if let Some(number) = numeric {
        let (ratio, _, decimals) = correction.unwrap_or((1.0, suffix, 3));
        number_text(number * ratio, decimals)
    } else { value_text(property) };
    let note = if electrical_order(property) < 4 && suffix.is_empty() { Some("原始读数 · 单位待确认") }
        else if known_plug && property.key == "power-consumption" { Some("上报电量 · 周期以设备为准") } else { None };
    Reading { value, unit: if numeric.is_some() { suffix.into() } else { String::new() }, note, valid }
}

pub fn advanced(property: &Property) -> bool {
    matches!(property.key.as_str(), "default-power-on-state" | "power-on-state" |
        "power-loss-memory" | "indicator-light" | "alarm" | "fault")
}

/// A small native profile for climate controls; all remaining SPEC features stay available.
pub fn advanced_for(device: &Device, property: &Property) -> bool {
    if setting_group(property)<3 { return true; }
    if advanced(property) { return true; }
    if device.kind == "speaker" { return !speaker_daily(property); }
    if matches!(device.kind.as_str(),"plug"|"switch") && property.writable && property.key!="on" && property.key!="countdown" {return true;}
    device.kind == "climate" && !matches!(property.key.as_str(),
        "on" | "mode" | "target-temperature" | "fan-level" | "horizontal-swing" |
        "vertical-swing" | "temperature" | "relative-humidity")
}

pub fn device_category(device:&Device)-> &'static str {
    match device.kind.as_str() {"plug"=>"插座开关","switch"=>"智能开关","light"=>"灯具","sensor"=>"传感器",
        "speaker"=>"音箱","climate"=>"空调","fan"=>"风扇","purifier"=>"空气净化器","humidifier"=>"加湿器",
        "vacuum"=>"扫地机","camera"=>"摄像头","mosquito"=>"驱蚊器",_=>"智能设备"}
}

pub fn setting_group(property:&Property)->u8 {
    let name=format!("{} {} {} {}",property.key,property.service_key,property.name,property.service).to_ascii_lowercase();
    if ["indicator","指示灯"].iter().any(|key|name.contains(key)) {1}
    else if ["inching","momentary","jog","点动","按键"].iter().any(|key|name.contains(key)) {0}
    else if matches!(property.key.as_str(),"default-power-on-state"|"power-on-state"|"power-loss-memory") {2}
    else {3}
}

pub fn property_help(property:&Property)->Option<&'static str> {
    let name=format!("{} {}",property.key,property.name).to_ascii_lowercase();
    if setting_group(property)==1 {
        Some("设置指示灯的显示方式，不代表插座是否通电；未知选项请对照米家设置。")
    } else if (name.contains("点动") || name.contains("inching") || name.contains("momentary")) &&
        (name.contains("默认") || name.contains("default")) {
        Some("配置点动模式的默认开关状态，不代表当前是否通电。")
    } else if (name.contains("点动") || name.contains("inching") || name.contains("momentary")) &&
        (name.contains("时长") || name.contains("duration")) {
        Some("点动触发后的持续时间，不是每日定时开关计划。")
    } else if setting_group(property)==2 {
        Some("决定断电后重新通电的行为，不代表设备当前开关状态。")
    } else { None }
}

pub fn speaker_daily(property:&Property)->bool {
    matches!(property.key.as_str(),"volume"|"playing-state") || (property.key=="mute" &&
        (property.service=="扬声器" || property.service.contains("音箱") || property.service.eq_ignore_ascii_case("speaker")))
}

pub fn speaker_status(device:&Device)->String {
    let Some(property)=device.properties.iter().find(|p|p.key=="playing-state") else {return "播放状态暂不可读".into();};
    let text=value_text(property);
    if property.error.is_some() || property.value.is_none() { return text; }
    if property.source==crate::model::StateSource::Cloud {format!("{text} · 云端上报")}
    else if property.checked_at.is_some_and(|at|crate::now_ms().saturating_sub(at)>=15_000) {format!("上次回读：{text}")}
    else {text}
}

/// Snap against the SPEC's minimum and step, including ranges whose minimum isn't zero.
pub fn snap_value(value: f64, range: &[f64], integer: bool) -> f64 {
    let mut value = value.clamp(range[0], range[1]);
    if let Some(step) = range.get(2).filter(|step| step.is_finite() && **step > 0.0) {
        value = range[0] + ((value - range[0]) / step).round() * step;
    }
    if integer { value = value.round(); }
    value.clamp(range[0], range[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PropertyOption;
    use serde_json::json;

    #[test]
    fn card_labels_qualify_stale_and_cached_state_instead_of_claiming_current_state() {
        let mut p=Property{value:Some(json!(true)),source:crate::model::StateSource::Device,checked_at:Some(1000),..Default::default()};
        assert_eq!(reported_text(&p,"已开启",1000),"已开启");
        assert_eq!(reported_text(&p,"已开启",47000),"上次已开启");
        p.source=crate::model::StateSource::Cloud;assert_eq!(reported_text(&p,"已开启",1000),"已开启 · 上报");
        p.error=Some("missing".into());assert_eq!(reported_text(&p,"已开启",1000),"状态未获取");
    }

    #[test]
    fn cached_playback_and_write_only_properties_are_identified() {
        let mut device=Device{properties:vec![Property{key:"playing-state".into(),value:Some(json!("Stopped")),source:crate::model::StateSource::Cloud,..Default::default()}],..Default::default()};
        assert_eq!(speaker_status(&device),"已停止 · 云端上报");
        device.properties[0].error=Some("missing row".into());
        assert_eq!(speaker_status(&device),"读取失败");
        assert_eq!(value_text(&Property{writable:true,..Default::default()}),"仅支持设置");
    }

    #[test]
    fn speaker_states_use_spec_labels_and_empty_audio_has_a_clear_placeholder() {
        let mut property=Property {key:"playing-state".into(),value:Some(json!(2)),options:vec![PropertyOption{value:json!(7),name:"Playing".into()},PropertyOption{value:json!(2),name:"Pause".into()},PropertyOption{value:json!(4),name:"Stop".into()}],..Default::default()};
        assert_eq!(value_text(&property),"已暂停");
        property.value=Some(json!(7));assert_eq!(value_text(&property),"播放中");
        property.value=Some(json!(4));assert_eq!(value_text(&property),"已停止");
        property.value=Some(json!(99));assert_eq!(value_text(&property),"未知状态（99）");
        assert_eq!(value_text(&Property{key:"audio-id".into(),value:Some(json!("")),..Default::default()}),"暂无音频");
        assert_eq!(speaker_status(&Device::default()),"播放状态暂不可读");
        assert!(advanced_for(&Device{kind:"speaker".into(),..Default::default()},&Property{key:"enable-time-period".into(),..Default::default()}));
        assert!(!speaker_daily(&Property{key:"mute".into(),service:"麦克风".into(),..Default::default()}));
    }

    #[test]
    fn climate_profile_keeps_daily_controls_and_moves_vendor_features_to_more() {
        let device = Device { kind:"climate".into(), ..Default::default() };
        for key in ["on","mode","target-temperature","fan-level","horizontal-swing","temperature"] {
            assert!(!advanced_for(&device,&Property { key:key.into(), ..Default::default() }));
        }
        for key in ["sleep-mode","favorite-type","target-humidity","indicator-light","fault"] {
            assert!(advanced_for(&device,&Property { key:key.into(), ..Default::default() }));
        }
        assert!(!advanced_for(&Device::default(),&Property { key:"target-humidity".into(), ..Default::default() }));
        assert_eq!(option_label("Beep"),"提示音");
        assert_eq!(option_label("No Beep"),"静音");
    }

    #[test]
    fn official_units_are_symbols_without_changing_values() {
        for (unit, value, expected) in [("percentage", json!(100), "100%"), ("celsius", json!(23.4), "23.4°C"), ("kelvin", json!(3970), "3970 K")] {
            let property = Property { unit: unit.into(), value: Some(value.clone()), ..Default::default() };
            assert_eq!(value_text(&property), expected);
            assert_eq!(property.value, Some(value));
        }
    }

    #[test]
    fn enum_values_show_their_labels_instead_of_protocol_numbers() {
        let property = Property { value: Some(json!(0)), options: vec![PropertyOption { value: json!(0), name: "Normal".into() }], ..Default::default() };
        assert_eq!(value_text(&property), "标准");
        assert_eq!(reading(&Device::default(), &property).text(), "标准");
        assert_eq!(option_label("Off"), "关闭");
        assert_eq!(option_label("vendor-specific-mode"), "未提供中文");
    }

    #[test]
    fn nonzero_minimum_is_preserved_when_snapping() {
        assert_eq!(snap_value(4055.0, &[2710.0, 6510.0, 100.0], true), 4010.0);
        assert_eq!(snap_value(0.0, &[1.0, 100.0, 1.0], true), 1.0);
    }

    #[test]
    fn lowercase_power_options_are_not_mistaken_for_property_names() {
        let (mut properties, _) = model::parse_spec(&json!({"services":[{"iid":2,"properties":[{
            "iid":3,"type":"urn:miot-spec-v2:property:default-power-on-state:1",
            "format":"uint8","access":["read","write"],
            "value-list":[{"value":1,"description":"on"}]
        }]}]}));
        properties[0].value = Some(json!(1));
        assert_eq!(property_name(&properties[0]), "来电状态");
        assert_eq!(value_text(&properties[0]), "开启");
        assert_eq!(properties[0].value, Some(json!(1)));
    }
    #[test]
    fn exact_plug_units_are_display_only_and_energy_is_not_hourly_extrapolated() {
        let device=Device { model:"chuangmi.plug.212a01".into(), spec_urn:"urn:miot-spec-v2:device:outlet:0000A002:chuangmi-212a01:3".into(), ..Default::default() };
        for (piid,key,raw,expected) in [(1,"power-consumption",29715,"29.715 Wh"),(2,"electric-current",5,"5 mA"),
            (3,"voltage",220,"220 V"),(6,"electric-power",1534,"15.34 W")] {
            let property=Property {siid:5,piid,key:key.into(),value:Some(json!(raw)),..Default::default()};
            assert_eq!(reading(&device,&property).text(),expected);
            assert_eq!(property.value,Some(json!(raw)));
        }
        let unknown=Device {model:device.model.clone(),spec_urn:"future-spec".into(),..Default::default()};
        let mut property=Property {siid:5,piid:2,key:"electric-current".into(),value:Some(json!(5)),..Default::default()};
        assert_eq!(reading(&unknown,&property).text(),"5");
        assert_eq!(reading(&unknown,&property).note,Some("原始读数 · 单位待确认"));
        property.error=Some("读取失败".into());
        let failed=reading(&device,&property);
        assert!(!failed.valid);
        assert_eq!(failed.text(),"读取失败");
        assert_eq!(failed.unit,"mA");
    }

    #[test]
    fn repeated_modes_use_service_names_without_guessing_channel_positions() {
        let mut device=Device::default();
        for (siid,service) in [(2,"左键"),(5,"中键"),(8,"右键")] {
            device.properties.push(Property {siid,piid:1,key:"mode".into(),service:service.into(),..Default::default()});
        }
        assert_eq!(property_title(&device,&device.properties[1]),"中键 · 模式");
        device.properties.iter_mut().for_each(|property|property.service="开关".into());
        assert_eq!(property_title(&device,&device.properties[1]),"通道 2 · 模式");
        assert_eq!(option_label("Wired And Wireless"),"有线与无线");
        assert_eq!(option_label("Wireless"),"无线");
        let property=Property {options:vec![PropertyOption{value:json!(9),name:"Unrecognized vendor choice".into()}],..Default::default()};
        assert_eq!(option_text(&property,&property.options[0]),"厂商选项 1");
        assert!(option_tooltip(&property,&property.options[0]).contains("尚未确认"));
        assert_eq!(property.options[0].value,json!(9));
    }

    #[test]
    fn plug_configuration_is_separate_from_daily_controls_with_plain_help() {
        let device=Device{kind:"plug".into(),name:"热水器".into(),..Default::default()};
        let state=Property{key:"inching-default-state".into(),name:"点动模式默认状态".into(),writable:true,..Default::default()};
        let duration=Property{key:"inching-duration".into(),name:"点动模式按下持续时长".into(),writable:true,..Default::default()};
        let indicator=Property{key:"on".into(),service_key:"indicator-light".into(),writable:true,..Default::default()};
        assert!(advanced_for(&device,&state));assert!(advanced_for(&device,&duration));assert!(advanced_for(&device,&indicator));
        assert!(property_help(&state).unwrap().contains("不代表当前"));
        assert!(property_help(&duration).unwrap().contains("不是每日定时"));
        assert_eq!(property_title(&device,&indicator),"指示灯开关");
        assert!(!advanced_for(&device,&Property{key:"electric-power".into(),readable:true,..Default::default()}));
        assert_eq!(device_category(&device),"插座开关");
        assert_eq!(property_title(&device,&Property{key:"temperature".into(),..Default::default()}),"插座温度");
    }

    #[test]
    fn indicator_translations_do_not_guess_unknown_values_or_change_motor_meaning() {
        let p=Property{service_key:"indicator-light".into(),..Default::default()};
        let option=PropertyOption{value:json!(19),name:"Follow Switch".into()};
        assert_eq!(option_text(&p,&option),"跟随开关");
        assert_eq!(option_tooltip(&p,&option),"跟随开关");
        assert_eq!(option.value,json!(19));
        assert_eq!(option_label("Reverse"),"反向");
    }

}
