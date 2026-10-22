#[path = "detail.rs"]
mod detail;
#[path = "activity.rs"]
mod activity;
#[path = "speaker_panel.rs"]
mod speaker_panel;
#[path = "plugin_panel.rs"]
mod plugin_panel;

use crate::{
    cloud::{Cloud, LoginStatus},
    model::{Action, Device, GlassStyle, Home, Preferences, Property},
    design::{self, Icon}, native, storage,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use eframe::egui::{self, Align, Align2, Color32, CornerRadius, Frame, Id, Layout, RichText, Sense, Stroke, Vec2};
use image::GenericImageView;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};
use tokio::{runtime::Handle, sync::watch};
use tray_icon::{menu::MenuEvent, MouseButton, MouseButtonState, TrayIcon, TrayIconEvent};
#[cfg(debug_assertions)]
use crate::model::PropertyOption;

const PANEL_SIZE: [f32; 2] = [460.0, 620.0];
const ACCENT: Color32 = design::GREEN;

#[derive(Clone, Copy)]
enum CardIntent { Details, Power, Favorite, Refresh, Hide }

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Login,
    Dashboard,
    Activity,
    Settings,
}

enum UiMessage {
    TrayToggle(Option<[f64; 2]>),
    TraySettings,
    Quit,
    DevicesLoaded(Result<(Vec<Home>, Vec<Device>), String>),
    DeviceUpdated(String, u64, Result<Device, String>),
    PropertyWritten(String, Result<Device, String>),
    ActionFinished(String, Result<Device, String>),
    ImageLoaded(String, Result<Option<String>, String>),
    LogoutFinished(Result<(), String>),
    PluginInspected(Result<crate::plugin_probe::Report, String>),
    PluginImported(Result<(), String>),
    PluginDownloaded(String,u64,Result<crate::plugin_download::DownloadReport,String>),
    PluginPrepared(String,u64,Option<(Device,bool,crate::plugin_download::DownloadReport)>,Result<crate::plugin_prepare::Prepared,String>),
    PluginStarted(u64,u64,Result<crate::plugin_bridge::Session,String>),
    MinaLoginError(u64,u64,String),
    MinaRead(String,u64,u64,u64,Result<crate::speaker_service::Snapshot,String>),
    MinaWritten(String,u64,u64,Result<crate::speaker_service::Snapshot,String>),
    #[cfg(debug_assertions)]
    DemoMaterial(GlassStyle,bool,bool),
}

pub struct HomePanelApp {
    context: egui::Context,
    cloud: Arc<Cloud>,
    runtime: Handle,
    root: PathBuf,
    preferences: Preferences,
    homes: Vec<Home>,
    devices: Vec<Device>,
    authenticated: bool,
    loading: bool,
    refreshing: bool,
    login: LoginStatus,
    login_rx: watch::Receiver<LoginStatus>,
    events_tx: mpsc::Sender<UiMessage>,
    events_rx: mpsc::Receiver<UiMessage>,
    _tray: TrayIcon,
    native_handle: Option<isize>,
    page: Page,
    visible: bool,
    dismiss_guard: crate::dismiss::DismissGuard,
    quitting: bool,
    demo: bool,
    appearance: native::Appearance,
    last_appearance_poll: Instant,
    dark: bool,
    selected_home: Option<String>,
    selected_room: String,
    search_query: String,
    search_open: bool,
    selected_device: Option<String>,
    detail_tab: detail::DetailTab,
    activity_filter: crate::overview::ActivityFilter,
    confirm_hide: Option<String>,
    confirm_logout: bool,
    pending: HashSet<String>,
    control_drafts: HashMap<String, Value>,
    textures: HashMap<String, egui::TextureHandle>,
    image_requests: HashSet<String>,
    qr_texture: Option<(String, egui::TextureHandle)>,
    reading: HashSet<String>,
    poll_schedule: crate::state_sync::PollSchedule,
    last_state_poll: Instant,
    last_discovery: Instant,
    window_region:Option<[i32;5]>,
    detail_geometry:bool,
    backdrop_pending:bool,
    main_position:Option<egui::Pos2>,
    content_motion:crate::motion::Transition,
    frame_motion:crate::motion::Sample,
    error: Option<(String, Instant)>,
    notice: Option<(String, Instant)>,
    image_model: String,
    image_url: String,
    permission_url: String,
    adapters: crate::adapters::Catalog,
    plugin_report: Option<crate::plugin_probe::Report>,
    plugin_inspecting: bool,
    plugin_importing: bool,
    plugin_download_pending: HashMap<String,u64>,
    plugin_download_results: HashMap<String,Result<crate::plugin_download::DownloadReport,String>>,
    plugin_catalog: Vec<crate::plugin_catalog::CatalogEntry>,
    plugin_catalog_error: Option<String>,
    plugin_preparing: HashMap<String,u64>,
    plugin_prepared: HashMap<String,Result<crate::plugin_prepare::Prepared,String>>,
    plugin_session: Option<crate::plugin_bridge::Session>,
    plugin_launch_generation:u64,
    plugin_launching:bool,
    mina:Arc<crate::speaker_service::Service>,
    mina_login_rx:watch::Receiver<LoginStatus>,
    mina_login:LoginStatus,
    mina_login_loading:bool,
    mina_connected:bool,
    mina_qr:Option<(String,egui::TextureHandle)>,
    mina_states:HashMap<String,crate::speaker_service::Snapshot>,
    mina_reading:HashSet<String>,
    mina_pending:HashSet<String>,
    mina_generation:u64,
}

