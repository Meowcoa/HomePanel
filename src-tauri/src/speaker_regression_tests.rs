//! Protocol regressions use only synthetic identities and loopback transport.
use super::*;
use std::io::BufRead;

fn device() -> Device {
    Device {
        id: "regression-miot".into(),
        model: "xiaomi.wifispeaker.different_namespace".into(),
        kind: "speaker".into(),
        spec_status: "ready".into(),
        spec_urn: "urn:miot-spec-v2:device:speaker:0000A015:regression:1".into(),
        online: true,
        ..Default::default()
    }
}

fn row() -> Value {
    json!({
        "miotDID":"regression-miot", "deviceID":"regression-mina",
        "hardware":"L05C", "presence":"online",
        "capabilities":{"ai_protocol_3_0":1,"night_mode":1}
    })
}

#[test]
fn connecting_xiaoai_does_not_take_over_third_party_speakers() {
    let mut speaker=device();assert!(eligible(&speaker));
    speaker.model="thirdparty.speaker.example".into();assert!(!eligible(&speaker));
    speaker.model="xiaomi.wifispeaker.l05c".into();speaker.kind="plug".into();assert!(!eligible(&speaker));
}

fn target(row: Value) -> Target {
    select_target(&json!({"deviceList":[row],"groupInfo":[]}), "regression-miot").unwrap()
}

fn state(device: &Device) -> Snapshot {
    let target = target(row());
    Snapshot {
        at: now_ms(), player_at: now_ms(), sleep_at: now_ms(), bluetooth_at: now_ms(),
        status: Some(1), volume: Some(35), loop_type: Some(1),
        duration_ms: Some(120_000), position_ms: Some(30_000),
        sleeping: Some(false), bluetooth: Some(false), online: Some(true),
        adapter: PhoneAdapter::resolve(device, &target),
        ..Default::default()
    }
}

#[test]
fn v3_cookie_contract_uses_positive_integer_capability_not_a_top_level_flag() {
    for (value, expected) in [
        (Value::Null, false), (json!(0), false), (json!(-1), false),
        (json!(true), false), (json!("1"), false), (json!(1.5), false),
        (json!(1), true), (json!(2), true),
    ] {
        let mut source = row();
        source["capabilities"]["ai_protocol_3_0"] = value;
        source["isV3Protocol"] = json!(!expected);
        assert_eq!(target(source).is_v3, expected);
    }
    let mut source = row();
    source["capabilities"] = json!({});
    source["isV3Protocol"] = json!(true);
    assert!(!target(source).is_v3);
}

#[test]
fn known_hardware_is_case_insensitive_and_does_not_guess_a_miot_model_alias() {
    let device = device();
    for hardware in ["L05C", "l05c", " L05c ", "l09b", "LX01"] {
        let mut source = row();
        source["hardware"] = json!(hardware);
        let adapter = PhoneAdapter::resolve(&device, &target(source)).unwrap();
        assert!(adapter.validate(&device, &Command::Loop(3)).is_ok());
    }
    let mut unknown = row();
    unknown["hardware"] = json!("UNKNOWN");
    let adapter=PhoneAdapter::resolve(&device, &target(unknown)).unwrap();
    for command in [Command::Loop(3),Command::Volume(40),Command::Seek(60_000),Command::Play("pause")] {
        assert!(adapter.validate(&device,&command).is_ok());
    }
    assert!(adapter.validate(&device,&Command::Bluetooth(true)).is_err());
    for (model, kind, spec_status, urn) in [
        ("vendor.wifispeaker.l05c", "speaker", "ready", device.spec_urn.as_str()),
        (device.model.as_str(), "plug", "ready", device.spec_urn.as_str()),
        (device.model.as_str(), "speaker", "unloaded", device.spec_urn.as_str()),
        (device.model.as_str(), "speaker", "ready", "urn:miot-spec-v2:device:outlet:1"),
    ] {
        let mut other = device.clone();
        other.model = model.into(); other.kind = kind.into();
        other.spec_status = spec_status.into(); other.spec_urn = urn.into();
        assert!(PhoneAdapter::resolve(&other, &target(row())).is_none());
    }
}

