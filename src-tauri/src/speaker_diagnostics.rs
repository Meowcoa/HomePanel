//! Debug-only protocol checks. The explicitly selected loop probe temporarily
//! writes and restores selected devices; the ordinary diagnostic remains read-only.
//! All output values are closed enums or booleans; private strings cannot enter.
use crate::{cloud::Cloud, model::{Device, Home}, speaker_service::Service};
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Environment, Runtime, MainSession, SpeakerSession, Identity, DeviceList,
    Home, Target, Spec, Player, Sleep, Bluetooth, Preflight, LoopChange, LoopRestore, Overall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    Ok, Unavailable, Rejected, InvalidResponse, Timeout, Unauthorized, Forbidden,
    Network, SessionChanged, Ambiguous, ResponseTooLarge, Unconfirmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Flag {
    SessionPresent, IdentityMatches, DeviceListPresent, DeviceListParsed,
    TargetMatched, TargetFieldsValid, EnvelopeParsed, InfoPresent, InfoParsed,
    StatusPresent, StatusParsed, VolumePresent, VolumeParsed, PositionPresent,
    PositionParsed, DurationPresent, DurationParsed, LoopPresent, LoopParsed,
    SleepPresent, SleepParsed, BluetoothPresent, BluetoothParsed,
    HardwareRecognized, CapabilitiesPresent, IsV3Present,
    AnyConfirmed, AllConfirmed, WriteAttempted,
    EligibleSpeaker, SpecReady, SpeakerSpec, AdapterResolved, PlayerCurrent,
    LoopChangeAllowed, LoopRestoreAllowed,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticStep {
    stage: Stage,
    code: Code,
    flags: BTreeMap<Flag, bool>,
}

impl DiagnosticStep {
    pub fn new(stage: Stage, code: Code) -> Self {
        Self { stage, code, flags: BTreeMap::new() }
    }

    pub fn flag(mut self, flag: Flag, value: bool) -> Self {
        self.flags.insert(flag, value);
        self
    }
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    read_only: bool,
    steps: Vec<DiagnosticStep>,
}

fn report_json(steps: Vec<DiagnosticStep>, read_only: bool) -> String {
    // This schema contains no arbitrary data strings, paths, counts, IDs or
    // free-form errors. Serialization cannot expose an upstream error message.
    serde_json::to_string_pretty(&Report {
        schema: "homepanel_speaker_diagnostic_v1",
        read_only,
        steps,
    }).expect("closed diagnostic report is serializable")
}

fn directories() -> Option<(PathBuf, PathBuf)> {
    let root = PathBuf::from(std::env::var_os("APPDATA")?).join(crate::APP_ID);
    let cache = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(crate::APP_ID);
    // Deliberately do not call app_directories: it creates account directories.
    Some((root, cache))
}

fn collect() -> Vec<DiagnosticStep> {
    let Some((root, cache)) = directories() else {
        return vec![DiagnosticStep::new(Stage::Environment, Code::Unavailable)];
    };
    // These constructors only load existing DPAPI sessions. No discovery,
    // login, refresh, logout or preference/cache saving occurs in this entry.
    let Ok(main) = Cloud::new(root.clone(), cache) else {
        return vec![DiagnosticStep::new(Stage::Environment, Code::Unavailable)];
    };
    let Ok(service) = Service::new(&root) else {
        return vec![DiagnosticStep::new(Stage::Environment, Code::Unavailable)];
    };
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
        .enable_all().worker_threads(2).build() else {
        return vec![DiagnosticStep::new(Stage::Runtime, Code::Unavailable)];
    };
    let result = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(35), service.diagnostic(&main)).await
    });
    runtime.shutdown_timeout(Duration::from_secs(2));
    match result {
        Ok(steps) => steps,
        Err(_) => vec![DiagnosticStep::new(Stage::Overall, Code::Timeout)],
    }
}

fn select_probe_home<'a>(homes: impl Iterator<Item = &'a Home>, name: &str) -> Result<String, Code> {
    let mut matching = homes.filter(|home| home.name == name);
    let selected = matching.next().ok_or(Code::Unavailable)?;
    if selected.id.is_empty() { return Err(Code::InvalidResponse); }
    // Discovery may repeat an own/shared-home record. Equal account-scoped IDs
    // identify one home; a same-name record with a different ID remains unsafe.
    for home in matching {
        if home.id.is_empty() { return Err(Code::InvalidResponse); }
        if home.id != selected.id { return Err(Code::Ambiguous); }
    }
    Ok(selected.id.clone())
}

fn select_probe_device<'a>(devices: impl Iterator<Item = &'a Device>, name: &str, room: &str, home_id: &str) -> Result<Device, Code> {
    let mut matching = devices.filter(|device| device.kind == "speaker" && device.name == name
        && device.room == room && device.home_id == home_id);
    let selected = matching.next().ok_or(Code::Unavailable)?;
    if matching.next().is_some() { return Err(Code::Ambiguous); }
    Ok(selected.clone())
}