impl HomePanelApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        cloud: Arc<Cloud>,
        runtime: Handle,
        preferences: Preferences,
        root: PathBuf,
        demo: bool,
        demo_detail: bool,
        show_at_start: bool,
    ) -> Result<Self, String> {
        let appearance = native::appearance();
        let dark = theme_is_dark(&preferences, appearance);
        let native_handle = native::native_window_id(cc);
        cc.egui_ctx.options_mut(|options| options.zoom_with_keyboard = false);
        cc.egui_ctx.set_zoom_factor(1.0);
        crate::fonts::install(&cc.egui_ctx);
        apply_visuals(&cc.egui_ctx, dark, appearance.reduce_motion);
        native::apply_window_appearance(
            cc,
            preferences.material(appearance.glass_available),
            dark,
        );
        let tray = native::create_tray_icon()?;
        let (events_tx, events_rx) = mpsc::channel();
        install_tray_handlers(&cc.egui_ctx, events_tx.clone(), native_handle);
        let update_context=cc.egui_ctx.clone();let update_tx=events_tx.clone();
        crate::update::listen(move || dispatch_menu_command("quit",&update_context,&update_tx,native_handle))?;
        #[cfg(debug_assertions)]
        if demo {eprintln!("LIFECYCLE update-listener-ready");}
        let login_rx = cloud.subscribe_login();

        let authenticated = demo || cloud.has_saved_session;
        #[cfg(debug_assertions)]
        let (homes, devices) = if demo {
            demo_devices()
        } else {
            (Vec::new(), Vec::new())
        };
        #[cfg(not(debug_assertions))]
        let (homes, devices) = (Vec::new(), Vec::new());
        #[cfg(debug_assertions)]
        let selected_device = if demo_detail {
            if std::env::args().any(|arg| arg == "--demo-speaker" || arg == "--demo-speaker-service") { Some("demo-speaker".into()) }
            else if std::env::args().any(|arg| arg == "--demo-climate") { Some("demo-climate".into()) }
            else { devices.first().map(|device| device.id.clone()) }
        } else {
            None
        };
        #[cfg(not(debug_assertions))]
        let selected_device = {
            let _ = demo_detail;
            None
        };

        let mina=Arc::new(crate::speaker_service::Service::new(&root)?);
        let mina_login_rx=mina.auth.subscribe_login();
        let mina_connected=mina.auth.has_saved_session;
        let adapters = crate::adapters::Catalog::load(&root);
        let mut app = Self {
            context: cc.egui_ctx.clone(),
            cloud,
            runtime,
            root,
            login_rx,
            preferences,
            homes,
            devices,
            authenticated,
            loading: authenticated && !demo,
            refreshing: false,
            login: LoginStatus::default(),
            events_tx,
            events_rx,
            _tray: tray,
            native_handle,
            page: if authenticated { Page::Dashboard } else { Page::Login },
            visible: show_at_start,
            dismiss_guard: Default::default(),
            quitting: false,
            demo,
            appearance,
            last_appearance_poll: Instant::now(),
            dark,
            selected_home: None,
            selected_room: "全部".into(),
            search_query: String::new(),
            search_open: false,
            selected_device,
            detail_tab: Default::default(),
            activity_filter: Default::default(),
            confirm_hide: None,
            confirm_logout: false,
            pending: HashSet::new(),
            control_drafts: HashMap::new(),
            textures: HashMap::new(),
            image_requests: HashSet::new(),
            qr_texture: None,
            reading: HashSet::new(),
            poll_schedule: Default::default(),
            last_state_poll: Instant::now(),
            last_discovery: Instant::now(),
            window_region:None,
            detail_geometry:false,
            backdrop_pending:false,
            main_position:None,
            content_motion:Default::default(),
            frame_motion:crate::motion::Sample{opacity:1.0,offset:0.0,active:false},
            error: None,
            notice: None,
            image_model: String::new(),
            image_url: String::new(),
            permission_url: String::new(),
            adapters,
            plugin_report: None,
            plugin_inspecting: false,
            plugin_importing: false,
            plugin_download_pending: HashMap::new(),
            plugin_download_results: HashMap::new(),
            plugin_catalog: Vec::new(),
            plugin_catalog_error: None,
            plugin_preparing: HashMap::new(),
            plugin_prepared: HashMap::new(),
            plugin_session: None,
            plugin_launch_generation:0,
            plugin_launching:false,
            mina,mina_login_rx,mina_connected,mina_login:LoginStatus::default(),mina_login_loading:false,
            mina_qr:None,mina_states:HashMap::new(),mina_reading:HashSet::new(),mina_pending:HashSet::new(),mina_generation:0,
        };
        #[cfg(debug_assertions)]
        if demo && std::env::args().any(|arg|arg=="--demo-speaker-service") {
            app.detail_tab=detail::DetailTab::Speaker;app.mina_connected=true;
            app.mina_states.insert("demo-speaker".into(),crate::speaker_service::Snapshot{at:crate::now_ms(),player_at:crate::now_ms(),sleep_at:crate::now_ms(),bluetooth_at:crate::now_ms(),status:Some(1),volume:Some(42),title:"小爱面板 · 演示音频".into(),artist:"测试数据，不连接真实设备".into(),duration_ms:Some(180000),position_ms:Some(32000),loop_type:Some(1),sleeping:Some(false),bluetooth:Some(false),..Default::default()});
        }
        app.refresh_plugin_catalog();
        if authenticated && !demo {
            app.refresh_devices();
        }
        #[cfg(debug_assertions)]
        if demo { app.install_lifecycle_probe(&cc.egui_ctx); }
        Ok(app)
    }

    fn material(&self) -> GlassStyle {
        self.preferences.material(self.appearance.glass_available)
    }

    fn refresh_devices(&mut self) {
        if self.demo {
            self.refreshing = false;
            self.notice("演示设备不会连接真实米家账号");
            return;
        }
        self.refreshing = true;
        self.last_discovery = Instant::now();
        let cloud = self.cloud.clone();
        let tx = self.events_tx.clone();
        let region = self.preferences.region.clone();
        let context = self.context.clone();
        self.runtime.spawn(async move {
            let result = async {
                cloud.discover(&region).await?;
                let homes = cloud.homes.lock().await.clone();
                let mut devices: Vec<_> = cloud.devices.lock().await.values().cloned().collect();
                devices.sort_by(|a, b| a.name.cmp(&b.name));
                Ok((homes, devices))
            }
            .await;
            let _ = tx.send(UiMessage::DevicesLoaded(result));
            context.request_repaint();
        });
    }

    fn read_device(&mut self, id: &str) {
        if self.demo || self.device_busy(id) || !self.reading.insert(id.to_owned()) {
            return;
        }
        let cloud = self.cloud.clone();
        let tx = self.events_tx.clone();
        let id = id.to_owned();
        let region = self.preferences.region.clone();
        let context = self.context.clone();
        self.runtime.spawn(async move {
            let result = cloud.read_device(&region, &id).await;
            let _ = tx.send(UiMessage::DeviceUpdated(id, crate::now_ms(), result));
            context.request_repaint();
        });
    }

    fn write_property(&mut self, id: &str, property: &Property, value: Value) {
        if self.mina_connected && property.key=="volume" {
            if let Some(device)=self.devices.iter().find(|device|device.id==id && crate::speaker_service::eligible(device)).cloned() {
                if let Some(volume)=value.as_u64().filter(|volume|*volume<=100){self.run_speaker_command(&device,crate::speaker_service::Command::Volume(volume as u8));}
                return;
            }
        }
        let key = format!("{id}:{}:{}", property.siid, property.piid);
        if self.device_busy(id) { return; }
        if self.demo {
            if let Some(mut device) = self.devices.iter().find(|device| device.id == id).cloned() {
                if let Some(target) = device
                    .properties
                    .iter_mut()
                    .find(|item| item.siid == property.siid && item.piid == property.piid)
                {
                    target.value = Some(value.clone());
                    target.error = None;
                    target.checked_at = Some(crate::now_ms());
                    target.source = crate::model::StateSource::Device;
                }
                self.pending.insert(key.clone());
                self.control_drafts.insert(key.clone(), value);
                let tx = self.events_tx.clone();
                let context = self.context.clone();
                self.runtime.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(650)).await;
                    let _ = tx.send(UiMessage::PropertyWritten(key, Ok(device)));
                    context.request_repaint();
                });
            }
            return;
        }
        if !self.pending.insert(key.clone()) {
            return;
        }
        self.control_drafts.insert(key.clone(), value.clone());
        let cloud = self.cloud.clone();
        let tx = self.events_tx.clone();
        let id = id.to_owned();
        let region = self.preferences.region.clone();
        let property = property.clone();
        let context = self.context.clone();
        self.runtime.spawn(async move {
            let result = cloud
                .write(&region, &id, property.siid, property.piid, value)
                .await;
            let _ = tx.send(UiMessage::PropertyWritten(key, result));
            context.request_repaint();
        });
    }

    fn run_action(&mut self, id: &str, action: &Action) {
        if self.mina_connected {
            let operation=match action.key.as_str(){"play"=>Some("play"),"pause"=>Some("pause"),"stop"=>Some("stop"),"previous"=>Some("prev"),"next"=>Some("next"),_=>None};
            if let Some(operation)=operation {
                if let Some(device)=self.devices.iter().find(|device|device.id==id && crate::speaker_service::eligible(device)).cloned(){self.run_speaker_command(&device,crate::speaker_service::Command::Play(operation));return;}
            }
        }
        let key = format!("action:{id}:{}:{}", action.siid, action.aiid);
        if self.demo {
            if self.device_busy(id) {return;}
            if let Some(mut device)=self.devices.iter().find(|device|device.id==id).cloned() {
                if let Some(state)=device.properties.iter_mut().find(|p|p.key=="playing-state") {
                    let expected=match action.key.as_str() {"play"=>"播放中","pause"=>"已暂停","stop"=>"已停止",_=>""};
                    if let Some(option)=state.options.iter().find(|o|crate::presentation::option_label(&o.name)==expected) {state.value=Some(option.value.clone());}
                }
                self.pending.insert(key.clone());
                let tx=self.events_tx.clone();
                let context=self.context.clone();
                self.runtime.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(650)).await;
                    let _=tx.send(UiMessage::ActionFinished(key,Ok(device)));
                    context.request_repaint();
                });
            }
            return;
        }
        if self.device_busy(id) || !self.pending.insert(key.clone()) {
            return;
        }
        let cloud = self.cloud.clone();
        let tx = self.events_tx.clone();
        let id = id.to_owned();
        let region = self.preferences.region.clone();
        let siid = action.siid;
        let aiid = action.aiid;
        let context = self.context.clone();
        self.runtime.spawn(async move {
            let result = cloud.action(&region, &id, siid, aiid).await;
            let _ = tx.send(UiMessage::ActionFinished(key, result));
            context.request_repaint();
        });
    }

    fn start_login(&mut self) {
        if self.demo {
            self.notice("演示模式 · 登录不会连接真实账号");
            return;
        }
        self.invalidate_mina_requests();
        self.error = None;
        let cloud = self.cloud.clone();
        let tx = self.events_tx.clone();
        self.runtime.spawn(async move {
            match cloud.begin_login().await {
                Ok((generation, url, client, jar)) => {
                    cloud.poll_login(generation, url, client, jar).await;
                }
                Err(error) => {
                    let _ = tx.send(UiMessage::DevicesLoaded(Err(error)));
                }
            }
        });
    }

    fn logout(&mut self) {
        self.plugin_session=None;
        self.confirm_logout = false;
        if self.demo {
            self.notice("演示模式 · 不会退出真实账号");
            return;
        }
        self.invalidate_mina_requests();
        self.mina_connected=false;self.mina_qr=None;
        let cloud = self.cloud.clone();
        let speaker=self.mina.auth.clone();
        let tx = self.events_tx.clone();
        self.runtime.spawn(async move {
            let result = match cloud.logout().await {Ok(())=>speaker.logout().await,Err(error)=>Err(error)};
            let _ = tx.send(UiMessage::LogoutFinished(result));
        });
    }

    fn cancel_login(&mut self) {
        self.invalidate_mina_requests();
        let cloud = self.cloud.clone();
        self.runtime.spawn(async move {
            let _ = cloud.cancel_login().await;
        });
        self.qr_texture = None;
    }

    fn load_images(&mut self) {
        if self.demo || self.preferences.approved_images.is_empty() {
            return;
        }
        for device in &self.devices {
            if self.textures.contains_key(&device.id) || !self.image_requests.insert(device.id.clone()) {
                continue;
            }
            let cloud = self.cloud.clone();
            let tx = self.events_tx.clone();
            let id = device.id.clone();
            let approved = self.preferences.approved_images.clone();
            self.runtime.spawn(async move {
                let result = cloud.image(&id, &approved).await;
                let _ = tx.send(UiMessage::ImageLoaded(id, result));
            });
        }
    }

    fn handle_messages(&mut self, ctx: &egui::Context) -> bool {
        let mut explicit_visibility=false;
        while let Ok(message) = self.events_rx.try_recv() {
            match message {
                #[cfg(debug_assertions)]
                UiMessage::DemoMaterial(material,dark,detail) if self.demo => {
                    self.preferences.glass=material!=GlassStyle::Solid;
                    self.preferences.glass_style=material;
                    self.preferences.theme=if dark {"dark"}else{"light"}.into();
                    self.dark=dark;apply_visuals(ctx,dark,true);
                    self.page=Page::Settings;
                    self.selected_device=detail.then(||"demo-light".into());
                    native::apply_window_appearance_hwnd(self.native_handle,material,dark);
                    self.window_region=None;ctx.request_repaint();
                }
                #[cfg(debug_assertions)]
                UiMessage::DemoMaterial(..)=>{},
                UiMessage::MinaLoginError(epoch,generation,error)=>{
                    if !speaker_event_current(self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst),self.mina_generation,epoch,generation) {continue;}
                    self.invalidate_mina_requests();
                    self.mina_login_loading=false;self.mina_connected=false;
                    self.mina_login=LoginStatus{stage:"error".into(),message:error,..Default::default()};
                }
                UiMessage::MinaRead(id,epoch,generation,started_at,result)=>{
                    if !speaker_event_current(self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst),self.mina_generation,epoch,generation) {continue;}
                    if self.mina_states.get(&id).is_some_and(|state|state.at>started_at) {continue;}
                    self.mina_reading.remove(&id);
                    if self.mina_states.get(&id).is_none_or(|state|state.at<=started_at) {
                        let previous=self.mina_states.get(&id).cloned().unwrap_or_default();
                        let state=match result {Ok(state)=>state.retain_last(&previous),Err(error)=>crate::speaker_service::Snapshot::read_failure(&previous,error)};
                        discard_invalid_speaker_drafts(&id,&state,&self.mina_pending,&mut self.control_drafts);
                        self.mina_states.insert(id,state);
                    }
                }
                UiMessage::MinaWritten(id,epoch,generation,result)=>{
                    if !speaker_event_current(self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst),self.mina_generation,epoch,generation) {continue;}
                    // No new read can start while this device is pending. Any
                    // remaining marker belongs to a read from before the write.
                    self.mina_reading.remove(&id);
                    self.mina_pending.remove(&id);
                    self.control_drafts.retain(|key,_|!key.starts_with(&format!("mina:{id}:")));
                    match result {
                        Ok(state)=>{self.mina_states.insert(id,state);self.notice("音箱已回读确认");},
                        Err(error)=>{
                            if let Some(state)=self.mina_states.get_mut(&id){*state=state.pending_refresh();}
                            self.report_error(error);
                            // Recover with a read, never repeat the device write.
                            if self.authenticated {if let Some(device)=self.devices.iter().find(|device|device.id==id).cloned(){self.request_speaker_state(&device);}}
                        },
                    }
                }
                UiMessage::PluginImported(result) => {
                    self.plugin_importing=false;
                    match result {Ok(())=>self.notice("本机 APK 公开验签根已导入，可校验已下载插件"),Err(error)=>self.report_error(error)}
                }
                UiMessage::PluginDownloaded(model,epoch,result)=>{
                    if self.plugin_download_pending.get(&model)!=Some(&epoch){continue;}
                    self.plugin_download_pending.remove(&model);
                    if self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst)!=epoch || !self.authenticated {continue;}
                    match &result {Ok(report)=>self.notice(format!("插件 v{} 已下载，已加入设置中的「已下载插件」",report.version)),Err(error)=>self.report_error(error.clone())}
                    self.plugin_download_results.insert(model,result);
                    self.refresh_plugin_catalog();
                }
                UiMessage::PluginPrepared(model,epoch,launch,result)=>{
                    if self.plugin_preparing.get(&model)!=Some(&epoch){continue;}
                    self.plugin_preparing.remove(&model);
                    if self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst)!=epoch || !self.authenticated {continue;}
                    let succeeded=result.is_ok();
                    match &result {Ok(_)=>self.notice("验签与运行文件检查通过，插件已准备"),Err(error)=>self.report_error(error.clone())}
                    self.plugin_prepared.insert(model.clone(),result);
                    self.refresh_plugin_catalog();
                    if succeeded {if let Some((device,writes,report))=launch {self.open_downloaded_plugin(&device,writes,&report);}}
                }
                UiMessage::PluginInspected(result) => {
                    self.plugin_inspecting=false;
                    match result { Ok(report)=>self.plugin_report=Some(report), Err(error)=>self.report_error(error) }
                }
                UiMessage::PluginStarted(epoch,generation,result)=>{
                    if generation!=self.plugin_launch_generation {continue;}
                    self.plugin_launching=false;
                    if self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst)!=epoch||!self.authenticated {continue;}
                    match result {Ok(session)=>{self.plugin_session=Some(session);self.notice("官方插件窗口已启动，状态与可用功能以窗口内反馈为准");},Err(error)=>self.report_error(error)}
                }
                UiMessage::TrayToggle(anchor) => {explicit_visibility=true;self.toggle_panel(ctx, anchor);}
                UiMessage::TraySettings => {
                    explicit_visibility=true;
                    self.page = Page::Settings;
                    self.show_panel(ctx, None);
                }
                UiMessage::Quit => {explicit_visibility=true;self.request_quit(ctx);}
                UiMessage::DevicesLoaded(result) => {
                    self.refreshing = false;
                    self.loading = false;
                    match result {
                        Ok((homes, mut devices)) => {
                            for device in &mut devices {
                                if let Some(current) = self.devices.iter().find(|d| d.id == device.id) {
                                    let mut merged = current.clone();
                                    crate::state_sync::merge(&mut merged, device.clone());
                                    *device = merged;
                                }
                            }
                            self.homes = homes;
                            self.devices = devices;
                            self.authenticated = true;
                            if !self.homes.iter().any(|home| Some(home.id.as_str()) == self.selected_home.as_deref()) {
                                self.selected_home = self.homes.first().map(|home| home.id.clone());
                            }
                            self.load_images();
                            // Routine online/list refreshes must not obscure detail status.
                        }
                        Err(error) => self.report_error(error),
                    }
                }
                UiMessage::DeviceUpdated(id, completed_at, result) => {
                    let direct = result.as_ref().is_ok_and(crate::state_sync::direct);
                    match result {
                        Ok(device) => {
                            if let Some(item) = self.devices.iter_mut().find(|item| item.id == device.id) {
                                crate::state_sync::merge(item, device);
                            }
                        }
                        Err(error) => {
                            if let Some(device) = self.devices.iter_mut().find(|d| d.id == id) {
                                let targets: Vec<_> = device.properties.iter().filter(|p| p.readable && p.checked_at.is_none_or(|at| at <= completed_at)).map(|p| (p.siid,p.piid)).collect();
                                crate::state_sync::apply(device, &targets, &Err(error.clone()), &Ok(json!([])), completed_at);
                            }
                            self.report_error(error);
                        }
                    }
                    self.reading.remove(&id);
                    self.poll_schedule.completed(&id, crate::now_ms(), direct);
                }
                UiMessage::PropertyWritten(key, result) => {
                    let id = result.as_ref().ok().map(|device| device.id.clone());
                    match finish_property_write(&mut self.devices, &mut self.pending, &mut self.control_drafts, &key, result) {
                        Ok(()) => { if self.demo { self.notice("演示模式 · 未控制真实设备"); } }
                        Err(error) => self.report_error(error),
                    }
                    // Target-position and current-position can be different properties.
                    if let Some(id) = id { self.read_device(&id); }
                }
                UiMessage::ActionFinished(key, result) => {
                    self.pending.remove(&key);
                    match result {
                        Ok(device) => {
                            if let Some(item) = self.devices.iter_mut().find(|item| item.id == device.id) {
                                crate::state_sync::merge(item, device);
                            }
                            self.notice("指令已受理，状态以设备回读为准");
                        }
                        Err(error) => self.report_error(error),
                    }
                }
                UiMessage::ImageLoaded(id, result) => match result {
                    Ok(Some(data)) => {
                        if let Some(texture) = decode_texture(ctx, &format!("device:{id}"), &data) {
                            self.textures.insert(id, texture);
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        self.image_requests.remove(&id);
                    }
                },
                UiMessage::LogoutFinished(result) => match result {
                    Ok(()) => {
                        self.authenticated = false;
                        self.devices.clear();
                        self.pending.clear();
                        self.reading.clear();
                        self.poll_schedule.forget();
                        self.control_drafts.clear();
                        self.homes.clear();
                        self.selected_device = None;
                        self.page = Page::Login;
                        self.login = LoginStatus::default();
                        self.notice("已退出 HomePanel 的本机会话");
                    }
                    Err(error) => self.report_error(error),
                },
            }
        }
        explicit_visibility
    }

    fn handle_login_updates(&mut self) {
        if self.mina_login_rx.has_changed().unwrap_or(false) {
            self.mina_login=self.mina_login_rx.borrow_and_update().clone();
            self.mina_login_loading=false;
            match self.mina_login.stage.as_str() {
                "success"=>{self.invalidate_mina_requests();self.mina_connected=true;self.mina_qr=None;self.notice("小爱服务已连接，将按设备身份读取状态");},
                "error"|"expired"|""=>{self.invalidate_mina_requests();self.mina_connected=false;self.mina_qr=None;},
                _=>{},
            }
        }
        if self.login_rx.has_changed().unwrap_or(false) {
            self.login = self.login_rx.borrow_and_update().clone();
            match self.login.stage.as_str() {
                "success" => {
                    self.invalidate_mina_requests();
                    self.authenticated = true;
                    self.page = Page::Dashboard;
                    self.refresh_devices();
                    self.notice("已连接米家账号");
                }
                "error" | "expired" => {self.invalidate_mina_requests();self.error = Some((self.login.message.clone(), Instant::now()));},
                _ => {}
            }
        }
    }

    fn report_error(&mut self, error: String) {
        if error.starts_with("AUTH_REQUIRED") {
            self.invalidate_mina_requests();
            self.pending.clear();
            self.reading.clear();
            self.poll_schedule.forget();
            self.control_drafts.clear();
            self.authenticated = false;
            self.page = Page::Login;
            self.selected_device = None;
        }
        let message = error
            .strip_prefix("AUTH_REQUIRED: ")
            .unwrap_or(&error)
            .to_owned();
        self.error = Some((message, Instant::now()));
    }

    fn persist_preferences(&mut self) {
        if self.demo {
            self.notice("演示模式 · 设置不会写入真实配置");
            return;
        }
        let bytes = match serde_json::to_vec_pretty(&self.preferences) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.report_error("无法保存设置".into());
                return;
            }
        };
        match storage::atomic_write(&self.root.join("preferences.json"), &bytes) {
            Ok(()) => self.notice("设置已保存"),
            Err(error) => self.report_error(error),
        }
    }

    fn notice(&mut self, message: impl Into<String>) {
        self.notice = Some((message.into(), Instant::now()));
    }

    fn show_panel(&mut self, ctx: &egui::Context, anchor: Option<[f64; 2]>) {
        self.content_motion.restart();
        self.dismiss_guard.reset();
        let scale = ctx.pixels_per_point();
        let position = native::panel_position(anchor, scale, PANEL_SIZE);
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(position[0], position[1])));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.visible = true;
        self.poll_schedule.forget();
    }

    fn request_quit(&mut self, ctx: &egui::Context) {
        self.plugin_session=None;
        self.quitting = true;
        if !native::request_window_exit(self.native_handle) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx.request_repaint();
        }
    }

    fn open_official_plugin(&mut self, device: &Device, writes: bool) {
        if self.demo {self.notice("演示模式不能启动真实设备插件");return;}
        self.plugin_session=None;
        match crate::plugin_bridge::launch(self.cloud.clone(),device.clone(),self.preferences.region.clone(),&self.root,writes,self.context.clone()) {
            Ok(session)=>{self.plugin_session=Some(session);self.notice(if writes {"已开启插件控制；修改仍需设备回读确认"}else{"已开启插件只读测试"});},
            Err(error)=>self.report_error(error),
        }
        self.dismiss_guard.reset();
    }

    fn hide_panel(&mut self, ctx: &egui::Context) {
        self.dismiss_guard.reset();
        self.selected_device = None;
        self.discard_unsubmitted_drafts();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.visible = false;
    }

    fn toggle_panel(&mut self, ctx: &egui::Context, anchor: Option<[f64; 2]>) {
        if self.visible {
            self.hide_panel(ctx);
        } else {
            self.show_panel(ctx, anchor);
        }
    }

    fn discard_unsubmitted_drafts(&mut self) {
        let pending=&self.pending;
        let mina_pending=&self.mina_pending;
        self.control_drafts.retain(|key,_| pending.contains(key) || mina_pending.iter().any(|id|key.starts_with(&format!("mina:{id}:"))));
    }

    fn invalidate_mina_requests(&mut self) {
        invalidate_speaker_requests(&mut self.mina_generation,&mut self.mina_reading,&mut self.mina_pending,&mut self.mina_states,&mut self.control_drafts);
    }
    fn uses_speaker_service(&self,device:&Device)->bool {self.mina_connected && crate::speaker_service::eligible(device)}

    fn dark_for_preferences(&self) -> bool {
        theme_is_dark(&self.preferences, self.appearance)
    }

    fn draw_login(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui|{
            ui.label(RichText::new("H O M E P A N E L").size(9.5).color(muted(self.dark)));
            ui.with_layout(Layout::right_to_left(Align::Center),|ui|{
                if design::icon_button(ui,Icon::Close,"收起到托盘",self.dark).clicked(){self.hide_panel(ctx);}
                if design::icon_button(ui,Icon::Settings,"外观设置",self.dark).clicked(){self.page=Page::Settings;}
            });
        });
        let waiting=self.login.stage=="waiting";
        ui.vertical_centered(|ui|{
            ui.add_space(if waiting{10.}else{52.});
            brand_mark(ui,if waiting{48.}else{64.});
            ui.add_space(16.);
            ui.label(RichText::new(if waiting{"扫描二维码连接"}else{"你的家，一点就到"}).size(25.).strong());
            ui.add_space(4.);
            ui.label(RichText::new("在桌面，轻松掌控米家设备").size(12.).color(muted(self.dark)));
            ui.add_space(24.);
            if waiting{
                if let Some(qr)=self.login.qr.as_deref(){
                    if self.qr_texture.as_ref().is_none_or(|(last,_)|last!=qr){self.qr_texture=decode_texture(ctx,"login-qr",qr).map(|texture|(qr.to_owned(),texture));}
                }
                egui::Frame::new().fill(Color32::WHITE).corner_radius(20).inner_margin(12.)
                    .stroke(Stroke::new(1.0_f32,design::line(self.dark))).show(ui,|ui|{
                        if let Some((_,texture))=&self.qr_texture{ui.add(egui::Image::new((texture.id(),Vec2::splat(196.))));}
                        else{ui.allocate_ui(Vec2::splat(196.),|ui|{ui.centered_and_justified(|ui|ui.spinner());});}
                    });
                ui.add_space(12.);
                ui.label(RichText::new("使用米家 App 扫码，在手机上确认").size(12.));
                if let Some(expires_at)=self.login.expires_at{
                    ui.label(RichText::new(format!("有效期约 {} 秒",expires_at.saturating_sub(crate::now_ms())/1000)).size(10.5).color(muted(self.dark)));
                }
                ui.add_space(6.);
                ui.horizontal(|ui|{
                    ui.add_space(100.);
                    if ui.button("刷新二维码").clicked(){self.start_login();}
                    if ui.button("取消").clicked(){self.cancel_login();}
                });
            }else{
                // Decorative artwork is ours; no example devices or simulated state on the login page.
                ui.horizontal(|ui|{
                    ui.add_space((ui.available_width()-208.)/2.);
                    for kind in ["light","fan","sensor"]{
                        let (rect,_)=ui.allocate_exact_size(Vec2::splat(64.),Sense::hover());
                        design::device_art(ui.painter(),rect,kind,self.dark);
                    }
                });
                ui.add_space(26.);
                let button=egui::Button::new(RichText::new("扫码连接米家").size(14.).strong().color(Color32::WHITE))
                    .fill(ACCENT).stroke(Stroke::NONE).corner_radius(14).min_size(Vec2::new(250.,46.));
                if ui.add(button).clicked(){self.start_login();}
                ui.add_space(10.);
                ui.label(RichText::new("打开米家 App，扫码即可登录").size(11.).color(muted(self.dark)));
                if self.login.stage=="error"||self.login.stage=="expired"{
                    ui.add_space(8.);ui.label(RichText::new(&self.login.message).size(11.).color(Color32::from_rgb(184,92,75)));
                }
            }
            ui.add_space(if waiting{10.}else{35.});
            ui.label(RichText::new("Windows 加密保护 · 会话仅保存在本机").size(10.5).color(muted(self.dark)));
            ui.add_space(5.);
            ui.label(RichText::new("独立社区客户端").size(9.5).color(muted(self.dark)));
        });
    }

    fn draw_dashboard(&mut self, ui: &mut egui::Ui) {
        self.draw_header(ui);
        ui.add_space(10.0);
        self.draw_overview(ui);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.set_height(30.0);
            if self.search_open {
                let width = (ui.available_width() - 38.0).max(80.0);
                ui.add_sized([width, 30.0], egui::TextEdit::singleline(&mut self.search_query)
                    .id(Id::new("device-search")).hint_text("搜索设备名称或房间").margin(egui::vec2(10.0, 6.0)));
                if design::icon_button(ui, Icon::Close, "关闭搜索", self.dark).clicked() {
                    self.search_open = false;
                    self.search_query.clear();
                }
            } else {
                ui.label(RichText::new("我的设备").size(15.0).strong());
                ui.label(RichText::new(format!("{:02}", self.filtered_devices().len())).size(12.0).color(muted(self.dark)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if design::icon_button(ui, Icon::Search, "搜索设备", self.dark).clicked() { self.search_open = true; ui.memory_mut(|m| m.request_focus(Id::new("device-search"))); }
                });
            }
        });
        ui.add_space(2.0);
        self.draw_room_filter(ui);
        ui.add_space(10.0);
        let devices = self.filtered_devices();
        let list_max_height = (ui.available_height() - 35.0).max(100.0);
        egui::ScrollArea::vertical().id_salt("device-list").max_height(list_max_height)
            .auto_shrink([false, false]).show(ui, |ui| {
                if devices.is_empty() { self.draw_empty(ui); }
                else {
                    for (row_index, row) in devices.chunks(2).enumerate() {
                        ui.columns(2, |columns| {
                            self.draw_device_card(&mut columns[0], &row[0]);
                            if let Some(device) = row.get(1) { self.draw_device_card(&mut columns[1], device); }
                        });
                        if row_index + 1 < devices.len().div_ceil(2) { ui.add_space(10.0); }
                    }
                }
            });
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.set_height(26.0);
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 2.5, if self.demo { Color32::from_rgb(191,147,65) } else { design::accent(self.dark) });
            ui.label(RichText::new(if self.demo { "演示预览 · 仅展示模拟设备" } else if self.refreshing { "正在同步设备状态…" } else { "已连接米家服务" }).size(10.5).color(muted(self.dark)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.refreshing { ui.add(egui::Spinner::new().size(16.0)); }
                else if design::icon_button(ui, Icon::Refresh, "刷新全部设备", self.dark).clicked() { self.refresh_devices(); }
            });
        });
    }

    fn draw_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.set_height(44.0);
            brand_mark(ui, 40.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new("H O M E P A N E L").size(9.0).color(muted(self.dark)));
                let selected = self.homes.iter()
                    .find(|home| Some(home.id.as_str()) == self.selected_home.as_deref())
                    .map(|home| home.name.as_str()).unwrap_or("我的家");
                egui::ComboBox::from_id_salt("home-select").selected_text(RichText::new(selected).size(20.0).strong())
                    .width(142.0).show_ui(ui, |ui| {
                        for home in &self.homes {
                            if ui.selectable_value(&mut self.selected_home, Some(home.id.clone()), &home.name).changed() {
                                self.selected_room = "全部".into();
                            }
                        }
                    });
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if design::icon_button(ui, Icon::Close, "收起到托盘", self.dark).clicked() { self.hide_panel(ui.ctx()); }
                if design::icon_button(ui, Icon::Settings, "设置", self.dark).clicked() { self.page = Page::Settings; }
            });
        });
    }

    fn draw_overview(&mut self, ui: &mut egui::Ui) {
        let devices=self.activity_candidates();
        let online=devices.iter().filter(|d|d.online).count();
        let running=devices.iter().filter(|d|crate::overview::activity(d,crate::now_ms())==crate::overview::Activity::On).count();
        let rooms=devices.iter().map(|d|d.room.as_str()).filter(|r|!r.is_empty()).collect::<HashSet<_>>().len();
        let offline=devices.len().saturating_sub(online);
        if let Some(filter)=activity::overview_card(ui,[online,running,rooms,offline],self.dark) {
            self.activity_filter=filter;
            self.page=Page::Activity;
            self.poll_schedule.forget();
        }
    }

    fn draw_room_filter(&mut self, ui: &mut egui::Ui) {
        let mut rooms = vec!["全部".to_owned()];
        if !self.preferences.favorites.is_empty() { rooms.push("常用".into()); }
        let mut home_rooms: Vec<String> = self.homes.iter()
            .find(|home| Some(home.id.as_str()) == self.selected_home.as_deref())
            .map(|home| home.rooms.iter().map(|room| room.name.clone()).collect()).unwrap_or_default();
        if home_rooms.is_empty() {
            home_rooms = self.devices.iter().filter(|d| self.device_in_selected_home(d))
                .map(|d| d.room.clone()).filter(|room| !room.is_empty()).collect();
        }
        home_rooms.sort(); home_rooms.dedup(); rooms.extend(home_rooms);
        if !rooms.contains(&self.selected_room) { self.selected_room = "全部".into(); }
        egui::ScrollArea::horizontal().id_salt("room-tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for room in rooms {
                    let count = self.devices.iter().filter(|d| self.device_in_selected_home(d) && !self.preferences.hidden_devices.contains(&d.id))
                        .filter(|d| room_count_matches(&room,d,&self.preferences.favorites)).count();
                    let selected = self.selected_room == room;
                    let color=if selected { if self.dark {Color32::from_rgb(27,43,33)} else {Color32::WHITE} } else {muted(self.dark)};
                    let button=egui::Button::new(RichText::new(format!("{room}  {count}")).size(11.0).color(color))
                        .fill(if selected {design::accent(self.dark)} else {Color32::TRANSPARENT})
                        .stroke(Stroke::NONE).corner_radius(20).min_size(Vec2::new(54.,27.));
                    if ui.add(button).clicked() {self.selected_room = room;}
                }
            });
        });
    }

    fn draw_empty(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(58.0);
            brand_mark(ui, 46.0);
            ui.add_space(12.0);
            ui.label(RichText::new(if !self.search_query.trim().is_empty() { "没有找到匹配设备" } else if self.loading { "正在读取设备" } else { "这里还没有设备" }).size(18.0).strong());
            ui.label(RichText::new(if !self.search_query.trim().is_empty() { "试试其他设备名称或房间" } else if self.selected_room == "常用" { "右键设备卡片即可收藏" } else { "请在米家 App 中添加设备后刷新" }).size(12.0).color(muted(self.dark)));
            ui.add_space(8.0);
            if !self.loading && ui.button("重新同步").clicked() {
                self.refresh_devices();
            }
            if self.preferences.hidden_devices.len() > 0 {
                if ui.small_button("管理隐藏设备").clicked() {
                    self.page = Page::Settings;
                }
            }
        });
    }

    fn draw_device_card(&mut self, ui: &mut egui::Ui, device: &Device) {
        let width = ui.available_width().clamp(140.0, 218.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 140.0), Sense::hover());
        let id = Id::new(("device-card", device.id.as_str()));
        let response = ui.interact(rect,id,Sense::click());
        let hover=ui.ctx().animate_bool(id.with("hover"),response.hovered());
        let on=power_property(device).filter(|p|p.error.is_none()).and_then(|p|p.value.as_ref()).and_then(Value::as_bool)==Some(true);
        let highlighted=on && crate::overview::activity(device,crate::now_ms())==crate::overview::Activity::On;
        let p=ui.painter();
        let active_fill=if self.dark {Color32::from_rgb(38,57,44)} else {Color32::from_rgb(242,250,245)};
        let fill=if highlighted {active_fill} else {design::surface(self.dark)};
        let border=if highlighted {design::accent(self.dark).gamma_multiply(0.3)} else {design::line(self.dark)};
        let fill = if self.material() == GlassStyle::Solid { fill } else {
            let [r,g,b,_] = fill.to_array();
            Color32::from_rgba_unmultiplied(r,g,b,if self.material() == GlassStyle::Glass { 105 } else { 175 })
        };
        p.rect_filled(rect,20,fill);
        p.rect_stroke(rect,20,Stroke::new(1.0_f32,border.lerp_to_gamma(design::accent(self.dark).gamma_multiply(0.6),hover)),egui::StrokeKind::Inside);
        let art=egui::Rect::from_min_size(rect.min+egui::vec2(13.,12.),Vec2::splat(50.));
        if let Some(texture)=self.textures.get(&device.id) {
            p.image(texture.id(),art,egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.,1.)),Color32::WHITE);
        } else { design::device_art(p,art,&device.kind,self.dark); }
        if self.preferences.favorites.contains(&device.id) {
            design::icon(p,rect.min+egui::vec2(75.,20.),Icon::Star,13.,Color32::from_rgb(192,156,78));
        }
        design::text(p,rect.min+egui::vec2(13.,73.),&device.name,14.,self.text_color(),width-26.);
        design::text(p,rect.min+egui::vec2(13.,96.),if device.room.is_empty(){"未分配房间"}else{&device.room},10.5,muted(self.dark),width-68.);
        let busy=self.device_busy(&device.id);
        let state=if busy {"正在同步…".into()} else if !device.online {"设备离线".into()}
            else if let Some(property)=power_property(device) {let text=match property.value.as_ref().and_then(Value::as_bool){Some(true)=>"已开启",Some(false)=>"已关闭",None=>"状态未读取"};crate::presentation::reported_text(property,text,crate::now_ms())} else {device_summary(device)};
        design::text(p,rect.min+egui::vec2(13.,117.),&state,10.5,
            if !device.online {Color32::from_rgb(184,117,77)} else if highlighted {design::accent(self.dark)} else {muted(self.dark)},width-70.);
        if device.online && on {
            let info=device.properties.iter().find(|p|p.key=="brightness").and_then(|p|p.value.as_ref()).and_then(Value::as_u64).map(|value|format!("{value}%"))
                .or_else(||device.properties.iter().find(|p|p.key=="fan-level").map(property_value));
            if let Some(info)=info {design::text(ui.painter(),rect.min+egui::vec2(76.,43.),&info,13.,design::accent(self.dark),width-118.);}
        }
        let menu_rect=egui::Rect::from_min_size(egui::pos2(rect.right()-39.,rect.top()+9.),Vec2::splat(28.));
        let menu_response=ui.interact(menu_rect,id.with("more"),Sense::click()).on_hover_text("设备操作");
        design::icon_button_at(ui,&menu_response,menu_rect,Icon::More,self.dark);
        let mut intent=None;
        egui::Popup::menu(&menu_response).frame(design::popup_frame(self.dark)).show(|ui| {intent=self.device_menu(ui,device);});
        egui::Popup::context_menu(&response).frame(design::popup_frame(self.dark)).show(|ui| {intent=self.device_menu(ui,device);});
        let mut power_clicked=false;
        if let Some(property)=power_property(device) {
            let available=device.online && !busy && property.error.is_none() && property.value.as_ref().and_then(Value::as_bool).is_some();
            let power_rect=egui::Rect::from_center_size(egui::pos2(rect.right()-30.,rect.bottom()-28.),Vec2::splat(33.));
            power_clicked=design::power_button(ui,id.with("power"),power_rect,on,available,busy,self.dark).clicked();
        }
        if power_clicked {intent=Some(CardIntent::Power);}
        else if response.clicked() && !menu_response.clicked() {intent=Some(CardIntent::Details);}
        match intent {
            Some(CardIntent::Details)=>{self.selected_device=Some(device.id.clone());self.detail_tab=Default::default();self.read_device(&device.id);}
            Some(CardIntent::Power)=>{if let Some(property)=power_property(device).cloned(){
                if let Some(value)=property.value.as_ref().and_then(Value::as_bool){self.write_property(&device.id,&property,json!(!value));}
            }}
            Some(CardIntent::Favorite)=>self.toggle_favorite(&device.id),
            Some(CardIntent::Refresh)=>{self.read_device(&device.id);if self.demo{self.notice("演示模式 · 未读取真实设备");}}
            Some(CardIntent::Hide)=>self.confirm_hide=Some(device.id.clone()),
            None=>{}
        }
    }

    fn device_menu(&self, ui: &mut egui::Ui, device: &Device) -> Option<CardIntent> {
        ui.set_width(216.);
        ui.spacing_mut().item_spacing.y=2.;
        ui.add(egui::Label::new(RichText::new(&device.name).size(11.5).strong().color(muted(self.dark))).truncate());
        ui.add_space(5.);
        let mut action=None;
        if design::menu_item(ui,Icon::Detail,"设备详情",self.dark,true).clicked(){action=Some(CardIntent::Details);}
        if let Some(property)=power_property(device) {
            let value=property.value.as_ref().and_then(Value::as_bool);
            let enabled=device.online && !self.device_busy(&device.id) && property.error.is_none() && value.is_some();
            if design::menu_item(ui,Icon::Power,if value==Some(true){"关闭设备"}else{"打开设备"},self.dark,enabled).clicked() && enabled {action=Some(CardIntent::Power);}
        }
        ui.separator();
        if design::menu_item(ui,Icon::Star,if self.preferences.favorites.contains(&device.id){"取消收藏"}else{"收藏到常用"},self.dark,true).clicked(){action=Some(CardIntent::Favorite);}
        if design::menu_item(ui,Icon::Refresh,"刷新设备状态",self.dark,true).clicked(){action=Some(CardIntent::Refresh);}
        ui.separator();
        if design::menu_item(ui,Icon::Hide,"仅从 HomePanel 隐藏…",self.dark,true).clicked(){action=Some(CardIntent::Hide);}
        design::menu_item(ui,Icon::Lock,"从米家移除 · 暂不可用",self.dark,false)
            .on_hover_text("社区接口尚未验证家庭解绑；隐藏只影响本面板，可在设置恢复");
        if action.is_some(){ui.close();}
        action
    }

    fn draw_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if design::icon_button(ui,Icon::Back,"返回设备",self.dark).clicked(){self.page=if self.authenticated{Page::Dashboard}else{Page::Login};}
            ui.vertical(|ui|{
                ui.label(RichText::new("偏好设置").size(21.).strong());
                ui.label(RichText::new("把家的控制，留在手边").size(10.5).color(muted(self.dark)));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("退出程序").on_hover_text("退出 HomePanel，保留登录状态和设置").clicked() {
                    self.request_quit(ctx);
                }
            });
        });
        ui.add_space(12.);
        let dark=self.dark;
        let mut changed=false;
        let mut region_changed=false;
        egui::ScrollArea::vertical().id_salt("settings-scroll").auto_shrink([false,false]).show(ui,|ui|{
            section_title(ui,"外观",dark);
            design::settings_frame(dark, self.material()).show(ui,|ui|{
                ui.set_width(ui.available_width());
                ui.label(RichText::new("主题").size(12.).strong());
                ui.add_space(3.);
                ui.columns(3,|columns|{
                    for (column,(value,label)) in columns.iter_mut().zip([("system","跟随系统"),("light","浅色"),("dark","深色")]){
                        let active=self.preferences.theme==value;
                        let button=egui::Button::new(RichText::new(label).size(11.5).color(if active{design::accent(dark)}else{muted(dark)}))
                            .fill(if active{if dark{Color32::from_rgb(45,70,54)}else{Color32::from_rgb(227,243,233)}}else{Color32::TRANSPARENT})
                            .stroke(Stroke::new(1.0_f32,if active{design::accent(dark).gamma_multiply(0.35)}else{design::line(dark)})).corner_radius(10)
                            .min_size(Vec2::new(column.available_width(),34.));
                        if column.add(button).clicked(){self.preferences.theme=value.into();changed=true;}
                    }
                });
                ui.add_space(8.);
                ui.separator();
                ui.add_space(4.);
                ui.label(RichText::new("背景材质").size(12.).strong());
                ui.add_space(5.);
                let selected = self.preferences.material(true);
                ui.columns(3, |columns| {
                    for (column, (value, label)) in columns.iter_mut().zip([
                        (GlassStyle::Glass, "通透玻璃"), (GlassStyle::Frosted, "毛玻璃"), (GlassStyle::Solid, "纯色")
                    ]) {
                        let active = selected == value;
                        let button = egui::Button::new(RichText::new(label).size(11.5).color(if active { design::accent(dark) } else { muted(dark) }))
                            .fill(if active { design::accent(dark).gamma_multiply(0.12) } else { Color32::TRANSPARENT })
                            .stroke(Stroke::new(1.0_f32, if active { design::accent(dark).gamma_multiply(0.5) } else { design::line(dark) }))
                            .corner_radius(11).min_size(Vec2::new(column.available_width(), 36.));
                        if column.add_enabled(self.appearance.glass_available || value == GlassStyle::Solid, button).clicked() {
                            self.preferences.glass = value != GlassStyle::Solid;
                            if value != GlassStyle::Solid { self.preferences.glass_style = value; }
                            changed = true;
                        }
                    }
                });
                ui.add_space(4.);
                ui.label(RichText::new(if !self.appearance.glass_available { "系统已关闭透明，当前使用纯色面板" }
                    else { match selected { GlassStyle::Glass => "透出桌面细节，轻薄高光边缘", GlassStyle::Frosted => "模糊桌面背景，柔和且易读", GlassStyle::Solid => "不透明背景，文字更清晰" } }).size(10.).color(muted(dark)));
            });

            section_title(ui,"连接与启动",dark);
            design::settings_frame(dark, self.material()).show(ui,|ui|{
                ui.set_width(ui.available_width());
                ui.horizontal(|ui|{
                    ui.label(RichText::new("米家服务地区").size(12.).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center),|ui|{
                        egui::ComboBox::from_id_salt("region").width(120.).selected_text(region_label(&self.preferences.region)).show_ui(ui,|ui|{
                            for (value,label) in [("cn","中国大陆"),("tw","台湾"),("sg","新加坡"),("de","欧洲"),("us","美国"),("ru","俄罗斯"),("i2","印度")]{
                                if ui.selectable_value(&mut self.preferences.region,value.into(),label).changed(){changed=true;region_changed=true;}
                            }
                        });
                    });
                });
                ui.add_space(4.);ui.separator();ui.add_space(4.);
                ui.horizontal(|ui|{
                    ui.label(RichText::new("开机驻留托盘").size(12.).strong());
                    let mut autostart=self.preferences.autostart;
                    ui.with_layout(Layout::right_to_left(Align::Center),|ui|{
                        ui.add_enabled_ui(!self.demo,|ui|{
                            if design::toggle(ui,&mut autostart).changed(){match native::set_autostart(autostart){Ok(())=>{self.preferences.autostart=autostart;changed=true;},Err(error)=>self.report_error(error)}}
                        });
                    });
                });
                ui.label(RichText::new(if self.demo{"演示模式不会修改 Windows 启动项"}else{"登录 Windows 后自动启动"}).size(10.).color(muted(dark)));
            });

            section_title(ui,"设备管理",dark);
            design::settings_frame(dark, self.material()).show(ui,|ui|{
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("隐藏设备  ·  {}",self.preferences.hidden_devices.len())).size(12.).strong());
                ui.label(RichText::new("只影响本面板，可随时恢复").size(10.).color(muted(dark)));
                for id in self.preferences.hidden_devices.clone(){
                    let name=self.devices.iter().find(|d|d.id==id).map(|d|d.name.clone()).unwrap_or_else(||"已移除的设备".into());
                    ui.horizontal(|ui|{
                        ui.add(egui::Label::new(RichText::new(name).size(12.)).truncate());
                        ui.with_layout(Layout::right_to_left(Align::Center),|ui|{
                            if ui.button("恢复").clicked(){self.preferences.hidden_devices.retain(|x|x!=&id);changed=true;}
                        });
                    });
                }
            });
            ui.add_space(12.);
            design::settings_frame(dark, self.material()).show(ui,|ui|{
                ui.set_width(ui.available_width());
                egui::CollapsingHeader::new(RichText::new("设备图片来源 · 高级").size(12.)).show(ui,|ui|{
                    ui.label(RichText::new("只添加获准使用的官方图片。公开可访问不代表已获授权。").size(10.).color(muted(dark)));
                    for source in self.preferences.approved_images.clone(){
                        ui.horizontal(|ui|{
                            ui.label(source.model.clone());
                            if ui.small_button("移除来源").clicked(){
                                self.preferences.approved_images.retain(|x|x.model!=source.model);
                                self.textures.clear();self.image_requests.clear();changed=true;
                            }
                        });
                    }
                    let width=ui.available_width();
                    ui.add(egui::TextEdit::singleline(&mut self.image_model).hint_text("设备型号，例如 yeelink.light.ceiling").desired_width(width));
                    ui.add(egui::TextEdit::singleline(&mut self.image_url).hint_text("官方 HTTPS 图片地址").desired_width(width));
                    ui.add(egui::TextEdit::singleline(&mut self.permission_url).hint_text("图片使用许可依据地址").desired_width(width));
                    if ui.add_enabled(!self.image_model.trim().is_empty()&&!self.image_url.trim().is_empty()&&!self.permission_url.trim().is_empty(),egui::Button::new("登记图片来源")).clicked(){
                        let permission_valid=url::Url::parse(self.permission_url.trim()).is_ok_and(|url|url.scheme()=="https");
                        match crate::cloud::official_url(self.image_url.trim()){
                            Ok(_) if permission_valid=>{
                                self.preferences.approved_images.retain(|x|x.model!=self.image_model.trim());
                                self.preferences.approved_images.push(crate::model::ApprovedImage{model:self.image_model.trim().into(),url:self.image_url.trim().into(),permission_url:self.permission_url.trim().into()});
                                self.image_model.clear();self.image_url.clear();self.permission_url.clear();self.textures.clear();self.image_requests.clear();changed=true;self.load_images();
                            }
                            _=>self.report_error("图片地址或许可依据无效".into())
                        }
                    }
                });
            });
            section_title(ui,"设备面板",dark);
            design::settings_frame(dark, self.material()).show(ui,|ui| {
                ui.set_width(ui.available_width());
                let mut enabled=self.preferences.native_panels;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("使用专用面板").size(12.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center),|ui| {
                        if design::toggle(ui,&mut enabled).changed() {self.preferences.native_panels=enabled;changed=true;}
                    });
                });
                ui.add(egui::Label::new(RichText::new(format!("已加载 {} 个适配包 · 未匹配时使用基础控制",self.adapters.count())).size(10.5).color(muted(dark))).wrap());
                if self.adapters.issues>0 {ui.label(RichText::new(format!("{} 个适配包无效或冲突，已忽略",self.adapters.issues)).size(10.0).color(Color32::from_rgb(166,119,43)));}
                egui::CollapsingHeader::new("导入本机适配包").show(ui,|ui| {
                    if ui.add_enabled(!self.demo,egui::Button::new("选择并导入适配包…")).clicked() {
                        match native::pick_panel_file(self.native_handle,false) {
                            Ok(Some(path))=>match self.adapters.install(&self.root,&path) {
                                Ok(name)=>self.notice(&format!("已导入：{name}")),Err(error)=>self.report_error(error),
                            },
                            Ok(None)=>{},Err(error)=>self.report_error(error),
                        }
                        self.dismiss_guard.reset();
                    }
                    ui.add(egui::Label::new(RichText::new("适配包只安排已有控件，不包含脚本，也不改变设备权限。").size(10.0).color(muted(dark))).wrap());
                });
                self.draw_plugin_library(ui);
                egui::CollapsingHeader::new("其他插件静态检测").show(ui,|ui| {
                    if ui.add_enabled(!self.plugin_inspecting,egui::Button::new(if self.plugin_inspecting {"正在检测…"}else{"选择插件并检测…"})).clicked() {
                        match native::pick_panel_file(self.native_handle,true) {
                            Ok(Some(path))=>{
                                self.plugin_inspecting=true;self.plugin_report=None;
                                let tx=self.events_tx.clone();let ctx=self.context.clone();
                                self.runtime.spawn_blocking(move || {let result=crate::plugin_probe::inspect(&path);let _=tx.send(UiMessage::PluginInspected(result));ctx.request_repaint();});
                            },Ok(None)=>{},Err(error)=>self.report_error(error),
                        }self.dismiss_guard.reset();
                    }
                    if let Some(report)=&self.plugin_report {
                        ui.add(egui::Label::new(RichText::new(format!("{} · {}\n{}",report.package,report.version,report.format.label())).size(11.0)).wrap());
                        ui.label(RichText::new("静态检测不授予运行权限；请使用已下载列表的校验入口").size(10.).color(muted(dark)));
                    }
                });
            });
            section_title(ui,"账号",dark);
            design::settings_frame(dark, self.material()).show(ui,|ui|{
                ui.set_width(ui.available_width());
                ui.horizontal(|ui|{
                    let (rect,_)=ui.allocate_exact_size(Vec2::splat(24.),Sense::hover());
                    design::icon(ui.painter(),rect.center(),Icon::Lock,20.,design::accent(dark));
                    ui.label(RichText::new("Windows 本机加密保护").size(12.).strong());
                });
                ui.label(RichText::new("扫码会话仅保存在本机").size(10.).color(muted(dark)));
                if self.authenticated && ui.button(RichText::new("退出当前账号").color(Color32::from_rgb(180,94,76))).clicked(){self.confirm_logout=true;}
            });
            ui.add_space(15.);
            egui::CollapsingHeader::new("开源字体许可（SIL OFL 1.1）").show(ui,|ui| {
                ui.add(egui::Label::new(RichText::new(crate::fonts::LICENSE).size(10.0)).wrap().selectable(true));
            });
            egui::CollapsingHeader::new("插件包读取组件许可（MIT）").show(ui,|ui| {
                ui.add(egui::Label::new(RichText::new(crate::plugin_probe::LICENSE).size(10.0)).wrap().selectable(true));
            });
            ui.vertical_centered(|ui|{ui.label(RichText::new(format!("HomePanel v{}  ·  MIT  ·  独立社区客户端",env!("CARGO_PKG_VERSION"))).size(10.).color(muted(dark)));});
            ui.add_space(8.);
        });
        if changed{
            self.dark=self.dark_for_preferences();apply_visuals(ctx,self.dark,self.appearance.reduce_motion);
            native::apply_window_appearance_hwnd(self.native_handle,self.material(),self.dark);
            self.window_region=None;
            self.persist_preferences();
        }
        if region_changed && self.authenticated{self.refresh_devices();}
        self.draw_logout_dialog(ctx);
    }

    fn draw_logout_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_logout {
            return;
        }
        egui::Window::new("退出当前账号？")
            .id(Id::new("logout-dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("此操作只清除 HomePanel 本机保存的会话，不会修改米家设备。");
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        self.confirm_logout = false;
                    }
                    if ui.button(RichText::new("退出账号").color(Color32::from_rgb(180, 62, 62))).clicked() {
                        self.logout();
                    }
                });
            });
    }

    fn draw_hide_confirmation(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_hide.clone() else {
            return;
        };
        let name = self.devices.iter().find(|device| device.id == id).map(|device| device.name.clone()).unwrap_or_else(|| "此设备".into());
        egui::Window::new("仅从 HomePanel 隐藏？")
            .id(Id::new("hide-device-dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!("“{name}”会从设备面板中隐藏，可在设置中恢复。"));
                ui.label(RichText::new("不会从米家家庭中删除设备。").size(11.0).color(muted(self.dark)));
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        self.confirm_hide = None;
                    }
                    if ui.button("隐藏设备").clicked() {
                        if !self.preferences.hidden_devices.contains(&id) {
                            self.preferences.hidden_devices.push(id.clone());
                        }
                        self.preferences.favorites.retain(|value| value != &id);
                        self.confirm_hide = None;
                        self.selected_device = None;
                        self.persist_preferences();
                    }
                });
            });
    }

    fn device_in_selected_home(&self, device: &Device) -> bool {
        self.selected_home.as_deref().is_none_or(|id| device.home_id == id)
    }

    fn device_busy(&self, id: &str) -> bool {
        self.mina_pending.contains(id) || self.pending.iter().any(|key| {
            key == id || key.starts_with(&format!("{id}:")) || key.starts_with(&format!("action:{id}:"))
        })
    }

    fn filtered_devices(&self) -> Vec<Device> {
        let mut devices: Vec<_> = self
            .devices
            .iter()
            .filter(|device| self.device_in_selected_home(device))
            .filter(|device| !self.preferences.hidden_devices.contains(&device.id))
            .filter(|device| room_count_matches(&self.selected_room, device, &self.preferences.favorites))
            .filter(|device| device_matches_query(device, &self.search_query))
            .cloned()
            .collect();
        devices.sort_by(|left, right| {
            let left_favorite = self.preferences.favorites.contains(&left.id);
            let right_favorite = self.preferences.favorites.contains(&right.id);
            right_favorite.cmp(&left_favorite).then_with(|| left.name.cmp(&right.name))
        });
        devices
    }

    fn toggle_favorite(&mut self, id: &str) {
        if self.preferences.favorites.iter().any(|value| value == id) {
            self.preferences.favorites.retain(|value| value != id);
            self.notice("已取消收藏");
        } else {
            self.preferences.favorites.push(id.into());
            self.notice("设备已收藏");
        }
        self.persist_preferences();
    }

    fn poll_periodic_reads(&mut self) {
        if self.demo || !self.authenticated || !self.visible || self.last_state_poll.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.last_state_poll = Instant::now();
        if !self.refreshing && self.last_discovery.elapsed() >= Duration::from_secs(60) { self.refresh_devices(); }
        let busy = self.devices.iter().filter(|d| self.device_busy(&d.id)).map(|d| d.id.clone()).collect();
        let devices = if self.page==Page::Activity { self.activity_candidates() } else { self.filtered_devices() };
        for id in self.poll_schedule.due(&devices, self.selected_device.as_deref(), &self.reading, &busy, crate::now_ms()) {
            self.read_device(&id);
        }
    }
}