#[test]
fn mina_presence_handles_case_and_explicit_offline_overrides_the_other_channel() {
    for (presence, expected) in [
        ("online", Some(true)), (" OnLiNe ", Some(true)),
        ("OFFLINE", Some(false)), (" offline ", Some(false)),
        ("unknown", None), ("", None),
    ] {
        let mut source = row(); source["presence"] = json!(presence);
        assert_eq!(target(source).online, expected);
    }
    let mut device = device();
    let mut snapshot = state(&device);
    device.online = false;
    assert!(Command::Volume(40).validate(&device, &snapshot).is_ok());
    device.online = true; snapshot.online = Some(false);
    assert!(Command::Volume(40).validate(&device, &snapshot).is_err());
    snapshot.online = None;
    assert!(snapshot.available(&device));
    device.online = false;
    assert!(!snapshot.available(&device));
}

#[test]
fn field_freshness_does_not_make_unrelated_failed_controls_unavailable() {
    let device = device();
    let mut snapshot = state(&device);
    snapshot.player_at = now_ms().saturating_sub(16_000);
    snapshot.error = Some("synthetic player failure".into());
    assert!(!snapshot.current());
    assert!(Command::Volume(40).validate(&device, &snapshot).is_err());
    assert!(Command::Bluetooth(true).validate(&device, &snapshot).is_ok());
    assert!(Command::Sleep(true).validate(&device, &snapshot).is_ok());
    snapshot.bluetooth_at = now_ms().saturating_sub(16_000);
    assert!(Command::Bluetooth(true).validate(&device, &snapshot).is_err());
    assert!(Command::Sleep(true).validate(&device, &snapshot).is_ok());
    snapshot.sleep_error = Some("synthetic sleep failure".into());
    assert!(Command::Sleep(true).validate(&device, &snapshot).is_err());
}

#[test]
fn failed_read_retains_display_values_without_claiming_old_values_are_fresh() {
    let device = device();
    let mut previous = state(&device);
    previous.title = "synthetic last song".into();
    previous.track_audio = Some(101); previous.track_global = Some(202);
    let failed = Snapshot::read_failure(&previous, "synthetic network failure".into());
    assert_eq!(failed.status, previous.status); assert_eq!(failed.volume, previous.volume);
    assert_eq!(failed.title, previous.title); assert_eq!(failed.position_ms, previous.position_ms);
    assert_eq!(failed.track_audio, Some(101)); assert_eq!(failed.track_global, Some(202));
    assert_eq!(failed.sleeping, previous.sleeping); assert_eq!(failed.bluetooth, previous.bluetooth);
    assert!(!failed.current()); assert!(!failed.sleep_current()); assert!(!failed.bluetooth_current());
    let pending = previous.pending_refresh();
    assert_eq!(pending.title, previous.title);
    assert!(!pending.current()); assert!(!pending.sleep_current()); assert!(!pending.bluetooth_current());
}

#[test]
fn track_confirmation_uses_same_source_ids_even_when_titles_repeat() {
    let before = Snapshot { track_audio: Some(101), title: "same title".into(), ..Default::default() };
    let changed = Snapshot { track_audio: Some(102), title: "same title".into(), ..Default::default() };
    for command in [Command::Play("next"), Command::Play("prev")] {
        assert!(command.confirmed(&before, &changed));
        let changed_source = Snapshot { track_global: Some(102), title: "new title".into(), ..Default::default() };
        assert!(!command.confirmed(&before, &changed_source));
        let same_audio = Snapshot { track_audio: Some(101), title: "new title".into(), ..Default::default() };
        assert!(!command.confirmed(&before, &same_audio));
        let before_global = Snapshot { track_global: Some(501), title: "same title".into(), ..Default::default() };
        let after_global = Snapshot { track_global: Some(502), title: "same title".into(), ..Default::default() };
        assert!(command.confirmed(&before_global, &after_global));
    }
}