fn select_matching_probe_devices<'a>(devices: impl Iterator<Item = &'a Device>, name: &str, room: &str, home_id: &str) -> Result<Vec<Device>, Code> {
    let mut selected = BTreeMap::new();
    for device in devices.filter(|device| device.kind == "speaker" && device.name == name
        && device.room == room && device.home_id == home_id) {
        if device.id.is_empty() { return Err(Code::InvalidResponse); }
        if let Some(previous) = selected.get(&device.id) {
            let previous: &Device = previous;
            if previous.model != device.model { return Err(Code::InvalidResponse); }
        } else {
            selected.insert(device.id.clone(), device.clone());
        }
        if selected.len() > 8 { return Err(Code::Rejected); }
    }
    if selected.is_empty() { return Err(Code::Unavailable); }
    Ok(selected.into_values().collect())
}

fn exact_phase_code(report: &[DiagnosticStep], stage: Stage) -> Option<Code> {
    let mut phases = report.iter().filter(|step| step.stage == stage);
    let phase = phases.next()?;
    if phases.next().is_some() { return None; }
    Some(phase.code)
}

fn write_attempted(report: &[DiagnosticStep]) -> bool {
    report.iter().any(|step| step.stage == Stage::LoopChange)
}

fn continue_after_probe(report: &[DiagnosticStep]) -> bool {
    !write_attempted(report) || exact_phase_code(report, Stage::LoopRestore) == Some(Code::Ok)
}

fn aggregate_probe_reports(reports: &[Vec<DiagnosticStep>], all_selected_processed: bool, preflight_error: Option<Code>) -> Vec<DiagnosticStep> {
    let written: Vec<_> = reports.iter().filter(|report| write_attempted(report)).collect();
    let attempted = !written.is_empty();
    let change_any = written.iter().any(|report| exact_phase_code(report, Stage::LoopChange) == Some(Code::Ok));
    let change_all = !reports.is_empty() && all_selected_processed && preflight_error.is_none()
        && reports.iter().all(|report| exact_phase_code(report, Stage::LoopChange) == Some(Code::Ok));
    let restore_any = written.iter().any(|report| exact_phase_code(report, Stage::LoopRestore) == Some(Code::Ok));
    let restore_all = attempted && written.iter().all(|report| exact_phase_code(report, Stage::LoopRestore) == Some(Code::Ok));
    let change = if change_all { Code::Ok } else {
        written.iter().find_map(|report| exact_phase_code(report, Stage::LoopChange).filter(|code| *code != Code::Ok)).or(preflight_error)
            .or_else(|| reports.iter().flat_map(|report| report.iter()).find(|step| step.code != Code::Ok).map(|step| step.code))
            .unwrap_or(Code::Unavailable)
    };
    // Restoration concerns only targets that actually reached a write. A
    // rejected preflight is safe to skip and cannot manufacture restoration.
    let restore = if !attempted { Code::Unavailable } else if restore_all { Code::Ok } else {
        written.iter().find_map(|report| exact_phase_code(report, Stage::LoopRestore).filter(|code| *code != Code::Ok))
            .unwrap_or(Code::Unavailable)
    };
    let preflight_flags = [Flag::EligibleSpeaker, Flag::SpecReady, Flag::SpeakerSpec,
        Flag::AdapterResolved, Flag::PlayerCurrent, Flag::LoopChangeAllowed, Flag::LoopRestoreAllowed];
    let flag_all = |flag| !reports.is_empty() && reports.iter().all(|report| {
        exact_phase_code(report, Stage::Preflight).is_some()
            && report.iter().find(|step| step.stage == Stage::Preflight)
                .and_then(|step| step.flags.get(&flag)).copied() == Some(true)
    });
    let preflight_ok = all_selected_processed && preflight_error.is_none() && !reports.is_empty()
        && reports.iter().all(|report| exact_phase_code(report, Stage::Preflight) == Some(Code::Ok))
        && preflight_flags.iter().all(|flag| flag_all(*flag));
    let preflight_code = if preflight_ok { Code::Ok } else {
        preflight_error.or_else(|| reports.iter().find_map(|report|
            exact_phase_code(report, Stage::Preflight).filter(|code| *code != Code::Ok)))
            .unwrap_or(Code::Unavailable)
    };
    let mut preflight = DiagnosticStep::new(Stage::Preflight, preflight_code);
    for flag in preflight_flags { preflight = preflight.flag(flag, flag_all(flag)); }
    vec![DiagnosticStep::new(Stage::LoopChange, change).flag(Flag::AnyConfirmed, change_any)
            .flag(Flag::AllConfirmed, change_all).flag(Flag::WriteAttempted, attempted),
        DiagnosticStep::new(Stage::LoopRestore, restore).flag(Flag::AnyConfirmed, restore_any)
            .flag(Flag::AllConfirmed, restore_all).flag(Flag::WriteAttempted, attempted), preflight]
}