impl eframe::App for HomePanelApp {
    #[cfg(debug_assertions)]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.demo { eprintln!("LIFECYCLE gui-on-exit"); }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] { [0.0; 4] }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        input.events.retain(|event| !matches!(event, egui::Event::Zoom(_)));
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(result)=self.plugin_session.as_ref().and_then(|session|session.outcome()) {
            self.plugin_session=None;
            match result {Ok(())=>self.notice("插件窗口已关闭"),Err(_)=>self.report_error("插件连接已中断，请重新打开或使用原生控制".into())}
        }
        let explicit_visibility=self.handle_messages(ctx);
        self.poll_speaker_state();
        #[cfg(windows)]
        if !explicit_visibility && !self.visible && !self.quitting && native::is_window_visible(self.native_handle) {
            // A second launch restores the existing native window through single-instance activation.
            self.visible=true;
            self.poll_schedule.forget();
            self.dismiss_guard.reset();
            #[cfg(debug_assertions)]
            if self.demo {eprintln!("PREVIEW native-restore visible=true");}
        }
        #[cfg(not(windows))]
        let _=explicit_visibility;
        self.handle_login_updates();
        self.poll_periodic_reads();
        #[cfg(debug_assertions)]
        let layout_preview=self.demo && std::env::args().any(|arg|arg=="--demo-layout");
        #[cfg(not(debug_assertions))]
        let layout_preview=false;
        if !self.quitting && !layout_preview && self.dismiss_guard.observe(self.visible, native::own_foreground(self.native_handle), Instant::now()) {
            self.hide_panel(ctx);
            #[cfg(debug_assertions)]
            if self.demo {
                eprintln!("PREVIEW outside-dismiss hidden={}", !self.visible);
                let handle=self.native_handle;
                self.runtime.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    eprintln!("PREVIEW native-hidden={}",!native::is_window_visible(handle));
                });
            }
        } else if self.dismiss_guard.waiting() {
            ctx.request_repaint_after(crate::dismiss::DismissGuard::DELAY);
        }
        #[cfg(debug_assertions)]
        if self.demo && ctx.input(|input| input.key_pressed(egui::Key::F8)) {
            self.report_error("UNCERTAIN: 演示提示：操作结果尚未确认，请刷新查看；程序不会自动重复发送".into());
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            if self.quitting {
                return;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_panel(ctx);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if egui::Popup::is_any_open(ctx) {
                egui::Popup::close_all(ctx);
            } else if self.selected_device.is_some() {
                self.selected_device = None;
                self.discard_unsubmitted_drafts();
            } else if self.confirm_hide.is_some() {
                self.confirm_hide = None;
            } else if self.confirm_logout {
                self.confirm_logout = false;
            } else if matches!(self.page,Page::Settings|Page::Activity) {
                self.page = Page::Dashboard;
            } else {
                self.hide_panel(ctx);
            }
        }

        let mut appearance_changed = false;
        if self.visible && self.last_appearance_poll.elapsed() >= Duration::from_secs(2) {
            let appearance = native::appearance();
            appearance_changed = appearance != self.appearance;
            self.appearance = appearance;
            self.last_appearance_poll = Instant::now();
        }
        let dark = self.dark_for_preferences();
        if dark != self.dark || appearance_changed {
            self.dark = dark;
            apply_visuals(ctx, dark, self.appearance.reduce_motion);
            native::apply_window_appearance_hwnd(self.native_handle, self.material(), dark);
            self.window_region=None;
        }

        let detail_open=self.selected_device.is_some();
        if detail_open!=self.detail_geometry {
            let size=if detail_open {design::DETAIL_SIZE} else {Vec2::new(PANEL_SIZE[0],PANEL_SIZE[1])};
            if detail_open {
                self.main_position=ctx.input(|i|i.viewport().outer_rect.map(|r|r.min));
                if let Some(position)=self.main_position {ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(position+egui::vec2(36.0,40.0)));}
            } else if let Some(position)=self.main_position.take() {ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(position));}
            // Two fixed sizes, changed once on navigation; never resize for values,
            // sliders, polling, tabs, or animation frames. Backdrop matches the card.
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(size));
            ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(size));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            native::apply_window_appearance_hwnd(self.native_handle,GlassStyle::Glass,self.dark);
            self.window_region=None;self.detail_geometry=detail_open;self.backdrop_pending=true;
            ctx.request_repaint();
        }
        let expected=if detail_open {design::DETAIL_SIZE}else{Vec2::new(PANEL_SIZE[0],PANEL_SIZE[1])};
        if (ctx.content_rect().size()-expected).length_sq()<1.0 && self.backdrop_pending {
            native::apply_window_appearance_hwnd(self.native_handle,self.material(),self.dark);
            self.window_region=None;self.backdrop_pending=false;
        }
        self.window_region=native::rounded_window(self.native_handle,ctx.pixels_per_point(),self.selected_device.is_some(),self.window_region);
        let motion_key=Id::new((self.page as u8,&self.selected_device,self.detail_tab as u8)).value();
        self.frame_motion=self.content_motion.sample(motion_key,ctx.input(|i|i.time),self.appearance.reduce_motion);
        if self.frame_motion.active {ctx.request_repaint_after(Duration::from_millis(16));}

        egui::CentralPanel::default()
            .frame(design::main_frame(self.dark,self.material(),self.selected_device.is_some()))
            .show(ctx, |ui| {
                if self.selected_device.is_none() {
                    ctx.layer_painter(ui.layer_id()).rect_stroke(ctx.content_rect().shrink(0.5),design::WINDOW_RADIUS,
                        Stroke::new(1.0_f32,Color32::from_white_alpha(if self.dark{35}else{170})),egui::StrokeKind::Inside);
                }
                let rect=ui.max_rect().translate(egui::vec2(0.0,self.frame_motion.offset));
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect),|ui| {
                ui.multiply_opacity(self.frame_motion.opacity);
                if self.selected_device.is_none() {ui.set_min_width(400.0);}
                match self.page {
                    Page::Login => { if self.selected_device.is_none() { self.draw_login(ui, ctx); } },
                    Page::Dashboard => { if self.selected_device.is_none() { self.draw_dashboard(ui); } },
                    Page::Activity => { if self.selected_device.is_none() { self.draw_activity(ui); } },
                    Page::Settings => { if self.selected_device.is_none() { self.draw_settings(ui, ctx); } },
                }
                });
            });
        self.draw_detail(ctx);
        self.draw_hide_confirmation(ctx);
        self.draw_toasts(ctx);

        if self.visible {
            let playing=!self.appearance.reduce_motion && self.selected_device.as_ref().and_then(|id|self.mina_states.get(id)).is_some_and(|state|state.current()&&state.status==Some(1));
            ctx.request_repaint_after(if playing{Duration::from_millis(250)}else{Duration::from_secs(1)});
        }
    }
}