#[test]
fn player_ids_keep_audio_and_serialized_global_id_separate_and_validate_long_values() {
    let parsed = parse_player(&json!({"status":1,"play_song_detail":{"audio_id":101,"global_id":"202","musicID":999}})).unwrap();
    assert_eq!(parsed.track_audio, Some(101)); assert_eq!(parsed.track_global, Some(202));
    for invalid in [Value::Null, json!(0), json!(-1), json!(true), json!(1.5), json!("bad"), json!("9223372036854775808")] {
        let parsed = parse_player(&json!({"status":1,"play_song_detail":{"audio_id":invalid,"global_id":invalid}})).unwrap();
        assert_eq!(parsed.track_audio, None); assert_eq!(parsed.track_global, None);
    }
}

#[test]
fn only_explicit_play_of_an_empty_idle_queue_uses_the_fixed_start_music_payload() {
    let device = device();
    let mut idle = state(&device); idle.status = Some(0);
    assert_eq!(idle.play_label(), "播放音乐");
    assert_eq!(Command::Play("play").wire_for(&idle).unwrap(), (
        "mibrain", "ai_service", json!({"nlp":1,"tts":0,"nlp_execute":1,"nlp_text":"播放音乐","tts_play":0,"caller":"app_android"})
    ));
    for action in ["pause", "next", "prev", "stop"] {
        assert_eq!(Command::Play(action).wire_for(&idle).unwrap().0, "mediaplayer");
    }
    for (status, track) in [(2, None), (0, Some(101)), (1, None)] {
        let mut resume = idle.clone(); resume.status = Some(status); resume.track_audio = track;
        assert_eq!(Command::Play("play").wire_for(&resume).unwrap(), (
            "mediaplayer", "player_play_operation", json!({"action":"play","media":"app_android"})
        ));
    }
    idle.adapter = None;
    assert_eq!(Command::Play("play").wire_for(&idle).unwrap().0, "mediaplayer");
}

#[derive(Clone, Copy)]
enum HttpScenario { PlayerFailureBluetooth, IdlePlay, ResumePlay }

fn read_line_before(reader: &mut std::io::BufReader<std::net::TcpStream>, line: &mut String, deadline: std::time::Instant) {
    loop {
        match reader.read_line(line) {
            Ok(0) => panic!("speaker HTTP fixture request ended before its headers"),
            Ok(_) => return,
            Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
                && std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => panic!("speaker HTTP fixture could not read request headers: {error}"),
        }
    }
}