fn batch_failure(code: Code) -> Vec<DiagnosticStep> {
    aggregate_probe_reports(&[], false, Some(code))
}

fn collect_matching_loop_probes(name: &str, room: &str, home: &str) -> Vec<DiagnosticStep> {
    let Some((root, cache)) = directories() else { return batch_failure(Code::Unavailable); };
    let Ok(main) = Cloud::new(root.clone(), cache) else { return batch_failure(Code::Unavailable); };
    let Ok(service) = Service::new(&root) else { return batch_failure(Code::Unavailable); };
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
        .enable_all().worker_threads(2).build() else { return batch_failure(Code::Unavailable); };
    let steps = runtime.block_on(async {
        let prepare = async {
            main.discover("cn").await.map_err(|_| Code::Rejected)?;
            let home_id = {
                let homes = main.homes.lock().await;
                select_probe_home(homes.iter(), home)?
            };
            let devices = main.devices.lock().await;
            select_matching_probe_devices(devices.values(), name, room, &home_id)
        };
        let selected = match tokio::time::timeout(Duration::from_secs(30), prepare).await {
            Ok(Ok(selected)) => selected,
            Ok(Err(code)) => return batch_failure(code),
            Err(_) => return batch_failure(Code::Timeout),
        };
        let mut reports = Vec::new();
        for device in &selected {
            // Only this next device's pre-write SPEC read is cancellable. The
            // preceding target has already finished its restoration attempt.
            let current = match tokio::time::timeout(Duration::from_secs(30), main.load_spec(&device.id)).await {
                Ok(Ok(current)) if current.model == device.model && current.name == name
                    && current.room == room && current.home_id == device.home_id && current.kind == "speaker" => current,
                Ok(_) => { reports.push(vec![DiagnosticStep::new(Stage::Spec, Code::Rejected)]); continue; },
                Err(_) => { reports.push(vec![DiagnosticStep::new(Stage::Spec, Code::Timeout)]); continue; },
            };
            // Never add an outer timeout after a write can begin: always give
            // Service its bounded recovery/readback sequence before continuing.
            let report = service.probe_loop(&main, &current).await;
            let can_continue = continue_after_probe(&report);
            reports.push(report);
            if !can_continue {
                return aggregate_probe_reports(&reports, reports.len() == selected.len(), None);
            }
        }
        aggregate_probe_reports(&reports, true, None)
    });
    runtime.shutdown_timeout(Duration::from_secs(2));
    steps
}

fn collect_loop_probe(name: &str, room: &str, home: &str) -> Vec<DiagnosticStep> {
    let Some((root, cache)) = directories() else {
        return vec![DiagnosticStep::new(Stage::Environment, Code::Unavailable)];
    };
    let Ok(main) = Cloud::new(root.clone(), cache) else {
        return vec![DiagnosticStep::new(Stage::Environment, Code::Unavailable)];
    };
    let Ok(service) = Service::new(&root) else {
        return vec![DiagnosticStep::new(Stage::Environment, Code::Unavailable)];
    };
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
        .enable_all().worker_threads(2).build() else {
        return vec![DiagnosticStep::new(Stage::Runtime, Code::Unavailable)];
    };
    let steps = runtime.block_on(async {
        // Timeout only covers preflight reads, before Service can send a write.
        let prepare = async {
            main.discover("cn").await.map_err(|_| (Stage::DeviceList, Code::Rejected))?;
            let home_id = {
                let homes = main.homes.lock().await;
                select_probe_home(homes.iter(), home).map_err(|code| (Stage::Home, code))?
            };
            let device = {
                let devices = main.devices.lock().await;
                select_probe_device(devices.values(), name, room, &home_id).map_err(|code| (Stage::Target, code))?
            };
            main.load_spec(&device.id).await.map_err(|_| (Stage::Spec, Code::Rejected))
        };
        match tokio::time::timeout(Duration::from_secs(30), prepare).await {
            // Do not wrap this future in a cancellation timeout: restoration
            // must be attempted after any accepted or ambiguous temporary write.
            Ok(Ok(device)) => service.probe_loop(&main, &device).await,
            Ok(Err((stage, code))) => vec![DiagnosticStep::new(stage, code)],
            Err(_) => vec![DiagnosticStep::new(Stage::Overall, Code::Timeout)],
        }
    });
    runtime.shutdown_timeout(Duration::from_secs(2));
    steps
}