impl HomePanelApp {
    fn draw_toasts(&mut self, ctx: &egui::Context) {
        if self.error.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(7)) {
            self.error = None;
        }
        if self.notice.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(3)) {
            self.notice = None;
        }
        // Device sheets use their reserved footer, so messages cannot cover controls.
        if self.selected_device.is_some() { return; }
        let toast = self.error.as_ref().map(|(text, _)| (text.clone(), true)).or_else(|| self.notice.as_ref().map(|(text, _)| (text.clone(), false)));
        if let Some((text, is_error)) = toast {
            let (message, pending) = toast_message(&text);
            let failed = is_error && !pending;
            egui::Area::new(Id::new("homepanel-toast"))
                .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -48.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let fill = if pending { if self.dark { Color32::from_rgb(75, 61, 36) } else { Color32::from_rgb(255, 243, 214) } }
                        else if failed { Color32::from_rgb(176, 63, 62) } else if self.dark { Color32::from_rgb(47, 66, 63) } else { Color32::from_rgb(225, 244, 238) };
                    let color = if failed { Color32::WHITE } else if pending { if self.dark { Color32::from_rgb(255, 222, 155) } else { Color32::from_rgb(113, 79, 26) } } else { self.text_color() };
                    egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(12)).inner_margin(10.0).show(ui, |ui| {
                        ui.set_width(300.0);
                        if pending { ui.label(RichText::new("状态待确认").size(12.0).strong().color(color)); }
                        ui.add(egui::Label::new(RichText::new(message).size(11.0).color(color)).wrap());
                    });
                });
        }
    }

    fn text_color(&self) -> Color32 {
        design::ink(self.dark)
    }
}