fn exercise_http(scenario: HttpScenario) -> Snapshot {
    use std::{io::{BufReader, Read, Write}, net::TcpListener, time::Instant};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut requests = 0; let mut writes = 0;
        while requests < 6 && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5)); continue;
                },
                Err(error) => panic!("fixture accept: {error}"),
            };
            stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let request_deadline = Instant::now() + Duration::from_secs(8);
            let mut first = String::new(); read_line_before(&mut reader,&mut first,request_deadline);
            let mut length = 0; let mut cookie = String::new();
            loop {
                let mut line = String::new(); read_line_before(&mut reader,&mut line,request_deadline);
                if line == "\r\n" { break; } assert!(!line.is_empty());
                let (key, value) = line.split_once(':').unwrap();
                if key.eq_ignore_ascii_case("content-length") { length = value.trim().parse::<usize>().unwrap(); }
                if key.eq_ignore_ascii_case("cookie") { cookie = value.trim().into(); }
            }
            assert!(length < 16_384);
            let mut body = vec![0; length]; reader.read_exact(&mut body).unwrap();
            requests += 1;
            let data = if first.starts_with("GET /admin/v3/device_list?") {
                assert_eq!(requests, 1);
                let mut source = row(); source["presence"] = json!("ONLINE");
                json!({"deviceList":[source],"groupInfo":[]})
            } else {
                assert!(first.starts_with("POST /remote/ubus?"));
                assert!(cookie.contains("isV3Protocol=1"));
                assert!(!cookie.contains("isV3Protocol=true"));
                let fields: HashMap<String, String> = url::form_urlencoded::parse(&body).into_owned().collect();
                assert_eq!(fields["deviceId"], "regression-mina");
                let message: Value = serde_json::from_str(&fields["message"]).unwrap();
                match (fields["path"].as_str(), fields["method"].as_str()) {
                    ("mediaplayer", "player_get_play_status") => {
                        assert_eq!(message, json!({}));
                        if matches!(scenario, HttpScenario::PlayerFailureBluetooth) {
                            json!({"code":-1})
                        } else {
                            let status = if writes > 0 { 1 } else if matches!(scenario, HttpScenario::ResumePlay) { 2 } else { 0 };
                            let detail = if matches!(scenario, HttpScenario::ResumePlay) {
                                json!({"title":"fixture resumed song","audio_id":101})
                            } else { json!({}) };
                            json!({"code":0,"info":json!({"status":status,"volume":35,"loop_type":1,"play_song_detail":detail}).to_string()})
                        }
                    },
                    ("nightmode", "status") => json!({"code":0,"info":"{\"sleep_mode\":\"normal\"}"}),
                    ("mibt", "status") => {
                        assert_eq!(message, json!({"btmode":"classic"}));
                        json!({"code":0,"info":json!({"connect":if writes>0{1}else{0}}).to_string()})
                    },
                    (path, method) => {
                        match scenario {
                            HttpScenario::PlayerFailureBluetooth => {
                                assert_eq!((path, method), ("mibt", "enable"));
                                assert_eq!(message, json!({"btmode":"classic","connect":1,"discover":0}));
                            },
                            HttpScenario::IdlePlay => {
                                assert_eq!((path, method), ("mibrain", "ai_service"));
                                assert_eq!(message, json!({"nlp":1,"tts":0,"nlp_execute":1,"nlp_text":"播放音乐","tts_play":0,"caller":"app_android"}));
                            },
                            HttpScenario::ResumePlay => {
                                assert_eq!((path, method), ("mediaplayer", "player_play_operation"));
                                assert_eq!(message, json!({"action":"play","media":"app_android"}));
                            },
                        }
                        writes += 1; assert_eq!(writes, 1, "a write must never be retried");
                        json!({"code":0,"info":"{}"})
                    },
                }
            };
            let response = serde_json::to_vec(&json!({"code":0,"data":data})).unwrap();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len()).unwrap();
            stream.write_all(&response).unwrap();
        }
        assert_eq!(requests, 6); assert_eq!(writes, 1);
    });
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let root = std::env::temp_dir().join(format!("homepanel-speaker-regression-{}-{}", std::process::id(), address.port()));
    assert!(!root.exists());
    let main = Cloud::new(root.join("main"), root.join("cache")).unwrap();
    let mut service = Service::new(&root).unwrap();
    service.test_base = format!("http://{address}");
    service.client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none()).build().unwrap();
    let synthetic = crate::storage::Session {
        user_id: "regression-account".into(), ssecurity: "synthetic-only".into(),
        service_token: "synthetic-only".into(), agent: "test".into(),
    };
    let result = runtime.block_on(async {
        *main.session.write().await = Some(synthetic.clone());
        *service.auth.session.write().await = Some(synthetic);
        let mut selected = device(); selected.online = false;
        let command = if matches!(scenario, HttpScenario::PlayerFailureBluetooth) { Command::Bluetooth(true) } else { Command::Play("play") };
        service.control_loaded(&main, &selected, command).await
    });
    server.join().unwrap();
    assert!(!root.exists(), "synthetic sessions must never be persisted");
    result.unwrap()
}

#[test]
fn failed_player_does_not_block_an_independently_read_and_confirmed_bluetooth_write() {
    let snapshot = exercise_http(HttpScenario::PlayerFailureBluetooth);
    assert!(snapshot.error.is_some()); assert!(!snapshot.current());
    assert_eq!(snapshot.bluetooth, Some(true)); assert!(snapshot.bluetooth_current());
}

#[test]
fn an_explicit_idle_play_click_starts_music_once_and_requires_player_readback() {
    assert_eq!(exercise_http(HttpScenario::IdlePlay).status, Some(1));
}

#[test]
fn paused_play_resumes_the_existing_queue_without_an_nlp_start_music_command() {
    let snapshot = exercise_http(HttpScenario::ResumePlay);
    assert_eq!(snapshot.status, Some(1)); assert_eq!(snapshot.track_audio, Some(101));
}