enum Entry<'a> {
    ReadOnly,
    LoopProbe { name: &'a str, room: &'a str, home: &'a str },
    MatchingLoopProbes { name: &'a str, room: &'a str, home: &'a str },
    Rejected { read_only: bool, batch: bool },
}

fn valid_selector(value: &str) -> bool {
    !value.trim().is_empty() && !value.starts_with("--") && value.len() <= 1024
        && !value.chars().any(char::is_control)
}

fn entry(args: &[String]) -> Option<Entry<'_>> {
    let read_only = args.iter().any(|arg| arg == "--diagnose-speaker");
    let loop_probe = args.iter().any(|arg| arg == "--probe-speaker-loop");
    let batch = args.iter().any(|arg| arg == "--probe-matching-speaker-loops");
    if !read_only && !loop_probe && !batch { return None; }
    if args.len() == 2 && args[1] == "--diagnose-speaker" {
        return Some(Entry::ReadOnly);
    }
    if args.len() == 8 && (args[1] == "--probe-speaker-loop" || args[1] == "--probe-matching-speaker-loops") && args[3] == "--room"
        && args[5] == "--home" && args[7] == "--restore-original" {
        let name = &args[2];
        let room = &args[4];
        let home = &args[6];
        if valid_selector(name) && valid_selector(room) && valid_selector(home) {
            return Some(if batch { Entry::MatchingLoopProbes { name, room, home } }
                else { Entry::LoopProbe { name, room, home } });
        }
    }
    Some(Entry::Rejected { read_only: !loop_probe && !batch, batch })
}