fn toast_message(text: &str) -> (&str, bool) {
    match text.strip_prefix("UNCERTAIN: ") {
        Some(message) => (message, true),
        None => (text, false),
    }
}

fn displayed_property(property: &Property, drafts: &HashMap<String, Value>, key: &str) -> Property {
    let mut shown = property.clone();
    if let Some(value) = drafts.get(key) { shown.value = Some(value.clone()); }
    shown
}

fn finish_property_write(devices: &mut [Device], pending: &mut HashSet<String>, drafts: &mut HashMap<String, Value>, key: &str, result: Result<Device, String>) -> Result<(), String> {
    pending.remove(key);
    drafts.remove(key);
    let device = result?;
    if let Some(item) = devices.iter_mut().find(|item| item.id == device.id) { crate::state_sync::merge(item, device); }
    Ok(())
}

fn install_tray_handlers(ctx: &egui::Context, tx: mpsc::Sender<UiMessage>, handle: Option<isize>) {
    let context = ctx.clone();
    let tray_tx = tx.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            position,
            ..
        } = event
        {
            queue_tray_command(&context, &tray_tx, UiMessage::TrayToggle(Some([position.x, position.y])), handle);
        }
    }));
    let context = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        dispatch_menu_command(event.id.as_ref(), &context, &tx, handle);
    }));
}

