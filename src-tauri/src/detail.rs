//! Fixed-size device sheet. The content is a clipped child, so data can't resize its shell.
use super::*;
use crate::presentation;
#[path = "speaker.rs"]
mod speaker;

const SHEET_SIZE: Vec2 = design::DETAIL_SIZE;

fn option_popup_rect(viewport:egui::Rect,button:egui::Rect,count:usize)->egui::Rect {
    let bounds=egui::Rect::from_center_size(viewport.center(),SHEET_SIZE).shrink(16.0);
    let rows=count.clamp(1,5) as f32;
    let size=Vec2::new(button.width().min(bounds.width()),rows*32.0+(rows-1.0)*4.0+16.0);
    let position=egui::pos2(button.left().clamp(bounds.left(),bounds.right()-size.x),
        (button.bottom()+4.0).clamp(bounds.top(),bounds.bottom()-size.y));
    egui::Rect::from_min_size(position,size)
}

#[derive(Clone,Copy,Default,PartialEq,Eq)]
pub(super) enum DetailTab { #[default] Controls, Settings, Info, Speaker }

fn clipped_child(ui: &mut egui::Ui, builder: egui::UiBuilder, bounds: egui::Rect) -> egui::Ui {
    let mut child = ui.new_child(builder);
    child.set_clip_rect(ui.clip_rect().intersect(bounds));
    child
}

struct SheetResponse {
    rect: egui::Rect,
    close_rect: egui::Rect,
    closed: bool,
    refresh: bool,
}

fn sheet(
    ctx: &egui::Context,
    device: &Device,
    dark: bool,
    material: GlassStyle,
    demo: bool,
    sync_status: &str,
    status: Option<(&str, bool)>,
    body: impl FnOnce(&mut egui::Ui),
) -> SheetResponse {
    let response = egui::Modal::new(Id::new("device-detail-window"))
        .backdrop_color(Color32::TRANSPARENT)
        .frame(Frame::NONE)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(SHEET_SIZE, Sense::hover());
            design::paint_glass(ui.painter(), rect, dark, material);
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(rect.right() - 48.0, rect.top() + 16.0), Vec2::splat(32.0),
            );
            let close = design::round_icon(ui, Id::new("detail-close"), close_rect, Icon::Close, "关闭详情", dark);
            design::text(ui.painter(), rect.min + egui::vec2(22.0, 24.0), "设备控制", 11.0, muted(dark), 180.0);

            let art = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.top() + 84.0), Vec2::splat(60.0),
            );
            design::device_art(ui.painter(), art, &device.kind, dark);
            // A single ellipsized line has a stable height, even for long vendor names.
            let title_rect = egui::Rect::from_min_size(rect.min + egui::vec2(22.0, 125.0), Vec2::new(344.0, 27.0));
            let mut title = clipped_child(ui, egui::UiBuilder::new().max_rect(title_rect).layout(Layout::top_down(Align::Center)), title_rect);
            title.add(egui::Label::new(RichText::new(&device.name).size(20.0).color(design::ink(dark))).truncate());
            let caption = format!("{} · {} · {}", if device.room.is_empty() { "未分配房间" } else { &device.room }, presentation::device_category(device), if device.online { "在线" } else { "离线" });
            ui.painter().text(egui::pos2(rect.center().x, rect.top() + 159.0), Align2::CENTER_CENTER, caption, egui::FontId::proportional(11.0), muted(dark));

            let body_rect = egui::Rect::from_min_size(rect.min + egui::vec2(20.0, 183.0), Vec2::new(348.0, 298.0));
            let mut body_ui = clipped_child(ui, egui::UiBuilder::new().id_salt("detail-body").max_rect(body_rect), body_rect);
            body(&mut body_ui);

            let footer_y = rect.bottom() - 46.0;
            ui.painter().line_segment([egui::pos2(rect.left() + 22.0, footer_y), egui::pos2(rect.right() - 22.0, footer_y)], Stroke::new(1.0_f32, design::line(dark).gamma_multiply(0.6)));
            let (footer, color) = match status {
                Some((raw, error)) => {
                    let (text, pending) = toast_message(raw);
                    (text, if pending { Color32::from_rgb(166,119,43) } else if error { Color32::from_rgb(191,68,67) } else { design::accent(dark) })
                }
                None => (if demo { "演示预览 · 未连接真实设备" } else { sync_status }, muted(dark)),
            };
            design::text(ui.painter(), egui::pos2(rect.left() + 22.0, footer_y + 15.0), footer, 10.0, color, 280.0);
            let footer_rect = egui::Rect::from_min_size(egui::pos2(rect.left()+22.0, footer_y+7.0), Vec2::new(280.0,32.0));
            ui.interact(footer_rect, Id::new("detail-status"), Sense::hover()).on_hover_text(footer);
            let refresh_rect = egui::Rect::from_min_size(egui::pos2(rect.right() - 48.0, footer_y + 7.0), Vec2::splat(32.0));
            let refresh = design::round_icon(ui, Id::new("detail-refresh"), refresh_rect, Icon::Refresh, "刷新状态", dark);
            (rect, close_rect, close.clicked(), refresh.clicked())
        });
    let (rect, close_rect, close, refresh) = response.inner;
    debug_assert_eq!(response.response.rect.size(), SHEET_SIZE);
    SheetResponse { rect, close_rect, closed: close || response.should_close(), refresh }
}