/// Intercept before single-instance acquisition so the currently running GUI
/// remains untouched. No path/file output option is accepted.
pub fn run(args: &[String]) -> Option<i32> {
    let (steps, read_only) = match entry(args)? {
        Entry::ReadOnly => (collect(), true),
        Entry::LoopProbe { name, room, home } => (collect_loop_probe(name, room, home), false),
        Entry::MatchingLoopProbes { name, room, home } => (collect_matching_loop_probes(name, room, home), false),
        Entry::Rejected { read_only, batch } => (if batch { batch_failure(Code::Rejected) }
            else { vec![DiagnosticStep::new(Stage::Environment, Code::Rejected)] }, read_only),
    };
    let exit = if steps.iter().all(|step| step.code == Code::Ok) { 0 } else { 2 };
    println!("{}", report_json(steps, read_only));
    Some(exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn report_only_contains_fixed_schema_and_boolean_fields() {
        let text = report_json(vec![
            DiagnosticStep::new(Stage::Player, Code::InvalidResponse)
                .flag(Flag::InfoPresent, true)
                .flag(Flag::InfoParsed, false),
        ], true);
        let value: Value = serde_json::from_str(&text).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 3);
        assert_eq!(value["schema"], "homepanel_speaker_diagnostic_v1");
        assert_eq!(value["read_only"], true);
        let step = value["steps"][0].as_object().unwrap();
        assert_eq!(step.len(), 3);
        assert_eq!(step["stage"], "player");
        assert_eq!(step["code"], "invalid_response");
        assert!(step["flags"].as_object().unwrap().values().all(Value::is_boolean));
        assert_eq!(step["flags"]["info_present"], true);
        assert_eq!(step["flags"]["info_parsed"], false);
    }

    #[test]
    fn every_output_token_is_allowlisted() {
        let stages = [Stage::Environment, Stage::Runtime, Stage::MainSession,
            Stage::SpeakerSession, Stage::Identity, Stage::DeviceList,
            Stage::Home, Stage::Target, Stage::Spec, Stage::Player, Stage::Sleep, Stage::Bluetooth,
            Stage::Preflight, Stage::LoopChange, Stage::LoopRestore, Stage::Overall];
        let codes = [Code::Ok, Code::Unavailable, Code::Rejected, Code::InvalidResponse,
            Code::Timeout, Code::Unauthorized, Code::Forbidden, Code::Network,
            Code::SessionChanged, Code::Ambiguous, Code::ResponseTooLarge, Code::Unconfirmed];
        let flags = [Flag::SessionPresent, Flag::IdentityMatches, Flag::DeviceListPresent,
            Flag::DeviceListParsed, Flag::TargetMatched, Flag::TargetFieldsValid,
            Flag::EnvelopeParsed, Flag::InfoPresent, Flag::InfoParsed,
            Flag::StatusPresent, Flag::StatusParsed, Flag::VolumePresent,
            Flag::VolumeParsed, Flag::PositionPresent, Flag::PositionParsed,
            Flag::DurationPresent, Flag::DurationParsed, Flag::LoopPresent,
            Flag::LoopParsed, Flag::SleepPresent, Flag::SleepParsed,
            Flag::BluetoothPresent, Flag::BluetoothParsed, Flag::HardwareRecognized,
            Flag::CapabilitiesPresent, Flag::IsV3Present, Flag::AnyConfirmed,
            Flag::AllConfirmed, Flag::WriteAttempted, Flag::EligibleSpeaker,
            Flag::SpecReady, Flag::SpeakerSpec, Flag::AdapterResolved,
            Flag::PlayerCurrent, Flag::LoopChangeAllowed, Flag::LoopRestoreAllowed];
        for stage in stages {
            for code in codes {
                let mut step = DiagnosticStep::new(stage, code);
                for flag in flags { step = step.flag(flag, true); }
                let report: Value = serde_json::from_str(&report_json(vec![step], true)).unwrap();
                let item = &report["steps"][0];
                for token in [item["stage"].as_str().unwrap(), item["code"].as_str().unwrap()] {
                    assert!(token.bytes().all(|c| c.is_ascii_lowercase() || c == b'_'));
                }
                for (key, value) in item["flags"].as_object().unwrap() {
                    assert!(key.bytes().all(|c| c.is_ascii_lowercase() || c == b'_' || c.is_ascii_digit()));
                    assert_eq!(value, true);
                }
            }
        }
    }

    #[test]
    fn unrecognized_arguments_do_not_initialize_account_services() {
        assert_eq!(run(&["HomePanel.exe".into(), "--unrelated".into()]), None);
        assert_eq!(run(&["HomePanel.exe".into(), "--diagnose-speaker".into(),
            "private-path-must-not-be-emitted".into()]), Some(2));
    }

    #[test]
    fn loop_probe_requires_exact_explicit_restore_arguments() {
        let args = |tail: &[&str]| std::iter::once("HomePanel.exe").chain(tail.iter().copied()).map(str::to_owned).collect::<Vec<_>>();
        let valid = args(&["--probe-speaker-loop", "测试音箱", "--room", "测试房间", "--home", "测试家庭", "--restore-original"]);
        assert!(matches!(entry(&valid), Some(Entry::LoopProbe { name: "测试音箱", room: "测试房间", home: "测试家庭" })));
        for tail in [
            vec!["--probe-speaker-loop", "测试音箱"],
            vec!["--probe-speaker-loop", "测试音箱", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--restore-original", "extra"],
            vec!["--restore-original", "--probe-speaker-loop", "测试音箱"],
            vec!["--probe-speaker-loop", "  ", "--restore-original"],
            vec!["--probe-speaker-loop", "private\nname", "--restore-original"],
            vec!["--probe-speaker-loop", "--diagnose-speaker", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--room", " ", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--room", "private\nroom", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--room", "--restore-original", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--room", "测试房间", "--restore-original", "extra"],
            vec!["--probe-speaker-loop", "测试音箱", "--restore-original", "--room", "测试房间"],
            vec!["--probe-speaker-loop", "测试音箱", "--room", "测试房间", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--room", "测试房间", "--home", " ", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--room", "测试房间", "--home", "private\nhome", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--room", "测试房间", "--home", "--restore-original", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--home", "测试家庭", "--room", "测试房间", "--restore-original"],
            vec!["--probe-speaker-loop", "测试音箱", "--room", "测试房间", "--home", "测试家庭", "--restore-original", "extra"],
        ] {
            assert!(matches!(entry(&args(&tail)), Some(Entry::Rejected { read_only: false, .. })));
        }
    }

    #[test]
    fn probe_target_requires_unique_exact_name_and_speaker_kind() {
        let speaker = Device { id: "synthetic-id".into(), name: "测试音箱".into(), room: "测试房间".into(), kind: "speaker".into(), ..Default::default() };
        let other = Device { kind: "switch".into(), ..speaker.clone() };
        assert!(select_probe_device([&speaker, &other].into_iter(), "测试音箱", "测试房间", "").is_ok());
        assert!(matches!(select_probe_device([&speaker, &speaker].into_iter(), "测试音箱", "测试房间", ""), Err(Code::Ambiguous)));
        assert!(matches!(select_probe_device([&other].into_iter(), "测试音箱", "测试房间", ""), Err(Code::Unavailable)));
        assert!(matches!(select_probe_device([&speaker].into_iter(), "测试", "测试房间", "").map(|_| ()), Err(Code::Unavailable)));
        assert!(matches!(select_probe_device([&speaker].into_iter(), "测试音箱", "测试", "").map(|_| ()), Err(Code::Unavailable)));
    }

    #[test]
    fn room_selector_disambiguates_same_name_without_accepting_same_room_duplicates() {
        let first = Device { id: "synthetic-first".into(), name: "测试音箱".into(), room: "测试房间甲".into(), kind: "speaker".into(), ..Default::default() };
        let second = Device { id: "synthetic-second".into(), room: "测试房间乙".into(), ..first.clone() };
        let duplicate = Device { id: "synthetic-duplicate".into(), ..first.clone() };
        let selected = select_probe_device([&first, &second].into_iter(), "测试音箱", "测试房间甲", "").unwrap();
        assert_eq!(selected.id, first.id);
        assert!(matches!(select_probe_device([&first, &duplicate, &second].into_iter(), "测试音箱", "测试房间甲", ""), Err(Code::Ambiguous)));
    }

    #[test]
    fn home_selection_requires_unique_exact_name_and_valid_identity() {
        let first = Home { id: "synthetic-home-first".into(), name: "测试家庭甲".into(), owner: String::new(), rooms: vec![] };
        let second = Home { id: "synthetic-home-second".into(), name: "测试家庭乙".into(), owner: String::new(), rooms: vec![] };
        let duplicate = Home { id: "synthetic-home-duplicate".into(), ..first.clone() };
        assert_eq!(select_probe_home([&first, &second].into_iter(), "测试家庭甲").unwrap(), first.id);
        assert!(matches!(select_probe_home([&first, &duplicate].into_iter(), "测试家庭甲"), Err(Code::Ambiguous)));
        assert!(matches!(select_probe_home([&first].into_iter(), "测试家庭"), Err(Code::Unavailable)));
        let invalid = Home { id: String::new(), ..first.clone() };
        assert!(matches!(select_probe_home([&invalid].into_iter(), "测试家庭甲"), Err(Code::InvalidResponse)));
    }

    #[test]
    fn repeated_home_records_with_same_identity_are_one_home() {
        let home = Home { id: "synthetic-home".into(), name: "测试家庭".into(), owner: String::new(), rooms: vec![] };
        let same = home.clone();
        assert_eq!(select_probe_home([&home, &same].into_iter(), "测试家庭").unwrap(), home.id);
        let different = Home { id: "synthetic-other-home".into(), ..home.clone() };
        assert!(matches!(select_probe_home([&home, &same, &different].into_iter(), "测试家庭"), Err(Code::Ambiguous)));
    }

    #[test]
    fn home_id_disambiguates_same_name_and_room_between_families() {
        let first = Device { id: "synthetic-device-first".into(), name: "测试音箱".into(), room: "测试房间".into(), home_id: "synthetic-home-first".into(), kind: "speaker".into(), ..Default::default() };
        let second = Device { id: "synthetic-device-second".into(), home_id: "synthetic-home-second".into(), ..first.clone() };
        let selected = select_probe_device([&first, &second].into_iter(), "测试音箱", "测试房间", &second.home_id).unwrap();
        assert_eq!(selected.id, second.id);
        assert!(matches!(select_probe_device([&first].into_iter(), "测试音箱", "测试房间", &second.home_id), Err(Code::Unavailable)));
    }

    #[test]
    fn batch_entry_is_explicit_and_does_not_relax_single_selection() {
        let valid = ["HomePanel.exe", "--probe-matching-speaker-loops", "测试音箱", "--room", "测试房间", "--home", "测试家庭", "--restore-original"].map(str::to_owned);
        assert!(matches!(entry(&valid), Some(Entry::MatchingLoopProbes { name: "测试音箱", room: "测试房间", home: "测试家庭" })));
        assert!(matches!(entry(&valid[..7]), Some(Entry::Rejected { read_only: false, batch: true })));
        let mut extra = valid.to_vec();
        extra.push("extra".into());
        assert!(matches!(entry(&extra), Some(Entry::Rejected { read_only: false, batch: true })));
        let device = Device { id: "synthetic-one".into(), name: "测试音箱".into(), room: "测试房间".into(), home_id: "synthetic-home".into(), kind: "speaker".into(), ..Default::default() };
        let other = Device { id: "synthetic-two".into(), ..device.clone() };
        assert!(matches!(select_probe_device([&device, &other].into_iter(), "测试音箱", "测试房间", "synthetic-home"), Err(Code::Ambiguous)));
    }

    #[test]
    fn matching_batch_is_exact_bounded_deduplicated_and_stably_sorted() {
        let first = Device { id: "synthetic-b".into(), name: "测试音箱".into(), room: "测试房间".into(), home_id: "synthetic-home".into(), model: "synthetic-model".into(), kind: "speaker".into(), ..Default::default() };
        let second = Device { id: "synthetic-a".into(), ..first.clone() };
        let elsewhere = Device { id: "synthetic-elsewhere".into(), room: "其他房间".into(), ..first.clone() };
        let other_home = Device { id: "synthetic-other-home".into(), home_id: "other-home".into(), ..first.clone() };
        let other_kind = Device { id: "synthetic-switch".into(), kind: "switch".into(), ..first.clone() };
        let other_name = Device { id: "synthetic-other-name".into(), name: "其他音箱".into(), ..first.clone() };
        let selected = select_matching_probe_devices([&first, &second, &first, &elsewhere, &other_home, &other_kind, &other_name].into_iter(), "测试音箱", "测试房间", "synthetic-home").unwrap();
        assert_eq!(selected.iter().map(|device| device.id.as_str()).collect::<Vec<_>>(), vec!["synthetic-a", "synthetic-b"]);
        let oversized: Vec<_> = (0..9).map(|index| Device { id: format!("synthetic-{index}"), ..first.clone() }).collect();
        assert!(select_matching_probe_devices(oversized[..8].iter(), "测试音箱", "测试房间", "synthetic-home").is_ok());
        assert!(matches!(select_matching_probe_devices(oversized.iter(), "测试音箱", "测试房间", "synthetic-home"), Err(Code::Rejected)));
        assert!(matches!(select_matching_probe_devices([&elsewhere].into_iter(), "测试音箱", "测试房间", "synthetic-home"), Err(Code::Unavailable)));
        let conflicting = Device { model: "different-model".into(), ..first.clone() };
        assert!(matches!(select_matching_probe_devices([&first, &conflicting].into_iter(), "测试音箱", "测试房间", "synthetic-home"), Err(Code::InvalidResponse)));
    }

    #[test]
    fn aggregate_batch_never_claims_restore_without_change_or_all_changes_for_incomplete_set() {
        let stage = |stage, code| DiagnosticStep::new(stage, code);
        let empty = aggregate_probe_reports(&[], false, Some(Code::Rejected));
        assert_eq!(exact_phase_code(&empty, Stage::LoopChange), Some(Code::Rejected));
        assert_eq!(exact_phase_code(&empty, Stage::LoopRestore), Some(Code::Unavailable));
        let preflight = vec![vec![stage(Stage::Target, Code::Unavailable)]];
        let rejected = aggregate_probe_reports(&preflight, true, None);
        assert_eq!(exact_phase_code(&rejected, Stage::LoopChange), Some(Code::Unavailable));
        assert_eq!(exact_phase_code(&rejected, Stage::LoopRestore), Some(Code::Unavailable));
        let valid = vec![vec![stage(Stage::LoopChange, Code::Ok), stage(Stage::LoopRestore, Code::Ok)]];
        let complete = aggregate_probe_reports(&valid, true, None);
        assert_eq!(exact_phase_code(&complete, Stage::LoopChange), Some(Code::Ok));
        assert_eq!(exact_phase_code(&complete, Stage::LoopRestore), Some(Code::Ok));
        let incomplete = aggregate_probe_reports(&valid, false, None);
        assert_eq!(exact_phase_code(&incomplete, Stage::LoopChange), Some(Code::Unavailable));
        assert_eq!(exact_phase_code(&incomplete, Stage::LoopRestore), Some(Code::Ok));
        assert_eq!(incomplete[0].flags.get(&Flag::AllConfirmed), Some(&false));
        assert_eq!(incomplete[1].flags.get(&Flag::AllConfirmed), Some(&true));
        let failed_restore = vec![vec![stage(Stage::LoopChange, Code::Rejected), stage(Stage::LoopRestore, Code::Rejected)]];
        let failed = aggregate_probe_reports(&failed_restore, true, None);
        assert_eq!(exact_phase_code(&failed, Stage::LoopChange), Some(Code::Rejected));
        assert_eq!(exact_phase_code(&failed, Stage::LoopRestore), Some(Code::Rejected));
        assert!(failed.iter().all(|step| matches!(step.stage, Stage::Preflight | Stage::LoopChange | Stage::LoopRestore)));
        let no_change = vec![vec![stage(Stage::LoopRestore, Code::Ok)]];
        assert_eq!(exact_phase_code(&aggregate_probe_reports(&no_change, true, None), Stage::LoopRestore), Some(Code::Unavailable));
        let mixed_missing_change = vec![valid[0].clone(), no_change[0].clone()];
        assert_eq!(exact_phase_code(&aggregate_probe_reports(&mixed_missing_change, true, None), Stage::LoopRestore), Some(Code::Ok));
    }

    #[test]
    fn rejected_preflight_can_continue_to_confirmed_write_and_all_actual_writes_restore() {
        let skipped = vec![DiagnosticStep::new(Stage::Target, Code::Rejected)];
        assert!(continue_after_probe(&skipped));
        let successful = vec![DiagnosticStep::new(Stage::LoopChange, Code::Ok), DiagnosticStep::new(Stage::LoopRestore, Code::Ok)];
        assert!(continue_after_probe(&successful));
        let aggregate = aggregate_probe_reports(&[skipped, successful], true, None);
        assert_eq!(exact_phase_code(&aggregate, Stage::LoopChange), Some(Code::Rejected));
        assert_eq!(aggregate[0].flags.get(&Flag::AnyConfirmed), Some(&true));
        assert_eq!(aggregate[0].flags.get(&Flag::AllConfirmed), Some(&false));
        assert_eq!(aggregate[0].flags.get(&Flag::WriteAttempted), Some(&true));
        assert_eq!(exact_phase_code(&aggregate, Stage::LoopRestore), Some(Code::Ok));
        assert_eq!(aggregate[1].flags.get(&Flag::AnyConfirmed), Some(&true));
        assert_eq!(aggregate[1].flags.get(&Flag::AllConfirmed), Some(&true));
    }

    #[test]
    fn preflight_flags_require_every_report_and_missing_fields_are_false() {
        let flags = [Flag::EligibleSpeaker, Flag::SpecReady, Flag::SpeakerSpec,
            Flag::AdapterResolved, Flag::PlayerCurrent, Flag::LoopChangeAllowed, Flag::LoopRestoreAllowed];
        let complete = || {
            let mut step = DiagnosticStep::new(Stage::Preflight, Code::Ok);
            for flag in flags { step = step.flag(flag, true); }
            step
        };
        let good = aggregate_probe_reports(&[vec![complete()], vec![complete()]], true, None);
        assert_eq!(exact_phase_code(&good, Stage::Preflight), Some(Code::Ok));
        let preflight = good.iter().find(|step| step.stage == Stage::Preflight).unwrap();
        assert!(flags.iter().all(|flag| preflight.flags.get(flag) == Some(&true)));
        let mut missing = complete();
        missing.flags.remove(&Flag::AdapterResolved);
        let partial = aggregate_probe_reports(&[vec![complete()], vec![missing]], true, None);
        let preflight = partial.iter().find(|step| step.stage == Stage::Preflight).unwrap();
        assert_eq!(preflight.code, Code::Unavailable);
        assert_eq!(preflight.flags.get(&Flag::AdapterResolved), Some(&false));
        assert_eq!(preflight.flags.get(&Flag::SpecReady), Some(&true));
        let false_flag = complete().flag(Flag::LoopChangeAllowed, false);
        let rejected = aggregate_probe_reports(&[vec![complete()], vec![false_flag]], true, None);
        let preflight = rejected.iter().find(|step| step.stage == Stage::Preflight).unwrap();
        assert_eq!(preflight.flags.get(&Flag::LoopChangeAllowed), Some(&false));
        let no_stage = aggregate_probe_reports(&[vec![complete()], vec![DiagnosticStep::new(Stage::Spec, Code::Rejected)]], true, None);
        let preflight = no_stage.iter().find(|step| step.stage == Stage::Preflight).unwrap();
        assert!(preflight.flags.values().all(|flag| !*flag));
        let empty = batch_failure(Code::Unavailable);
        let preflight = empty.iter().find(|step| step.stage == Stage::Preflight).unwrap();
        assert_eq!(preflight.code, Code::Unavailable);
        assert!(preflight.flags.values().all(|flag| !*flag));
    }

    #[test]
    fn failed_or_missing_real_restoration_stops_before_next_report() {
        let source = [
            vec![DiagnosticStep::new(Stage::Target, Code::Unavailable)],
            vec![DiagnosticStep::new(Stage::LoopChange, Code::Ok), DiagnosticStep::new(Stage::LoopRestore, Code::Rejected)],
            vec![DiagnosticStep::new(Stage::LoopChange, Code::Ok), DiagnosticStep::new(Stage::LoopRestore, Code::Ok)],
        ];
        let mut processed = Vec::new();
        for report in source.iter().cloned() {
            let can_continue = continue_after_probe(&report);
            processed.push(report);
            if !can_continue { break; }
        }
        assert_eq!(processed.len(), 2);
        let aggregate = aggregate_probe_reports(&processed, false, None);
        assert_eq!(exact_phase_code(&aggregate, Stage::LoopRestore), Some(Code::Rejected));
        assert_eq!(aggregate[1].flags.get(&Flag::AllConfirmed), Some(&false));
        assert!(!continue_after_probe(&[DiagnosticStep::new(Stage::LoopChange, Code::Ok)]));
        assert!(continue_after_probe(&[DiagnosticStep::new(Stage::LoopChange, Code::Rejected), DiagnosticStep::new(Stage::LoopRestore, Code::Ok)]));
    }

    #[test]
    fn write_probe_report_cannot_claim_read_only_or_include_target_values() {
        let text = report_json(vec![
            DiagnosticStep::new(Stage::LoopChange, Code::Ok).flag(Flag::LoopParsed, true),
            DiagnosticStep::new(Stage::LoopRestore, Code::Ok).flag(Flag::LoopParsed, true),
        ], false);
        let report: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(report["read_only"], false);
        assert_eq!(report["steps"][0]["stage"], "loop_change");
        assert_eq!(report["steps"][1]["stage"], "loop_restore");
        let allowed = ["homepanel_speaker_diagnostic_v1", "loop_change", "loop_restore", "ok"];
        fn check(value: &Value, allowed: &[&str]) {
            match value {
                Value::String(value) => assert!(allowed.contains(&value.as_str())),
                Value::Bool(_) => {},
                Value::Object(object) => object.values().for_each(|value| check(value, allowed)),
                Value::Array(array) => array.iter().for_each(|value| check(value, allowed)),
                _ => panic!("diagnostic output must not contain numeric or raw device values"),
            }
        }
        check(&report, &allowed);
    }
}