fn queue_tray_command(ctx: &egui::Context, tx: &mpsc::Sender<UiMessage>, message: UiMessage, handle: Option<isize>) {
    if tx.send(message).is_ok() {
        // Hidden Windows windows do not reliably receive redraws. Wake the
        // native window after queueing, before relying on an egui update.
        native::wake_window(handle);
        ctx.request_repaint();
    }
}

fn dispatch_menu_command(command: &str, ctx: &egui::Context, tx: &mpsc::Sender<UiMessage>, handle: Option<isize>) {
    // Mark the close as an explicit quit before Windows reports CloseRequested,
    // otherwise the app's close-to-tray policy would cancel it. A hidden window
    // is woken minimized so it can drain the queue without opening the panel.
    if command == "quit" {
        if tx.send(UiMessage::Quit).is_ok() {
            native::wake_window_for_exit(handle);
            native::request_window_exit(handle);
            ctx.request_repaint();
        }
        return;
    }
    let message = match command {
        "open" => Some(UiMessage::TrayToggle(None)),
        "settings" => Some(UiMessage::TraySettings),
        "quit" => Some(UiMessage::Quit),
        _ => None,
    };
    if let Some(message) = message { queue_tray_command(ctx, tx, message, handle); }
}

#[cfg(debug_assertions)]
impl HomePanelApp {
    /// Isolated, synthetic lifecycle tests. This code is absent from Release builds.
    fn install_lifecycle_probe(&mut self, ctx: &egui::Context) {
        let Some(scenario) = std::env::args().find_map(|arg| arg.strip_prefix("--demo-lifecycle=").map(str::to_owned)) else { return; };
        assert!(matches!(scenario.as_str(), "visible-quit" | "hidden-quit" | "hidden-open-quit" | "hidden-relaunch-quit" | "blocking-quit" | "material-corners"), "Unknown isolated lifecycle scenario");
        if scenario == "blocking-quit" {
            self.runtime.spawn_blocking(|| std::thread::sleep(Duration::from_secs(20)));
        }
        let context = ctx.clone();
        let tx = self.events_tx.clone();
        let handle = self.native_handle;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(900));
            if scenario=="material-corners" {
                for dark in [false,true] {
                    for material in [GlassStyle::Solid,GlassStyle::Frosted,GlassStyle::Glass] {
                        for detail in [false,true] {
                            let _=tx.send(UiMessage::DemoMaterial(material,dark,detail));context.request_repaint();
                            std::thread::sleep(Duration::from_millis(400));
                            let result=native::inspect_window_silhouette(handle);
                            eprintln!("MATERIAL_CORNER {}",serde_json::json!({"material":material,"dark":dark,"detail":detail,"native":result}));
                        }
                    }
                }
            }
            if scenario.starts_with("hidden") {
                dispatch_menu_command("open", &context, &tx, handle);
                std::thread::sleep(Duration::from_millis(900));
                eprintln!("LIFECYCLE hidden={}", !native::is_window_visible(handle));
                if scenario == "hidden-open-quit" {
                    dispatch_menu_command("open", &context, &tx, handle);
                    std::thread::sleep(Duration::from_millis(900));
                    eprintln!("LIFECYCLE reopened={}", native::is_window_visible(handle));
                } else if scenario=="hidden-relaunch-quit" {
                    let mut command=std::process::Command::new(std::env::current_exe().expect("isolated preview executable"));
                    command.arg("--demo");
                    #[cfg(windows)]
                    { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
                    assert!(command.status().expect("preview reactivation").success());
                    std::thread::sleep(Duration::from_millis(900));
                    eprintln!("LIFECYCLE reopened={}",native::is_window_visible(handle));
                }
            }
            eprintln!("LIFECYCLE quit-requested");
            dispatch_menu_command("quit", &context, &tx, handle);
        });
    }
}

