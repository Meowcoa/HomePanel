#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod design;
mod cloud;
mod control;
mod crypto;
mod model;
mod presentation;
mod native;
mod storage;
mod dismiss;
mod time_range;
mod state_sync;
mod overview;
mod motion;
mod fonts;
mod adapters;
mod plugin_probe;
mod plugin_bridge;
mod plugin_runtime;
mod plugin_download;
mod plugin_signature;
mod plugin_bridge_extensions;
mod plugin_catalog;
mod plugin_prepare;
mod plugin_services;
mod speaker_service;
#[cfg(debug_assertions)]
mod speaker_diagnostics;
mod update;

use app::HomePanelApp;
use model::Preferences;
use std::{path::PathBuf, sync::Arc};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::Builder;

const APP_ID: &str = "io.github.meowcoa.homepanel";

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn app_directories(demo: bool) -> Result<(PathBuf, PathBuf), String> {
    let config = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("Windows 用户配置目录不可用")?
        .join(if demo { "io.github.meowcoa.homepanel.demo" } else { APP_ID });
    let cache = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("Windows 本机缓存目录不可用")?
        .join(if demo { "io.github.meowcoa.homepanel.demo" } else { APP_ID });
    std::fs::create_dir_all(&config).map_err(|_| "无法创建 HomePanel 配置目录")?;
    std::fs::create_dir_all(&cache).map_err(|_| "无法创建 HomePanel 缓存目录")?;
    Ok((config, cache))
}

fn load_preferences(root: &std::path::Path) -> Preferences {
    std::fs::read(root.join("preferences.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(debug_assertions)]
    if let Some(code) = plugin_download::run(&args) { std::process::exit(code); }
    #[cfg(debug_assertions)]
    if let Some(code) = plugin_services::run(&args) { std::process::exit(code); }
    #[cfg(debug_assertions)]
    if let Some(code) = speaker_diagnostics::run(&args) { std::process::exit(code); }
    if args.get(1).is_some_and(|arg|arg=="--close-for-update") {
        if args.len()!=3 {std::process::exit(2);}
        match update::close_for_update(std::path::Path::new(&args[2])) {
            Ok(forced)=>std::process::exit(if forced {3}else{0}),
            Err(error)=>{eprintln!("{error}");std::process::exit(2);}
        }
    }
    #[cfg(debug_assertions)]
    if let Some(result) = plugin_bridge::run(&args) {
        if let Err(code) = result { eprintln!("RN host experiment: {code}"); std::process::exit(2); }
        return;
    }
    // Offline import for an explicitly selected directory; no login/session is read.
    if let Some(index) = args.iter().position(|arg| arg == "--import-plugin-apk") {
        let result = (|| {
            let apk=args.get(index+1).ok_or("请提供 APK 路径")?;
            let dir=args.iter().position(|arg|arg=="--plugin-data-dir").and_then(|i|args.get(i+1)).ok_or("请显式提供 --plugin-data-dir 本机导入目录")?;
            std::fs::create_dir_all(dir).map_err(|_|"无法创建本机导入目录")?;
            plugin_runtime::import_apk(std::path::Path::new(apk),std::path::Path::new(dir))?;
            plugin_runtime::validate(std::path::Path::new(dir))?;
            Ok::<(),String>(())
        })();
        match result {Ok(())=>println!("本机候选导入及校验通过"),Err(error)=>{eprintln!("{error}");std::process::exit(2);}}
        return;
    }
    // Offline entry point, before account directories, Cloud, tray or runtime.
    if let Some(index) = args.iter().position(|arg| arg == "--inspect-plugin") {
        let result = args.get(index + 1).ok_or_else(|| "请提供插件包路径".to_owned())
            .and_then(|path| plugin_probe::inspect(std::path::Path::new(path)));
        match result {
            Ok(report) => println!("{}", serde_json::to_string_pretty(&report).expect("serializable plugin report")),
            Err(error) => { eprintln!("{error}"); std::process::exit(2); }
        }
        return;
    }
    if args.iter().any(|arg|arg=="--self-check") {std::process::exit(if fonts::verify(){0}else{1});}
    if args.iter().any(|arg|arg=="--font-license") {println!("{}",fonts::LICENSE);return;}
    #[cfg(debug_assertions)]
    let demo = args.iter().any(|arg| arg == "--demo" || arg == "--demo-detail" || arg == "--demo-climate" || arg == "--demo-speaker" || arg == "--demo-speaker-service" || arg.starts_with("--demo-lifecycle="));
    #[cfg(not(debug_assertions))]
    let demo = false;
    #[cfg(debug_assertions)]
    let demo_detail = args.iter().any(|arg| arg == "--demo-detail" || arg == "--demo-climate" || arg == "--demo-speaker" || arg == "--demo-speaker-service");
    #[cfg(not(debug_assertions))]
    let demo_detail = false;

    match native::acquire_single_instance(demo) {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    }

    let (root, cache) = match app_directories(demo) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };
    let preferences = if demo { Preferences::default() } else { load_preferences(&root) };
    let cloud = match cloud::Cloud::new(root.clone(), cache) {
        Ok(cloud) => Arc::new(cloud),
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };
    let runtime = match Builder::new_multi_thread().enable_all().worker_threads(2).build() {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("无法启动设备服务运行时");
            return;
        }
    };

    let show_at_start = (demo && !args.iter().any(|arg|arg=="--background"))
        || (!args.iter().any(|arg| arg == "--background") && !cloud.has_saved_session);
    let initial_position = native::panel_position(None, 1.0, [460.0, 620.0]);
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title(if demo { "HomePanel · 演示预览" } else { "HomePanel" })
        .with_inner_size([460.0, 620.0])
        .with_min_inner_size([388.0, 540.0])
        .with_max_inner_size([460.0, 620.0])
        .with_position(initial_position)
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
        .with_visible(show_at_start)
        .with_taskbar(false)
        .with_always_on_top();
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    // The GUI owns only a runtime Handle. Drop its windows/tray before waiting
    // for network/DNS blocking work, and bound that wait so exit cannot hang.
    let runtime_handle = runtime.handle().clone();
    let result = eframe::run_native(
        "HomePanel",
        options,
        Box::new(move |cc| {
            match HomePanelApp::new(cc, cloud, runtime_handle, preferences, root, demo, demo_detail, show_at_start) {
                Ok(app) => Ok(Box::new(app) as Box<dyn eframe::App>),
                Err(error) => Err(std::io::Error::other(error).into()),
            }
        }),
    );
    #[cfg(debug_assertions)]
    if demo { eprintln!("LIFECYCLE gui-returned"); }
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    #[cfg(debug_assertions)]
    if demo { eprintln!("LIFECYCLE runtime-stopped"); }
    if let Err(error) = result {
        eprintln!("HomePanel 无法启动：{error}");
    }
}