impl HomePanelApp {
    pub(super) fn draw_detail(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected_device.clone() else { return; };
        let Some(device) = self.devices.iter().find(|device| device.id == id).cloned() else {
            self.selected_device = None;
            ctx.request_repaint();
            return;
        };
        let status = self.error.as_ref().map(|(text, _)| (text.clone(), true))
            .or_else(|| self.notice.as_ref().map(|(text, _)| (text.clone(), false)));
        let sync_status = if self.uses_speaker_service(&device) && matches!(self.detail_tab,DetailTab::Speaker|DetailTab::Controls) {
            if !self.mina_connected {"请先连接小爱音箱服务".into()}
            else if self.mina_pending.contains(&id) {"正在回读确认小爱音箱状态…".into()}
            else if self.mina_reading.contains(&id) {"正在读取小爱音箱状态…".into()}
            else if let Some(state)=self.mina_states.get(&id) {
                if state.error.is_some() {"部分状态读取失败 · 已保留上次结果".into()}
                else if state.current() {"小爱状态已读取 · 约每 3 秒查询".into()}
                else {"小爱状态已过期，请刷新".into()}
            }else {"等待小爱音箱状态…".into()}
        }else {crate::state_sync::device_status(&device, self.reading.contains(&id), crate::now_ms())};
        let response = sheet(ctx, &device, self.dark, self.material(), self.demo, &sync_status, status.as_ref().map(|(text, error)| (text.as_str(), *error)), |ui| {
            ui.multiply_opacity(self.frame_motion.opacity);
            ui.add_space(self.frame_motion.offset);
            let labels:&[&str]=if crate::speaker_service::eligible(&device) {&["控制","设置","信息","小爱"]}else{&["控制","设置","信息"]};
            let mut selected=(self.detail_tab as usize).min(labels.len()-1);
            design::segmented_tabs(ui,Id::new(("detail-tabs",&id)),labels,&mut selected,self.dark);
            let tab=[DetailTab::Controls,DetailTab::Settings,DetailTab::Info,DetailTab::Speaker][selected];
            if self.detail_tab!=tab {self.detail_tab=tab;self.discard_unsubmitted_drafts();}
            ui.add_space(8.0);
            egui::ScrollArea::vertical().id_salt(("detail-controls", &id,self.detail_tab as u8)).max_height(252.0)
                .auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 10.0;
                    if self.detail_tab==DetailTab::Speaker {self.draw_speaker_service(ui,&device);return;}
                    if self.detail_tab==DetailTab::Info {self.draw_device_info(ui,&device);return;}
                    if self.detail_tab==DetailTab::Settings {self.draw_device_settings(ui,&device);return;}
                    if device.kind == "speaker" { self.draw_speaker_controls(ui, &device); }
                    else if let Some(power) = power_property(&device).cloned() { self.draw_power_row(ui, &device, &power); }
                    let primary_power = power_property(&device).map(|p| (p.siid, p.piid));
                    let adapted=if device.kind=="speaker" {HashSet::new()} else {
                        self.draw_adapter_sections(ui,&device,crate::adapters::Page::Controls,primary_power)
                    };
                    let properties=crate::adapters::remaining(&device,crate::adapters::Page::Controls,primary_power,&adapted);
                    if device.kind == "speaker" {
                        // The dedicated player already renders daily speaker properties.
                    } else if device.kind == "sensor" && properties.iter().all(|property| !property.writable) {
                        if let Some(primary) = properties.iter().find(|property| property.key == "temperature") {
                            self.draw_reading(ui, &device, primary, 108.0, 38.0);
                        }
                        let others: Vec<_> = properties.iter().filter(|property| property.key != "temperature").collect();
                        for row in others.chunks(2) {
                            ui.columns(2, |columns| {
                                for (column, property) in columns.iter_mut().zip(row) { self.draw_reading(column, &device, property, 90.0, 25.0); }
                            });
                        }
                    } else {
                        // Controls stay full width; readings share compact, stable two-column tiles.
                        for property in properties.iter().filter(|p| p.writable) { self.draw_property(ui, &device, property); }
                        let mut readings: Vec<_> = properties.iter().filter(|p| !p.writable).copied().collect();
                        readings.sort_by_key(|p| presentation::electrical_order(p));
                        for row in readings.chunks(2) {
                            ui.columns(2, |columns| {
                                for (column, property) in columns.iter_mut().zip(row) {
                                    self.draw_reading(column, &device, property, 88.0, 24.0);
                                }
                            });
                        }
                    }
                    for action in &device.actions {
                        if device.kind=="speaker" && matches!(action.key.as_str(),"play"|"pause"|"stop"|"next"|"previous") {continue;}
                        let enabled=device.online&&!self.device_busy(&id);
                        if ui.add_enabled(enabled,egui::Button::new(&action.name).corner_radius(13).min_size(Vec2::new(ui.available_width(),38.))).on_disabled_hover_text(if device.online{"正在确认其他操作，请稍候"}else{"设备离线"}).clicked() { self.run_action(&id, action); }
                    }
                    if device.properties.is_empty() { ui.label(RichText::new("设备暂未提供可用功能").size(12.0).color(muted(self.dark))); }
                });
        });
        if response.refresh {
            self.read_device(&id);
            if device.kind=="speaker" {self.request_speaker_state(&device);}
        }
        if response.closed { self.selected_device = None; self.discard_unsubmitted_drafts();ctx.request_repaint(); }
        // Geometry is intentionally independent of data, pending requests and drag values.
        debug_assert_eq!(response.rect.size(), SHEET_SIZE);
        debug_assert!(response.rect.contains_rect(response.close_rect));
    }

    fn draw_device_settings(&mut self,ui:&mut egui::Ui,device:&Device) {
        let power=power_property(device).map(|p|(p.siid,p.piid));
        let adapted=self.draw_adapter_sections(ui,device,crate::adapters::Page::Settings,power);
        let properties=crate::adapters::remaining(device,crate::adapters::Page::Settings,power,&adapted);
        if properties.is_empty() {
            if adapted.is_empty() {ui.label(RichText::new("此设备暂未提供其他可用设置").size(12.0).color(muted(self.dark)));}
            return;
        }
        for (group,title) in [(0,"按键与点动"),(1,"指示灯"),(2,"通电与记忆"),(3,"其他设置")] {
            let members:Vec<_>=properties.iter().filter(|p|presentation::setting_group(p)==group).collect();
            if members.is_empty(){continue;}
            ui.label(RichText::new(title).size(12.0).strong().color(muted(self.dark)));
            for property in members {
                self.draw_property(ui,device,property);
                if let Some(help)=presentation::property_help(property) {
                    ui.add(egui::Label::new(RichText::new(help).size(10.0).color(muted(self.dark))).wrap());
                }
            }
        }
    }

    fn draw_device_info(&mut self,ui:&mut egui::Ui,device:&Device) {
        for (label,value) in [("设备类型",presentation::device_category(device).to_owned()),("设备型号",device.model.clone()),
            ("所在房间",if device.room.is_empty(){"未分配房间".into()}else{device.room.clone()}),
            ("状态来源",crate::state_sync::device_status(device,self.reading.contains(&device.id),crate::now_ms()))] {
            ui.label(RichText::new(label).size(10.5).color(muted(self.dark)));
            ui.add(egui::Label::new(RichText::new(value).size(12.0)).wrap().selectable(true));
        }
        ui.separator();
        let panel=if self.preferences.native_panels {self.adapters.resolve(device)}else{None};
        ui.label(RichText::new("设备面板").size(10.5).color(muted(self.dark)));
        ui.add(egui::Label::new(RichText::new(panel.map(|pack|format!("{} · 适配包 v{}",pack.name,pack.version)).unwrap_or_else(||"基础控制面板".into())).size(12.0)).wrap());
        ui.add(egui::Label::new(RichText::new("未适配属性继续显示在基础控件中。官方插件按版本逐项测试。").size(10.0).color(muted(self.dark))).wrap());
        self.draw_downloaded_device_plugin(ui,device);
        ui.label(RichText::new("支持的功能").size(12.0).strong());
        let readable=device.properties.iter().filter(|p|p.readable).count();
        let writable=device.properties.iter().filter(|p|p.writable).count();
        ui.label(RichText::new(format!("{readable} 项状态 · {writable} 项设置 · {} 项操作",device.actions.len())).size(11.0).color(muted(self.dark)));
        ui.add(egui::Label::new(RichText::new("按设备公开规格提供功能。第三方厂商的专属插件功能可能需要单独适配；设备名称不会改变设备类型。").size(11.0).color(muted(self.dark))).wrap());
        egui::CollapsingHeader::new("兼容信息").show(ui,|ui| {
            ui.add(egui::Label::new(RichText::new(if device.spec_urn.is_empty(){"尚未获取功能规格"}else{&device.spec_urn}).size(10.0)).wrap().selectable(true));
        });
    }

    fn draw_adapter_sections(&mut self,ui:&mut egui::Ui,device:&Device,page:crate::adapters::Page,primary:Option<(u32,u32)>)->HashSet<(u32,u32)> {
        let mut used=HashSet::new();
        if !self.preferences.native_panels {return used;}
        let Some(pack)=self.adapters.resolve(device).cloned() else {return used;};
        for section in crate::adapters::bind(&pack,device,page) {
            let properties:Vec<_>=section.properties.into_iter().filter(|p|Some((p.siid,p.piid))!=primary).collect();
            if properties.is_empty() {continue;}
            ui.label(RichText::new(&section.section.title).size(11.0).strong().color(muted(self.dark)));
            match section.section.style {
                crate::adapters::Style::Controls=>for property in &properties {self.draw_property(ui,device,property);},
                crate::adapters::Style::Readings=>for row in properties.chunks(2) {
                    ui.columns(2,|columns|for (column,property) in columns.iter_mut().zip(row) {
                        self.draw_reading(column,device,property,88.0,24.0);
                    });
                },
            }
            used.extend(properties.iter().map(|p|(p.siid,p.piid)));
        }
        used
    }

    fn draw_power_row(&mut self, ui: &mut egui::Ui, device: &Device, property: &Property) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), Sense::hover());
        design::control_surface(ui.painter(), rect, self.dark, self.material());
        let shown = displayed_property(property, &self.control_drafts, &property_control_key(device, property));
        let state = shown.value.as_ref().and_then(Value::as_bool);
        let busy = self.device_busy(&device.id);
        design::text(ui.painter(), rect.min + egui::vec2(16.0, 10.0), &presentation::property_title(device, property), 12.0, design::ink(self.dark), 200.0);
        let text=if busy {"正在确认…".into()}else{presentation::reported_text(property,if state==Some(true){"已开启"}else if state==Some(false){"已关闭"}else{"暂无状态"},crate::now_ms())};
        design::text(ui.painter(), rect.min + egui::vec2(16.0, 32.0), &text, 10.5, muted(self.dark), 220.0);
        let button_rect = egui::Rect::from_center_size(egui::pos2(rect.right() - 30.0, rect.center().y), Vec2::splat(36.0));
        if design::power_button(ui, Id::new(("detail-power", &device.id)), button_rect, state == Some(true), device.online && !busy && state.is_some() && property.error.is_none(), busy, self.dark).clicked() {
            self.write_property(&device.id, property, json!(!state.unwrap_or(false)));
        }
    }

    fn draw_reading(&self, ui: &mut egui::Ui, device: &Device, property: &Property, height: f32, size: f32) {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        design::control_surface(ui.painter(), rect, self.dark, self.material());
        let reading = presentation::reading(device, property);
        let width = rect.width() - 28.0;
        let p = ui.painter();
        design::text(p, rect.min + egui::vec2(14.0, 11.0), &presentation::property_title(device, property), 11.0, muted(self.dark), width);
        let size = if reading.valid { size } else { 16.0 };
        let value = p.layout_no_wrap(reading.value.clone(), egui::FontId::proportional(size), design::ink(self.dark));
        let suffix_width = if reading.unit.is_empty() { 0.0 } else { 31.0 };
        design::text(p, rect.min + egui::vec2(14.0, 32.0), &reading.value, size, design::ink(self.dark), width - suffix_width);
        if !reading.unit.is_empty() {
            design::text(p, rect.min + egui::vec2(18.0 + value.size().x.min(width - suffix_width), 40.0), &reading.unit, 11.0, muted(self.dark), suffix_width);
        }
        if let Some(note) = reading.note {
            design::text(p, rect.min + egui::vec2(14.0, height - 19.0), note, 9.0, muted(self.dark), width);
        }
        if let Some(error) = property.error.as_ref() { response.on_hover_text(error); }
        else { response.on_hover_text(format!("{}：{}\n{}", presentation::property_title(device, property), reading.text(), crate::state_sync::property_provenance(property, crate::now_ms()))); }
    }

    fn draw_property(&mut self, ui: &mut egui::Ui, device: &Device, property: &Property) {
        if !property.writable {
            let compact=property.format=="string" || !property.options.is_empty();
            self.draw_reading(ui, device, property, if compact {64.0}else{76.0}, if compact {15.0}else{24.0});
            return;
        }
        if crate::time_range::recognizes(property) { self.draw_time_property(ui,device,property); return; }
        let height = if property.format == "bool" { 56.0 } else { 88.0 };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        design::control_surface(ui.painter(), rect, self.dark, self.material());
        let key = property_control_key(device, property);
        ui.interact(rect, Id::new((&key, "provenance")), Sense::hover())
            .on_hover_text(crate::state_sync::property_provenance(property, crate::now_ms()));
        let shown = displayed_property(property, &self.control_drafts, &key);
        let p = ui.painter();
        design::text(p, rect.min + egui::vec2(16.0, 12.0), &presentation::property_title(device, property), 12.0, design::ink(self.dark), rect.width() - 126.0);
        // The value occupies its own fixed column and cannot move the slider or close button.
        let value_rect = egui::Rect::from_min_size(egui::pos2(rect.right() - 119.0, rect.top() + 10.0), Vec2::new(103.0, 24.0));
        let enabled = device.online && !self.device_busy(&device.id) && property.error.is_none() && (!property.readable || property.value.is_some());
        let control_rect = egui::Rect::from_min_max(rect.min + egui::vec2(14.0, 39.0), rect.max - egui::vec2(14.0, 9.0));
        let mut controls = clipped_child(ui, egui::UiBuilder::new().id_salt(&key).max_rect(control_rect), rect.shrink(3.0));
        if property.format == "bool" {
            if property.error.is_some() || (property.readable && property.value.is_none()) {
                design::text(ui.painter(), rect.min + egui::vec2(16.0, 34.0), "状态未获取", 10.0, muted(self.dark), rect.width()-90.0);
            }
            let toggle_rect = egui::Rect::from_min_size(egui::pos2(rect.right() - 60.0, rect.top() + 18.0), Vec2::new(44.0, 24.0));
            let mut control = ui.new_child(egui::UiBuilder::new().id_salt((&key, "toggle")).max_rect(toggle_rect));
            control.set_clip_rect(ui.clip_rect().intersect(toggle_rect));
            let mut value = shown.value.as_ref().and_then(Value::as_bool).unwrap_or(false);
            control.add_enabled_ui(enabled, |ui| { if design::toggle(ui, &mut value).changed() { self.write_property(&device.id, property, json!(value)); } });
        } else if !property.options.is_empty() {
            controls.add_enabled_ui(enabled, |ui| self.draw_option_property(ui, device, &shown));
        } else if matches!(property.format.as_str(), "float" | "double") || property.format.starts_with("uint") || property.format.starts_with("int") {
            controls.add_enabled_ui(enabled, |ui| self.draw_slider_property(ui, device, property));
        } else if property.format == "string" {
            controls.add_enabled_ui(enabled, |ui| self.draw_text_property(ui, device, property));
        }
        if property.format != "bool" {
            let shown = displayed_property(property, &self.control_drafts, &key);
            let mut value_ui = clipped_child(ui, egui::UiBuilder::new().max_rect(value_rect).layout(Layout::right_to_left(Align::Center)), value_rect);
            value_ui.add(egui::Label::new(RichText::new(presentation::value_text(&shown)).size(12.0).color(muted(self.dark))).truncate());
        }
    }

    fn draw_option_property(&mut self, ui: &mut egui::Ui, device: &Device, property: &Property) {
        let key = property_control_key(device, property);
        let value = property.value.as_ref();
        // Daily modes use a single click; a longer list gets a bounded menu.
        if property.options.len() <= 3 {
            ui.columns(property.options.len(), |columns| {
                for (column, option) in columns.iter_mut().zip(&property.options) {
                    let selected = value == Some(&option.value);
                    let label = presentation::option_text(property, option);
                    let button = egui::Button::new(RichText::new(label).size(11.5).color(if selected { design::accent(self.dark) } else { muted(self.dark) }))
                        .fill(if selected { design::surface(self.dark) } else { Color32::TRANSPARENT })
                        .stroke(Stroke::new(1.0_f32, if selected { design::line(self.dark) } else { Color32::TRANSPARENT }))
                        .corner_radius(12).truncate().min_size(Vec2::new(column.available_width(), 32.0));
                    if column.add(button).on_hover_text(presentation::option_tooltip(property, option)).clicked() && !selected { self.write_property(&device.id, property, option.value.clone()); }
                }
            });
        } else {
            let text = property.options.iter().find(|option| Some(&option.value) == value)
                .map(|option| presentation::option_text(property, option)).unwrap_or_else(|| "选择模式".into());
            let button=ui.push_id(&key,|ui|ui.add_sized([ui.available_width()-10.0,32.0],egui::Button::new(format!("{text}  ▾")).corner_radius(10).truncate())).inner;
            let popup=option_popup_rect(ui.ctx().content_rect(),button.rect,property.options.len());
            egui::Popup::menu(&button).at_position(popup.min).align_alternatives(&[])
                .frame(design::popup_frame(self.dark)).width(popup.width()).show(|ui| {
                    ui.set_width(popup.width()-16.0);ui.spacing_mut().item_spacing.y=4.0;
                    egui::ScrollArea::vertical().max_height(popup.height()-16.0).show(ui,|ui| {
                        for option in &property.options {
                            let selected=Some(&option.value)==value;
                            let response=ui.add_sized([ui.available_width(),32.0],egui::Button::selectable(selected,presentation::option_text(property,option)).truncate())
                                .on_hover_text(presentation::option_tooltip(property,option));
                            if response.clicked() && !selected {self.write_property(&device.id,property,option.value.clone());}
                        }
                    });
                });
        }
    }

    fn draw_slider_property(&mut self, ui: &mut egui::Ui, device: &Device, property: &Property) {
        let Some(range) = property.range.as_ref().filter(|r| r.len() >= 2 && r[0].is_finite() && r[1].is_finite() && r[1] > r[0]) else { return; };
        let key = property_control_key(device, property);
        let mut value = self.control_drafts.get(&key).and_then(Value::as_f64)
            .or_else(|| property.value.as_ref().and_then(Value::as_f64)).unwrap_or(range[0]);
        let response = design::glass_slider(ui, Id::new((&key, "slider")), &mut value, range, property.format.starts_with("uint") || property.format.starts_with("int"), self.dark);
        if response.changed() {
            self.control_drafts.insert(key.clone(), numeric_value(property, value));
        }
        let pointer_up = !ui.input(|i| i.pointer.any_down());
        if ui.is_enabled() && (response.drag_stopped() || (response.changed() && pointer_up)) {
            if let Some(value) = self.control_drafts.get(&key).cloned() { self.write_property(&device.id, property, value); }
        }
    }

    fn draw_text_property(&mut self, ui: &mut egui::Ui, device: &Device, property: &Property) {
        let key = property_control_key(device, property);
        let mut value = self.control_drafts.get(&key).and_then(Value::as_str)
            .or_else(|| property.value.as_ref().and_then(Value::as_str)).unwrap_or_default().to_owned();
        ui.horizontal(|ui| {
            ui.add_sized([ui.available_width() - 66.0, 30.0], egui::TextEdit::singleline(&mut value));
            if ui.button("应用").clicked() { self.control_drafts.remove(&key); self.write_property(&device.id, property, json!(value)); }
            else { self.control_drafts.insert(key, json!(value)); }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_option_menus_stay_inside_the_native_detail_silhouette() {
        let viewport=egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(460.0,620.0));
        let visible=egui::Rect::from_center_size(viewport.center(),SHEET_SIZE).shrink(16.0);
        for y in [270.0,450.0,500.0] {
            for count in [4,5,64] {
                let button=egui::Rect::from_min_size(egui::pos2(70.0,y),Vec2::new(306.0,32.0));
                assert!(visible.contains_rect(option_popup_rect(viewport,button,count)));
            }
        }
    }

    #[test]
    fn scrolled_controls_cannot_paint_over_header_or_footer() {
        let ctx = egui::Context::default();
        let viewport = egui::Rect::from_min_max(egui::pos2(40.0, 200.0), egui::pos2(388.0, 480.0));
        let output = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(460.0, 620.0))), ..Default::default() }, |ctx| {
            egui::CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
                ui.set_clip_rect(viewport);
                for y in [175.0, 450.0] {
                    let card = egui::Rect::from_min_size(egui::pos2(40.0, y), Vec2::new(348.0, 88.0));
                    let mut child = clipped_child(ui, egui::UiBuilder::new().id_salt(y as i32).max_rect(card), card.shrink(3.0));
                    let mut value = 50.0;
                    design::glass_slider(&mut child, Id::new(y as i32), &mut value, &[0.0,100.0,1.0], true, false);
                }
            });
        });
        let painted: Vec<_> = output.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == design::accent(false))).collect();
        assert!(!painted.is_empty());
        for shape in painted { assert!(viewport.contains_rect(shape.clip_rect), "control escaped scroll viewport: {:?}", shape.clip_rect); }
    }

    #[test]
    fn long_labels_and_property_changes_cannot_resize_or_move_the_sheet() {
        let ctx = egui::Context::default();
        let mut measured = Vec::new();
        for (name, content) in [("传感器".to_owned(), "23.4°C".to_owned()), ("设备名称".repeat(24), "长属性".repeat(100))] {
            for _ in 0..3 {
                let _ = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(460.0, 620.0))), ..Default::default() }, |ctx| {
                    let response = sheet(ctx, &Device { name: name.clone(), ..Default::default() }, false, GlassStyle::Frosted, true, "等待首次读取", None, |ui| { ui.label(&content); });
                    assert_eq!(response.rect.size(), SHEET_SIZE);
                    assert_eq!(response.close_rect.min - response.rect.min, egui::vec2(340.0, 16.0));
                    measured.push(response.rect);
                });
            }
        }
        assert_eq!(measured[2], measured[5]);
    }
}