fn apply_visuals(ctx: &egui::Context, dark: bool, reduce_motion: bool) {
    let mut visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    visuals.override_text_color = Some(design::ink(dark));
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.window_fill = design::surface(dark);
    visuals.window_corner_radius = CornerRadius::same(16);
    visuals.window_stroke = Stroke::new(1.0_f32,design::line(dark));
    visuals.extreme_bg_color = if dark {Color32::from_rgb(28,39,31)}else{Color32::from_rgb(241,245,236)};
    visuals.selection.bg_fill = design::accent(dark);
    visuals.selection.stroke = Stroke::new(1.0_f32,if dark {Color32::BLACK}else{Color32::WHITE});
    visuals.widgets.inactive.bg_fill = if dark {Color32::from_rgb(44,58,47)}else{Color32::from_rgb(226,235,229)};
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.5_f32,design::ink(dark));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32,design::ink(dark));
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32,design::ink(dark));
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32,design::line(dark));
    visuals.widgets.hovered.bg_fill = if dark {Color32::from_rgb(54,73,59)}else{Color32::from_rgb(228,238,219)};
    visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32,design::accent(dark).gamma_multiply(0.4));
    visuals.widgets.active.bg_fill = design::accent(dark);
    visuals.widgets.active.weak_bg_fill = visuals.widgets.hovered.bg_fill;
    for widget in [&mut visuals.widgets.noninteractive, &mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active, &mut visuals.widgets.open] {
        widget.corner_radius = CornerRadius::same(9);
    }
    ctx.set_theme(if dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.set_visuals(visuals);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.,6.);
    style.spacing.button_padding = Vec2::new(10.,6.);
    style.spacing.interact_size.y = 28.;
    style.spacing.scroll.bar_width = 3.;
    style.spacing.slider_rail_height = 5.;
    style.animation_time = if reduce_motion {0.} else {0.18};
    ctx.set_style(style);
}

fn theme_is_dark(preferences: &Preferences, appearance: native::Appearance) -> bool {
    match preferences.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => appearance.system_dark,
    }
}

fn decode_texture(ctx: &egui::Context, key: &str, uri: &str) -> Option<egui::TextureHandle> {
    let (_, encoded) = uri.split_once(',')?;
    let bytes = B64.decode(encoded).ok()?;
    if bytes.len() > 2_000_000 {
        return None;
    }
    let image = image::load_from_memory(&bytes).ok()?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return None;
    }
    let rgba = image.to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba.as_raw());
    Some(ctx.load_texture(key, color, egui::TextureOptions::LINEAR))
}

fn brand_mark(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(13), if ui.visuals().dark_mode { Color32::from_rgb(34, 77, 67) } else { Color32::from_rgb(222, 242, 234) });
    let center = rect.center();
    let scale = size / 48.0;
    let stroke = Stroke::new(2.4 * scale, ACCENT);
    painter.line_segment([center + egui::vec2(-12.0 * scale, -1.0 * scale), center + egui::vec2(0.0, -12.0 * scale)], stroke);
    painter.line_segment([center + egui::vec2(0.0, -12.0 * scale), center + egui::vec2(12.0 * scale, -1.0 * scale)], stroke);
    painter.rect_stroke(egui::Rect::from_center_size(center + egui::vec2(0.0, 5.0 * scale), egui::vec2(17.0 * scale, 13.0 * scale)), CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
    painter.rect_filled(egui::Rect::from_center_size(center + egui::vec2(0.0, 8.0 * scale), egui::vec2(4.0 * scale, 7.0 * scale)), CornerRadius::same(1), ACCENT);
}

fn section_title(ui: &mut egui::Ui, title: &str, dark: bool) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).size(13.0).strong().color(if dark { Color32::from_rgb(153, 198, 183) } else { ACCENT }));
    ui.add_space(4.0);
}

fn muted(dark: bool) -> Color32 { design::secondary(dark) }

fn device_matches_query(device: &Device, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || device.name.to_lowercase().contains(&query) || device.room.to_lowercase().contains(&query)
}

fn power_property(device: &Device) -> Option<&Property> {
    crate::overview::power_property(device)
}

fn device_summary(device: &Device) -> String {
    if !device.online {
        return "设备离线".into();
    }
    if device.kind=="speaker" { return crate::presentation::speaker_status(device); }
    if crate::overview::power_properties(device).len()>1 {
        return match crate::overview::activity(device,crate::now_ms()) {
            crate::overview::Activity::On=>"有通道已开启",crate::overview::Activity::Off=>"所有通道已关闭",_=>"通道状态待确认",
        }.into();
    }
    let property=["temperature","relative-humidity","electric-power","brightness","mode","status"].into_iter()
        .find_map(|key|device.properties.iter().find(|p|p.key==key && p.readable && p.error.is_none() && p.value.is_some()))
        .or_else(||device.properties.iter().find(|p|p.readable && p.key!="on"));
    if let Some(property) = property {
        let text=format!("{} {}",crate::presentation::property_name(property),crate::presentation::reading(device,property).text());
        return crate::presentation::reported_text(property,&text,crate::now_ms());
    }
    if device.spec_status == "unsupported" {
        "暂不支持此型号".into()
    } else {
        "设备在线".into()
    }
}

fn property_value(property: &Property) -> String {
    crate::presentation::value_text(property)
}

fn room_count_matches(room: &str, device: &Device, favorites: &[String]) -> bool {
    match room {
        "全部" => true,
        "常用" => favorites.iter().any(|id| id == &device.id),
        other => device.room == other,
    }
}

fn region_label(region: &str) -> &'static str {
    match region {
        "cn" => "中国大陆",
        "tw" => "台湾",
        "sg" => "新加坡",
        "de" => "欧洲",
        "us" => "美国",
        "ru" => "俄罗斯",
        "i2" => "印度",
        _ => "未知地区",
    }
}

fn property_control_key(device: &Device, property: &Property) -> String {
    format!("{}:{}:{}", device.id, property.siid, property.piid)
}

fn numeric_value(property: &Property, value: f64) -> Value {
    if property.format.starts_with("uint") || property.format.starts_with("int") {
        json!(value.round() as i64)
    } else {
        json!(value)
    }
}

