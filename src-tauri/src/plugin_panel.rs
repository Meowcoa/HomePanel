use super::*;
use crate::{plugin_catalog::{CatalogEntry,PackageKind},plugin_download::DownloadReport};

impl HomePanelApp {
    pub(super) fn refresh_plugin_catalog(&mut self) {
        match crate::plugin_catalog::list(&self.root) {
            Ok(entries)=>{self.plugin_catalog=entries;self.plugin_catalog_error=None;},
            Err(error)=>{self.plugin_catalog_error=Some(error);},
        }
    }
    fn downloaded_plugin(&self,model:&str)->Option<DownloadReport> {
        self.plugin_catalog.iter().filter(|e|e.kind==Some(PackageKind::Plugin)&&e.model.as_deref()==Some(model)&&e.cache_available)
            .filter_map(|e|e.report.as_ref()).max_by_key(|r|r.version).cloned()
    }
    fn start_plugin_download(&mut self,device:&Device) {
        if self.demo||!self.authenticated{return;}
        let epoch=self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst);
        if self.plugin_download_pending.get(&device.model)==Some(&epoch){return;}
        self.plugin_download_pending.insert(device.model.clone(),epoch);
        self.plugin_download_results.remove(&device.model);
        let cloud=self.cloud.clone();let root=self.root.clone();let region=self.preferences.region.clone();let model=device.model.clone();
        let tx=self.events_tx.clone();let context=self.context.clone();
        self.notice("正在从米家查询型号、下载并检查插件…");
        self.runtime.spawn(async move{
            let result=tokio::time::timeout(Duration::from_secs(120),crate::plugin_download::download(&cloud,&region,&model,&root)).await
                .unwrap_or_else(|_|Err("插件下载检查超时，请重试".into()));
            let _=tx.send(UiMessage::PluginDownloaded(model,epoch,result));context.request_repaint();
        });
    }
    fn start_plugin_prepare(&mut self,model:&str,report:DownloadReport,launch:Option<(Device,bool)>) {
        if self.demo||!self.authenticated{return;}
        let epoch=self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst);
        if self.plugin_preparing.get(model)==Some(&epoch){return;}
        self.plugin_preparing.insert(model.into(),epoch);self.plugin_prepared.remove(model);
        let cloud=self.cloud.clone();let root=self.root.clone();let region=self.preferences.region.clone();let model=model.to_owned();
        let tx=self.events_tx.clone();let context=self.context.clone();
        self.notice("正在验签并准备运行文件，首次会获取对应 SDK…");
        self.runtime.spawn(async move {
            let mut result=tokio::time::timeout(Duration::from_secs(150),crate::plugin_prepare::prepare(&cloud,&region,&root,&model,&report)).await
                .unwrap_or_else(|_|Err("插件准备超时，原有缓存已保留".into()));
            let launch=if result.is_ok() {match launch {
                Some((device,writes))=>match cloud.load_spec(&device.id).await {
                    Ok(current) if current.model==model&&cloud.epoch.load(std::sync::atomic::Ordering::SeqCst)==epoch=>Some((current,writes)),
                    Ok(_)=>{result=Err("设备或账号已变化，取消启动".into());None},
                    Err(error)=>{result=Err(format!("运行文件已准备，但设备规格读取失败：{error}"));None},
                },None=>None,
            }}else{None};
            let _=tx.send(UiMessage::PluginPrepared(model,epoch,launch.map(|(device,writes)|(device,writes,report)),result));context.request_repaint();
        });
    }
    pub(super) fn open_downloaded_plugin(&mut self,device:&Device,writes:bool,report:&DownloadReport) {
        if self.demo{return;}
        self.plugin_session=None;
        self.plugin_launch_generation=self.plugin_launch_generation.wrapping_add(1);self.plugin_launching=true;
        let generation=self.plugin_launch_generation;let epoch=self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst);
        let cloud=self.cloud.clone();let device=device.clone();let region=self.preferences.region.clone();let root=self.root.clone();let report=report.clone();let context=self.context.clone();let tx=self.events_tx.clone();
        self.runtime.spawn_blocking(move||{
            let result=if cloud.epoch.load(std::sync::atomic::Ordering::SeqCst)==epoch {crate::plugin_bridge::launch_prepared(cloud,device,region,&root,&report,writes,epoch,context.clone())}else{Err("账号已变更，取消插件启动".into())};
            let _=tx.send(UiMessage::PluginStarted(epoch,generation,result));context.request_repaint();
        });
        self.notice("正在核对运行文件并启动插件窗口…");
        self.dismiss_guard.reset();
    }
    pub(super) fn draw_downloaded_device_plugin(&mut self,ui:&mut egui::Ui,device:&Device) {
        let epoch=self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst);
        let downloading=self.plugin_download_pending.get(&device.model)==Some(&epoch);
        ui.label(RichText::new("米家官方插件").size(12.).strong());
        ui.add(egui::Label::new(RichText::new("下载记录保存在本机。验签、SDK 和 Windows 兼容检查通过后，可从这里打开插件窗口。").size(10.).color(muted(self.dark))).wrap());
        if ui.add_enabled(!self.demo&&self.authenticated&&self.preferences.region=="cn"&&!downloading,
            egui::Button::new(if downloading{"正在下载并检查…"}else if self.downloaded_plugin(&device.model).is_some(){"检查更新 / 重新关联"}else{"下载此型号插件"})).clicked(){self.start_plugin_download(device);}
        if downloading {ui.horizontal(|ui|{ui.spinner();ui.label("正在查询、下载和检查文件，请稍候");});}
        if let Some(report)=self.downloaded_plugin(&device.model) {
            self.draw_plugin_report(ui,&device.model,&report);
            self.draw_plugin_run_buttons(ui,device,&report);
        }
        if let Some(Err(error))=self.plugin_download_results.get(&device.model) {ui.add(egui::Label::new(RichText::new(format!("最近下载失败：{error}")).color(Color32::from_rgb(190,85,65)).size(11.)).wrap());}
        if self.preferences.region!="cn"{ui.label(RichText::new("当前只验证中国大陆服务区").size(10.).color(muted(self.dark)));}
        if ui.button("查看所有已下载插件").clicked(){self.page=Page::Settings;self.selected_device=None;self.refresh_plugin_catalog();}
    }
    fn draw_plugin_report(&mut self,ui:&mut egui::Ui,model:&str,report:&DownloadReport) {
        ui.add(egui::Label::new(RichText::new(format!("已下载 · v{} · {:.1} MB · SDK {}",report.version,report.bytes as f64/1_000_000.,report.sdk_version.map(|v|v.to_string()).unwrap_or_else(||"未知".into()))).size(11.).color(design::accent(self.dark))).wrap());
        let busy=self.plugin_preparing.get(model)==Some(&self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst));
        if busy {ui.horizontal(|ui|{ui.spinner();ui.label("正在验签 / 获取 SDK / 准备运行文件…");});}
        if let Some(result)=self.plugin_prepared.get(model) {
            let text=match result {
                Ok(p)=>format!("本机 APK 信任根验签通过\n插件签名覆盖 {} / {} 字节；SDK 覆盖 {} / {} 字节。完整包另以已核对的 SHA-256 锁定。",p.package_signature.signed_bundle_bytes,p.package_signature.total_bundle_bytes,p.sdk_signature.signed_bundle_bytes,p.sdk_signature.total_bundle_bytes),
                Err(error)=>format!("准备未完成：{error}"),
            };
            ui.add(egui::Label::new(RichText::new(text).size(10.).color(muted(self.dark))).wrap());
        } else {
            ui.add(egui::Label::new(RichText::new("文件已保存；启动时重新验证签名、完整摘要和运行文件。").size(10.).color(muted(self.dark))).wrap());
        }
        if let Err(reason)=crate::plugin_prepare::profile(model,report) {ui.add(egui::Label::new(RichText::new(reason).size(10.).color(muted(self.dark))).wrap());}
    }
    fn draw_plugin_run_buttons(&mut self,ui:&mut egui::Ui,device:&Device,report:&DownloadReport) {
        let busy=self.plugin_preparing.get(&device.model)==Some(&self.cloud.epoch.load(std::sync::atomic::Ordering::SeqCst));
        let allowed=!self.demo&&self.authenticated&&!busy&&!self.plugin_launching&&crate::plugin_prepare::profile(&device.model,report).is_ok();
        ui.horizontal_wrapped(|ui|{
            if ui.add_enabled(allowed,egui::Button::new("校验并打开 · 只读")).clicked(){self.start_plugin_prepare(&device.model,report.clone(),Some((device.clone(),false)));}
            if ui.add_enabled(allowed,egui::Button::new("打开 · 允许控制")).on_hover_text("点击官方插件控件会控制当前这台设备，仍受公开规格和回读限制").clicked(){self.start_plugin_prepare(&device.model,report.clone(),Some((device.clone(),true)));}
        });
        if crate::plugin_signature::trust_info(&self.root).is_err(){ui.add(egui::Label::new(RichText::new("首次请在设置 → 已下载插件中导入你持有的米家 APK，建立本机验签根。").size(10.).color(muted(self.dark))).wrap());}
    }
    pub(super) fn draw_plugin_library(&mut self,ui:&mut egui::Ui) {
        ui.label(RichText::new("已下载插件").strong());
        ui.add(egui::Label::new(RichText::new("插件保存在当前 Windows 用户目录。官方脚本由独立宿主按需运行，关闭后释放宿主进程。").size(10.5).color(muted(self.dark))).wrap());
        let installed=crate::plugin_runtime::host_path().is_ok();
        ui.label(if installed{"Windows 插件宿主已安装"}else{"未安装插件宿主，请使用集成安装包"});
        let trusted=crate::plugin_signature::trust_info(&self.root).is_ok();
        ui.label(if trusted{"本机 APK 公开验签根已导入"}else{"尚未导入本机 APK 验签根"});
        ui.horizontal_wrapped(|ui|{
            if ui.add_enabled(!self.demo&&!self.plugin_importing,egui::Button::new(if self.plugin_importing{"正在导入…"}else{"导入本机米家 APK…"})).clicked(){
                match native::pick_mijia_apk(self.native_handle) {
                    Ok(Some(path))=>{
                        self.plugin_importing=true;let root=self.root.clone();let tx=self.events_tx.clone();let ctx=self.context.clone();
                        self.runtime.spawn_blocking(move||{let result=crate::plugin_signature::import_root_from_apk(&path,&root).map(|_|());let _=tx.send(UiMessage::PluginImported(result));ctx.request_repaint();});
                    },Ok(None)=>{},Err(error)=>self.report_error(error),
                }self.dismiss_guard.reset();
            }
            if ui.button("刷新列表").clicked(){self.refresh_plugin_catalog();}
        });
        if self.plugin_session.is_some()&&ui.button("关闭正在运行的插件").clicked(){self.plugin_session=None;self.notice("插件窗口已关闭");}
        if self.plugin_launching{ui.horizontal(|ui|{ui.spinner();ui.label("正在核对文件并启动插件窗口…");});}
        if let Some(error)=&self.plugin_catalog_error{ui.add(egui::Label::new(format!("列表读取失败：{error}")).wrap());}
        if self.plugin_catalog.is_empty(){ui.label("还没有下载插件。打开设备 → 信息 → 下载此型号插件。");}
        for entry in self.plugin_catalog.clone() {self.draw_plugin_library_entry(ui,&entry);}
        ui.add(egui::Label::new(RichText::new("验签基于你导入的 APK 公开根；未独立认证 APK 发行身份。仅开放已实测的包版本，手机专属服务会明确报告不可用。").size(10.).color(muted(self.dark))).wrap());
    }
    fn draw_plugin_library_entry(&mut self,ui:&mut egui::Ui,entry:&CatalogEntry) {
        let name=entry.model.clone().or_else(||entry.public_project.as_ref().and_then(|p|p.package_path.clone())).unwrap_or_else(||"旧版已下载包 · 未关联型号".into());
        let kind=if entry.kind==Some(PackageKind::Sdk){"共享 SDK"}else{"设备插件"};
        egui::CollapsingHeader::new(format!("{kind} · {name}" )).id_salt(&entry.sha256).default_open(entry.kind!=Some(PackageKind::Sdk)).show(ui,|ui|{
            ui.add(egui::Label::new(RichText::new(format!("{:.1} MB · {}",entry.bytes as f64/1_000_000.,if entry.cache_available{"已缓存"}else{"文件缺失"})).size(11.)).wrap());
            if let (Some(model),Some(report))=(&entry.model,&entry.report) {
                self.draw_plugin_report(ui,model,report);
                let devices:Vec<_>=self.devices.iter().filter(|d|&d.model==model).cloned().collect();
                if devices.is_empty(){ui.label(RichText::new("当前账号未找到匹配设备，不能启动设备控制").size(10.).color(muted(self.dark)));}
                for device in devices {
                    ui.label(RichText::new(format!("{} · {}",device.name,device.room)).size(11.).strong());
                    self.draw_plugin_run_buttons(ui,&device,report);
                }
            } else if entry.kind==Some(PackageKind::Sdk) {ui.label(RichText::new("共享依赖由对应设备插件加载").size(10.).color(muted(self.dark)));}
            else {ui.add(egui::Label::new(RichText::new("旧版缓存没有型号记录。请到对应设备重新查询一次，完整摘要相同会复用已有文件并建立关联。").size(10.).color(muted(self.dark))).wrap());}
        });
    }
}