#[cfg(debug_assertions)]
fn demo_devices() -> (Vec<Home>, Vec<Device>) {
    use crate::model::Room;
    let home = Home {
        id: "demo-home".into(),
        name: "我的家".into(),
        owner: String::new(),
        rooms: vec![
            Room { id: "living".into(), name: "客厅".into(), dids: vec!["demo-light".into(), "demo-plug".into()] },
            Room { id: "bedroom".into(), name: "卧室".into(), dids: vec!["demo-fan".into(), "demo-switch".into()] },
            Room { id: "balcony".into(), name: "阳台".into(), dids: vec!["demo-sensor".into()] },
        ],
    };
    let mut light = demo_device("demo-light", "客厅吸顶灯", "light", "yeelink.light.ceiling", "客厅", true);
    light.spec_urn="urn:miot-spec-v2:device:light:demo".into();
    light.properties = vec![
        demo_property(2, 1, "on", "电源", "bool", true, json!(true), vec![], None),
        demo_property(2, 2, "brightness", "亮度", "uint8", true, json!(64), vec![], Some(vec![1.0, 100.0, 1.0])),
        demo_property(2, 3, "color-temperature", "色温", "uint16", true, json!(4200), vec![], Some(vec![2700.0, 6500.0, 100.0])),
    ];
    let mut fan = demo_device("demo-fan", "循环扇", "fan", "dmaker.fan.p5", "卧室", true);
    fan.spec_urn="urn:miot-spec-v2:device:fan:demo".into();
    fan.properties = vec![
        demo_property(2, 1, "on", "电源", "bool", true, json!(false), vec![], None),
        demo_property(2, 2, "fan-level", "风速", "uint8", true, json!(2), vec![], Some(vec![1.0, 4.0, 1.0])),
    ];
    let mut plug = demo_device("demo-plug", "智能插座 · 四格演示", "plug", "chuangmi.plug.212a01", "客厅", true);
    plug.spec_urn = "urn:miot-spec-v2:device:outlet:0000A002:chuangmi-212a01:3".into();
    plug.properties = vec![
        demo_property(2, 1, "on", "电源", "bool", true, json!(true), vec![], None),
        demo_property(5, 1, "power-consumption", "用电量", "uint32", false, json!(29715), vec![], None),
        demo_property(5, 2, "electric-current", "电流", "uint16", false, json!(5), vec![], None),
        demo_property(5, 3, "voltage", "电压", "uint16", false, json!(220), vec![], None),
        demo_property(5, 6, "electric-power", "功率", "uint32", false, json!(1534), vec![], None),
        demo_property(2, 3, "default-power-on-state", "Default Power On State", "uint8", true, json!(0), vec![PropertyOption{value:json!(0),name:"Off".into()},PropertyOption{value:json!(1),name:"On".into()},PropertyOption{value:json!(2),name:"Previous".into()}], None),
    ];
    let mut switch = demo_device("demo-switch", "三键开关 · 中文演示", "switch", "demo.switch", "卧室", true);
    for (index, label) in ["左键", "中键", "右键"].into_iter().enumerate() {
        let mut property = demo_property(index as u32 + 2, 1, "mode", "模式", "uint8", true, json!(0),
            vec![PropertyOption{value:json!(0),name:"Wired And Wireless".into()},PropertyOption{value:json!(3),name:"Wireless".into()}], None);
        property.service = label.into();
        switch.properties.push(property);
    }
    let mut sensor = demo_device("demo-sensor", "温湿度传感器", "sensor", "lumi.sensor_ht.agl02", "阳台", true);
    sensor.spec_urn="urn:miot-spec-v2:device:temperature-humidity-sensor:demo".into();
    sensor.properties = vec![
        demo_property(3, 1, "temperature", "温度", "float", false, json!(24.6), vec![], None),
        demo_property(3, 2, "relative-humidity", "湿度", "uint8", false, json!(48), vec![], None),
        demo_property(3, 3, "battery-level", "电量", "uint8", false, json!(100), vec![], None),
    ];
    let mut mosquito = demo_device("demo-mosquito", "驱蚊器 · 图标演示", "mosquito", "demo.mosquito", "卧室", true);
    mosquito.properties = vec![demo_property(2, 1, "on", "电源", "bool", true, json!(false), vec![], None)];
    let mut speaker = demo_device("demo-speaker", "智能音箱 · 交互演示", "speaker", "demo.speaker", "客厅", true);
    speaker.properties=vec![
        demo_property(2,1,"volume","音量","uint8",true,json!(36),vec![],Some(vec![4.0,100.0,1.0])),
        demo_property(2,2,"mute","静音","bool",true,json!(false),vec![],None),
        demo_property(3,1,"playing-state","播放状态","uint8",false,json!(2),vec![PropertyOption{value:json!(1),name:"Playing".into()},PropertyOption{value:json!(0),name:"Stop".into()},PropertyOption{value:json!(2),name:"Pause".into()}],None),
        demo_property(5,4,"audio-id","音频编号","string",false,json!(""),vec![],None),
        demo_property(7,1,"no-disturb","勿扰模式","bool",true,json!(true),vec![],None),
        demo_property(7,2,"enable-time-period","启用时间段(格式为：'00:00:00-00:00:00')","string",true,json!("22:00-06:00"),vec![],None),
    ];
    speaker.properties[1].service="扬声器".into();speaker.properties[0].unit="percentage".into();
    speaker.actions=["previous","play","pause","next"].into_iter().enumerate().map(|(index,key)|Action{siid:3,aiid:index as u32+1,key:key.into(),name:crate::model::chinese(key,"")}).collect();
    let mut climate = demo_device("demo-climate", "空调 · 滚动回归演示", "climate", "demo.climate", "卧室", true);
    climate.properties = vec![
        demo_property(2, 1, "on", "电源", "bool", true, json!(true), vec![], None),
        demo_property(2, 2, "mode", "模式", "uint8", true, json!(1), vec![PropertyOption{value:json!(0),name:"Auto".into()},PropertyOption{value:json!(1),name:"Cool".into()},PropertyOption{value:json!(2),name:"Heat".into()},PropertyOption{value:json!(3),name:"Dry".into()}], None),
        demo_property(2, 3, "target-temperature", "设定温度", "uint8", true, json!(26), vec![], Some(vec![16.0,30.0,1.0])),
        demo_property(2, 4, "fan-level", "风速", "uint8", true, json!(1), vec![PropertyOption{value:json!(0),name:"Low".into()},PropertyOption{value:json!(1),name:"Medium".into()},PropertyOption{value:json!(2),name:"High".into()}], None),
        demo_property(2, 5, "horizontal-swing", "左右摆风", "bool", true, json!(false), vec![], None),
        demo_property(2, 6, "vertical-swing", "上下摆风", "bool", true, json!(true), vec![], None),
        demo_property(3, 1, "temperature", "室温", "float", false, json!(27.4), vec![], None),
        demo_property(4, 1, "target-humidity", "设定湿度", "uint8", true, json!(40), vec![], Some(vec![0.0,100.0,5.0])),
        demo_property(4, 2, "sleep-mode", "睡眠模式", "bool", true, json!(false), vec![], None),
        demo_property(4, 3, "favorite-type", "喜好模式类型", "uint8", true, json!(0), vec![PropertyOption{value:json!(0),name:"Unknown vendor option A".into()},PropertyOption{value:json!(1),name:"Unknown vendor option B".into()}], None),
    ];
    for index in 4..15 { climate.properties.push(demo_property(4,index,"vendor-feature",&format!("附加设置 {index} · 演示"),"bool",true,json!(false),vec![],None)); }
    (vec![home], vec![light, fan, plug, sensor, switch, mosquito, speaker, climate])
}

#[cfg(debug_assertions)]
fn demo_device(id: &str, name: &str, kind: &str, model: &str, room: &str, online: bool) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        model: model.into(),
        home_id: "demo-home".into(),
        room: room.into(),
        online,
        kind: kind.into(),
        spec_status: "ready".into(),
        ..Default::default()
    }
}

#[cfg(debug_assertions)]
fn demo_property(siid: u32, piid: u32, key: &str, name: &str, format: &str, writable: bool, value: Value, options: Vec<PropertyOption>, range: Option<Vec<f64>>) -> Property {
    Property {
        siid,
        piid,
        key: key.into(),
        name: name.into(),
        service: String::new(),
        format: format.into(),
        unit: match key { "temperature" | "target-temperature" => "celsius", "relative-humidity" | "target-humidity" | "battery-level" | "brightness" => "percentage", "color-temperature" => "kelvin", _ => "" }.into(),
        readable: true,
        writable,
        range,
        options,
        value: Some(value),
        error: None,
        checked_at: Some(crate::now_ms()),
        source: crate::model::StateSource::Device,
        ..Default::default()
    }
}

fn speaker_event_current(main_epoch:u64,generation:u64,event_epoch:u64,event_generation:u64)->bool {
    main_epoch==event_epoch && generation==event_generation
}

fn invalidate_speaker_requests(generation:&mut u64,reading:&mut HashSet<String>,pending:&mut HashSet<String>,states:&mut HashMap<String,crate::speaker_service::Snapshot>,drafts:&mut HashMap<String,Value>) {
    *generation=generation.wrapping_add(1);
    reading.clear();pending.clear();states.clear();
    drafts.retain(|key,_|!key.starts_with("mina:"));
}

fn discard_invalid_speaker_drafts(id:&str,state:&crate::speaker_service::Snapshot,pending:&HashSet<String>,drafts:&mut HashMap<String,Value>) {
    // A successful refresh must not move a thumb under the user's pointer.
    // Discard only drafts whose source field is now unusable, and keep any
    // submitted value visible until its own confirmation event arrives.
    if pending.contains(id){return;}
    if !state.current() || state.volume.is_none(){drafts.remove(&format!("mina:{id}:volume"));}
    if !state.current() || state.position_ms.is_none() || state.duration_ms.is_none_or(|duration|duration==0){drafts.remove(&format!("mina:{id}:position"));}
}

#[cfg(test)]
mod ui_tests {
    use super::*;

    #[test]
    fn speaker_session_reset_rejects_old_results_without_leaving_busy_markers() {
        let mut generation=7;
        let mut reading=HashSet::from(["fixture-speaker".into()]);
        let mut pending=HashSet::from(["fixture-speaker".into()]);
        let mut states=HashMap::from([("fixture-speaker".into(),crate::speaker_service::Snapshot::default())]);
        let mut drafts=HashMap::from([("mina:fixture-speaker:volume".into(),json!(70)),("mina:fixture-speaker:sleep".into(),json!(true)),("other-device:2:1".into(),json!(55))]);
        invalidate_speaker_requests(&mut generation,&mut reading,&mut pending,&mut states,&mut drafts);
        assert_eq!(generation,8);
        assert!(reading.is_empty());assert!(pending.is_empty());assert!(states.is_empty());
        assert_eq!(drafts,HashMap::from([("other-device:2:1".into(),json!(55))]));
        assert!(!speaker_event_current(12,generation,11,7));
        assert!(!speaker_event_current(12,generation,12,7));
        assert!(!speaker_event_current(12,generation,11,generation));
        assert!(speaker_event_current(12,generation,12,generation));
        // A stale completion cannot clear the new generation's pending request.
        reading.insert("fixture-speaker".into());pending.insert("fixture-speaker".into());
        if speaker_event_current(12,generation,11,7){reading.clear();pending.clear();}
        assert!(reading.contains("fixture-speaker"));assert!(pending.contains("fixture-speaker"));
    }

    #[test]
    fn speaker_refresh_keeps_active_drag_and_clears_only_invalid_unsubmitted_fields() {
        use crate::speaker_service::Snapshot;
        let state=Snapshot{at:crate::now_ms(),player_at:crate::now_ms(),volume:Some(42),position_ms:Some(30000),duration_ms:Some(120000),..Default::default()};
        let make_drafts=||HashMap::from([("mina:fixture-speaker:volume".into(),json!(70)),("mina:fixture-speaker:position".into(),json!(60000)),("mina:fixture-speaker:sleep".into(),json!(true)),("mina:other-speaker:volume".into(),json!(25))]);
        let mut drafts=make_drafts();
        discard_invalid_speaker_drafts("fixture-speaker",&state,&HashSet::new(),&mut drafts);
        assert_eq!(drafts,make_drafts(),"fresh reads must not replace an active drag");
        let mut missing_position=state.clone();missing_position.position_ms=None;
        discard_invalid_speaker_drafts("fixture-speaker",&missing_position,&HashSet::new(),&mut drafts);
        assert!(!drafts.contains_key("mina:fixture-speaker:position"));
        assert_eq!(drafts.get("mina:fixture-speaker:volume"),Some(&json!(70)));
        let failed=Snapshot::read_failure(&state,"fixture network failure".into());
        discard_invalid_speaker_drafts("fixture-speaker",&failed,&HashSet::new(),&mut drafts);
        assert!(!drafts.contains_key("mina:fixture-speaker:volume"));
        assert_eq!(drafts.get("mina:fixture-speaker:sleep"),Some(&json!(true)));
        assert_eq!(drafts.get("mina:other-speaker:volume"),Some(&json!(25)));
        let mut submitted=make_drafts();
        discard_invalid_speaker_drafts("fixture-speaker",&failed,&HashSet::from(["fixture-speaker".into()]),&mut submitted);
        assert_eq!(submitted,make_drafts(),"submitted values belong to their confirmation event");
    }

    #[test]
    fn a_speaker_alarm_switch_is_never_presented_as_device_power() {
        let speaker=Device {kind:"speaker".into(),properties:vec![Property {key:"on".into(),service:"闹钟".into(),writable:true,..Default::default()}],..Default::default()};
        assert!(power_property(&speaker).is_none());
        let mut plug=Device{kind:"plug".into(),..speaker};
        assert!(power_property(&plug).is_none());
        plug.properties.push(Property{key:"on".into(),service_key:"switch".into(),writable:true,..Default::default()});
        assert!(power_property(&plug).is_some());
    }

    #[test]
    fn submitted_values_stay_visible_until_confirmed_and_failures_restore_old_state() {
        let key = "test-device:2:1";
        let property = Property { siid:2, piid:1, value:Some(json!(30)), ..Default::default() };
        for success in [true, false] {
            let mut devices = vec![Device { id:"test-device".into(), properties:vec![property.clone()], ..Default::default() }];
            let mut pending = HashSet::from([key.to_owned(),"another-device".to_owned()]);
            let mut drafts = HashMap::from([(key.to_owned(), json!(75))]);
            for _ in 0..3 { assert_eq!(displayed_property(&devices[0].properties[0], &drafts, key).value, Some(json!(75))); }
            assert_eq!(devices[0].properties[0].value, Some(json!(30)));
            let result = if success {
                let mut updated = devices[0].clone(); updated.properties[0].value=Some(json!(75)); Ok(updated)
            } else { Err("UNCERTAIN: 状态未确认".into()) };
            assert_eq!(finish_property_write(&mut devices,&mut pending,&mut drafts,key,result).is_ok(),success);
            assert!(!pending.contains(key));
            assert!(pending.contains("another-device"));
            assert!(!drafts.contains_key(key));
            assert_eq!(displayed_property(&devices[0].properties[0], &drafts, key).value, Some(json!(if success {75} else {30})));
        }
    }

    #[test]
    fn uncertainty_keeps_its_severity_and_hides_protocol_prefix() {
        assert_eq!(toast_message("UNCERTAIN: 状态尚未确认"), ("状态尚未确认", true));
        assert_eq!(toast_message("设备未接受操作（-1）"), ("设备未接受操作（-1）", false));
    }

    #[test]
    fn appearance_selects_the_active_theme_before_styling() {
        let ctx = egui::Context::default();
        ctx.set_theme(egui::Theme::Dark);
        apply_visuals(&ctx, false, true);
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert_eq!(ctx.style().visuals.selection.bg_fill, design::accent(false));
        assert_eq!(ctx.style().animation_time, 0.0);
        assert_eq!(ctx.style().visuals.strong_text_color(), design::ink(false));
        apply_visuals(&ctx, true, false);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert_eq!(ctx.style().visuals.selection.bg_fill, design::accent(true));
    }
}
