//! Original native panel for the separately authorized XiaoAI service.
use super::*;
use crate::speaker_service::Command;
use std::sync::atomic::Ordering;

impl HomePanelApp {
    fn start_speaker_login(&mut self) {
        if self.demo {self.notice("演示模式不会连接真实音箱账号");return;}
        if self.preferences.region!="cn" {self.report_error("小爱专用服务当前只适配中国大陆地区".into());return;}
        self.invalidate_mina_requests();
        self.mina_login_loading=true;self.mina_connected=false;self.mina_states.clear();self.mina_qr=None;
        let auth=self.mina.auth.clone();let tx=self.events_tx.clone();let context=self.context.clone();
        let epoch=self.cloud.epoch.load(Ordering::SeqCst);let generation=self.mina_generation;
        self.runtime.spawn(async move {
            match auth.begin_login().await {
                Ok((epoch,url,client,jar))=>auth.poll_login(epoch,url,client,jar).await,
                Err(error)=>{let _=tx.send(UiMessage::MinaLoginError(epoch,generation,error));},
            }
            context.request_repaint();
        });
    }
    pub(super) fn request_speaker_state(&mut self,device:&Device) {
        if self.demo || !self.uses_speaker_service(device) || self.mina_pending.contains(&device.id) || !self.mina_reading.insert(device.id.clone()){return;}
        let mina=self.mina.clone();let main=self.cloud.clone();let device=device.clone();let tx=self.events_tx.clone();let context=self.context.clone();let epoch=main.epoch.load(Ordering::SeqCst);
        let started_at=crate::now_ms();
        let generation=self.mina_generation;
        self.runtime.spawn(async move {
            let result=tokio::time::timeout(Duration::from_secs(12),mina.read(&main,&device)).await.unwrap_or_else(|_|Err("小爱状态读取超时，请稍后重试".into()));
            let _=tx.send(UiMessage::MinaRead(device.id,epoch,generation,started_at,result));context.request_repaint();
        });
    }
    pub(super) fn poll_speaker_state(&mut self) {
        if self.demo || !self.visible || !self.authenticated || !self.mina_connected{return;}
        let Some(device)=self.selected_device.as_ref().and_then(|id|self.devices.iter().find(|d|&d.id==id && crate::speaker_service::eligible(d))).cloned()else{return;};
        let due=self.mina_states.get(&device.id).is_none_or(|s|crate::now_ms().saturating_sub(s.at)>=if s.error.is_some(){5_000}else{3_000});
        if due {self.request_speaker_state(&device);}
    }
    pub(super) fn run_speaker_command(&mut self,device:&Device,command:Command) {
        if self.demo {self.notice("演示模式不会修改真实音箱");return;}
        if self.device_busy(&device.id) {return;}
        let state=self.mina_states.get(&device.id).cloned().unwrap_or_default();
        if let Err(error)=command.validate(device,&state){self.report_error(error);return;}
        if !self.mina_pending.insert(device.id.clone()){return;}
        let draft=command_draft(&command);
        if let Some((key,value))=draft {self.control_drafts.insert(format!("mina:{}:{key}",device.id),value);}
        self.context.request_repaint();
        let mina=self.mina.clone();let main=self.cloud.clone();let device=device.clone();let tx=self.events_tx.clone();let context=self.context.clone();let epoch=main.epoch.load(Ordering::SeqCst);
        let generation=self.mina_generation;
        self.runtime.spawn(async move {
            let result=tokio::time::timeout(Duration::from_secs(25),mina.control(&main,&device,command)).await.unwrap_or_else(|_|Err("UNCERTAIN: 音箱操作超时，修改可能已发送，请刷新确认".into()));
            let _=tx.send(UiMessage::MinaWritten(device.id,epoch,generation,result));context.request_repaint();
        });
    }
    pub(super) fn draw_speaker_service(&mut self,ui:&mut egui::Ui,device:&Device) {
        if self.preferences.region!="cn" {ui.label("小爱专用接口当前只适配中国大陆地区。");return;}
        if !self.mina_connected {
            ui.label(RichText::new("连接小爱音箱服务").size(15.0).strong());
            ui.add(egui::Label::new("米家开关接口与小爱播放接口分别授权。连接同一个小米账号后，可读取歌曲与播放状态、调节进度及循环方式。").wrap());
            if self.mina_login.stage=="waiting" {
                if let Some(qr)=&self.mina_login.qr {
                    if self.mina_qr.as_ref().is_none_or(|(old,_)|old!=qr) {self.mina_qr=decode_texture(ui.ctx(),"speaker-login-qr",qr).map(|texture|(qr.clone(),texture));}
                    if let Some((_,texture))=&self.mina_qr {egui::Frame::new().fill(Color32::WHITE).corner_radius(12).inner_margin(8).show(ui,|ui|{ui.add(egui::Image::new((texture.id(),Vec2::splat(146.0))));});}
                }
                ui.add(egui::Label::new(&self.mina_login.message).wrap());
                if self.mina_login.expires_at.is_some_and(|at|crate::now_ms()>at) && ui.button("二维码过期，重新获取").clicked(){self.start_speaker_login();}
            } else {
                if !self.mina_login.message.is_empty(){ui.add(egui::Label::new(&self.mina_login.message).wrap());}
                if ui.add_enabled(!self.mina_login_loading,egui::Button::new(if self.mina_login_loading{"正在获取二维码…"}else{"扫码连接小爱服务"})).clicked(){self.start_speaker_login();}
            }
            return;
        }
        ui.horizontal(|ui|{
            ui.label(RichText::new("小爱音箱服务").size(12.0).strong());
            let reading=self.mina_reading.contains(&device.id)||self.device_busy(&device.id);
            if ui.add_enabled(!reading,egui::Button::new(if reading{"读取中…"}else{"刷新"}).small()).clicked(){self.request_speaker_state(device);}
            if ui.small_button("重新连接").clicked(){self.start_speaker_login();}
        });
        let state=self.mina_states.get(&device.id).cloned().unwrap_or_default();
        let busy=self.device_busy(&device.id);
        let enabled=state.available(device)&&!busy;
        let disabled_reason=if !state.available(device){"小爱音箱离线，请先检查设备连接"}else if busy{"正在回读确认，请稍候"}
            else if !state.current(){"播放状态尚未更新，请刷新；其他已读功能可独立使用"}else{"当前型号尚未确认此操作的控制方式"};
        let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),155.0),Sense::hover());
        design::control_surface(ui.painter(),rect,self.dark,self.material());
        design::text(ui.painter(),rect.min+egui::vec2(14.,10.),if busy{"正在确认音箱状态…"}else{"当前播放"},11.,muted(self.dark),rect.width()-28.);
        design::text(ui.painter(),rect.min+egui::vec2(14.,31.),&if state.at==0 {"正在读取…".into()}else{state.state_text()},17.,design::ink(self.dark),rect.width()-28.);
        let title=if state.title.is_empty(){if state.error.is_some(){"播放读取失败，其他状态单独更新"}else{"当前音源未提供歌曲信息"}}else{&state.title};
        design::text(ui.painter(),rect.min+egui::vec2(14.,58.),title,11.5,muted(self.dark),rect.width()-28.);
        design::text(ui.painter(),rect.min+egui::vec2(14.,77.),&state.artist,10.,muted(self.dark),rect.width()-28.);
        if let Some(error)=&state.error {ui.interact(rect,Id::new(("mina-play-error",&device.id)),Sense::hover()).on_hover_text(error);}
        let row=egui::Rect::from_min_size(rect.min+egui::vec2(10.,103.),Vec2::new(rect.width()-20.,36.));
        let mut transport=ui.new_child(egui::UiBuilder::new().id_salt(("mina-transport",&device.id)).max_rect(row));transport.set_clip_rect(ui.clip_rect().intersect(row));
        let mut action=None;
        transport.add_enabled_ui(enabled,|ui|ui.columns(5,|columns|for (column,(label,key)) in columns.iter_mut().zip([("上一首","prev"),(state.play_label(),"play"),("暂停","pause"),("停止","stop"),("下一首","next")]) {
            let permission=Command::Play(key).validate(device,&state);
            let reason=if !enabled{disabled_reason}else{permission.as_ref().err().map(String::as_str).unwrap_or("修改后回读确认")};
            if column.add_enabled(permission.is_ok(),egui::Button::new(RichText::new(label).size(10.5)).corner_radius(10).min_size(Vec2::new(column.available_width(),32.))).on_hover_text("修改后回读确认").on_disabled_hover_text(reason).clicked(){action=Some(Command::Play(key));}
        }));
        if let Some(command)=action {self.run_speaker_command(device,command);}
        for (key,label,value,max) in [("volume","音量",state.volume.map(|v|v as f64),100.0),("position","播放进度",state.projected_position().map(|v|v as f64),state.duration_ms.unwrap_or(0) as f64)] {
            let draft_key=format!("mina:{}:{key}",device.id);
            let mut shown=self.control_drafts.get(&draft_key).and_then(Value::as_f64).or(value).unwrap_or(0.0);
            let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),88.),Sense::hover());design::control_surface(ui.painter(),rect,self.dark,self.material());
            design::text(ui.painter(),rect.min+egui::vec2(14.,10.),label,11.5,design::ink(self.dark),100.);
            let display=if state.at==0&&value.is_none(){"正在读取…".into()}else if value.is_none() || max<=0. {"此音源暂未提供".into()}else if key=="volume"{format!("{}{}%",if state.current(){""}else{"上次 "},shown.round())}else{format!("{}{} / {}",if state.current(){""}else{"上次 "},clock_ms(shown as u64),clock_ms(max as u64))};
            ui.painter().text(rect.right_top()+egui::vec2(-14.,16.),Align2::RIGHT_CENTER,display,egui::FontId::proportional(10.5),muted(self.dark));
            let slider_rect=egui::Rect::from_min_size(rect.min+egui::vec2(12.,37.),Vec2::new(rect.width()-24.,35.));
            let mut controls=ui.new_child(egui::UiBuilder::new().id_salt(&draft_key).max_rect(slider_rect));controls.set_clip_rect(ui.clip_rect().intersect(slider_rect));
            let permission=if key=="volume"{Command::Volume(shown.round() as u8)}else{Command::Seek(shown.round() as u64)}.validate(device,&state);
            let reason=if !enabled{disabled_reason}else{permission.as_ref().err().map(String::as_str).unwrap_or("松手提交，修改后回读确认")};
            let response=controls.add_enabled_ui(enabled&&value.is_some()&&max>0.&&permission.is_ok(),|ui|design::glass_slider(ui,Id::new(&draft_key),&mut shown,&[0.,max.max(1.),if key=="volume"{1.}else{1000.}],true,self.dark)).inner.on_disabled_hover_text(reason);
            if response.changed(){self.control_drafts.insert(draft_key,json!(shown.round() as u64));}
            if enabled && (response.drag_stopped()||(response.changed()&&!ui.input(|i|i.pointer.any_down()))) {
                self.run_speaker_command(device,if key=="volume" {Command::Volume(shown.round() as u8)}else{Command::Seek(shown.round() as u64)});
            }
        }
        ui.label(RichText::new("循环方式").size(11.5));
        let (shown_mode,pending_mode)=loop_feedback(state.loop_type,self.control_drafts.get(&format!("mina:{}:loop",device.id)),self.mina_pending.contains(&device.id));
        let mut mode=None;
        ui.add_enabled_ui(enabled&&state.loop_type.is_some(),|ui|ui.columns(4,|columns|for (index,(column,label)) in columns.iter_mut().zip(["单曲","列表","顺序","随机"]).enumerate(){
            let permission=Command::Loop(index as u8).validate(device,&state);
            let reason=if !enabled {disabled_reason}else if state.loop_type.is_none(){"尚未读到循环状态，请刷新"}else{permission.as_ref().err().map(String::as_str).unwrap_or("可设置，修改后回读确认")};
            if column.add_enabled(permission.is_ok(),egui::Button::selectable(shown_mode==Some(index as u8),label).corner_radius(10).min_size(Vec2::new(column.available_width(),30.))).on_hover_text("修改后回读确认").on_disabled_hover_text(reason).clicked() && state.loop_type!=Some(index as u8){mode=Some(index as u8);}
        }));
        let supported:Vec<_>=["单曲","列表","顺序","随机"].into_iter().enumerate().filter(|(index,_)|Command::Loop(*index as u8).validate(device,&state).is_ok()).map(|(_,label)|label).collect();
        let hint=if let Some(label)=pending_mode{format!("正在设置：{label} · 等待设备回读")}else if !enabled{disabled_reason.into()}else if state.loop_type.is_none(){"循环状态未读到，请刷新".into()}else if supported.is_empty(){"循环模式只读：此型号的控制方式尚未确认".into()}else{format!("可设置：{}",supported.join("、"))};
        ui.label(RichText::new(hint).size(10.).color(muted(self.dark)));
        if let Some(mode)=mode{self.run_speaker_command(device,Command::Loop(mode));}
        for (key,label,reported) in [("sleep","睡眠模式",state.sleeping),("bluetooth","音箱蓝牙",state.bluetooth)] {
            let draft_key=format!("mina:{}:{key}",device.id);
            let mut value=self.control_drafts.get(&draft_key).and_then(Value::as_bool).or(reported).unwrap_or(false);
            let permission=if key=="sleep"{Command::Sleep(value)}else{Command::Bluetooth(value)}.validate(device,&state);
            let read_error=if key=="sleep"{state.sleep_error.as_deref()}else{state.bluetooth_error.as_deref()};
            let reason=if !enabled {disabled_reason}else if reported.is_none(){read_error.unwrap_or("尚未读取到此功能的状态，请刷新")}
                else {permission.as_ref().err().map(String::as_str).unwrap_or("可切换，修改后回读确认")};
            let fresh=if key=="sleep"{state.sleep_current()}else{state.bluetooth_current()};
            let badge=if reported.is_none(){"状态未读到"}else if !fresh{"上次状态"}else if permission.is_err(){"仅可查看"}else{"可控制"};
            design::settings_frame(self.dark,self.material()).show(ui,|ui|ui.horizontal(|ui|{
                ui.label(RichText::new(label).size(11.5));
                ui.with_layout(Layout::right_to_left(Align::Center),|ui|{
                    ui.add_enabled_ui(enabled&&reported.is_some()&&permission.is_ok(),|ui|{if design::toggle(ui,&mut value).on_disabled_hover_text(reason).changed(){self.run_speaker_command(device,if key=="sleep"{Command::Sleep(value)}else{Command::Bluetooth(value)});}});
                    ui.label(RichText::new(badge).size(9.5).color(muted(self.dark))).on_hover_text(reason);
                });
            }));
        }
        ui.add(egui::Label::new(RichText::new("约每 3 秒查询，修改后回读确认。已适配型号可使用手机专用功能；蓝牙开关控制音箱连接，不是电脑音频推流。").size(9.5).color(muted(self.dark))).wrap());
    }
}
fn clock_ms(ms:u64)->String {let s=ms/1000;format!("{}:{:02}",s/60,s%60)}

fn command_draft(command:&Command)->Option<(&'static str,Value)> {
    match command {
        Command::Volume(value)=>Some(("volume",json!(value))),Command::Seek(value)=>Some(("position",json!(value))),
        Command::Loop(value)=>Some(("loop",json!(value))),Command::Sleep(value)=>Some(("sleep",json!(value))),
        Command::Bluetooth(value)=>Some(("bluetooth",json!(value))),_=>None,
    }
}

fn loop_feedback(reported:Option<u8>,draft:Option<&Value>,pending:bool)->(Option<u8>,Option<&'static str>) {
    let requested=draft.and_then(Value::as_u64).filter(|value|*value<=3).map(|value|value as u8);
    match requested.filter(|_|pending) {
        Some(mode)=>(Some(mode),Some(["单曲","列表","顺序","随机"][mode as usize])),
        None=>(reported,None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_selection_is_pending_feedback_until_the_real_readback_arrives() {
        let reported=Some(1);
        let (key,draft)=command_draft(&Command::Loop(3)).unwrap();
        assert_eq!(key,"loop");
        assert_eq!(loop_feedback(reported,Some(&draft),true),(Some(3),Some("随机")));
        assert_eq!(reported,Some(1),"pending feedback must not manufacture confirmed state");
        // Completion removes the draft; a failed write keeps the last reported
        // mode, then the regular recovery read supplies the actual device mode.
        assert_eq!(loop_feedback(reported,None,false),(Some(1),None));
        assert_eq!(loop_feedback(Some(3),None,false),(Some(3),None));
        assert_eq!(loop_feedback(reported,Some(&draft),false),(Some(1),None));
    }

    #[test]
    fn invalid_or_missing_loop_drafts_cannot_show_a_requested_selection() {
        for draft in [json!(4),json!(-1),json!("3"),json!(true),Value::Null] {
            assert_eq!(loop_feedback(Some(1),Some(&draft),true),(Some(1),None));
        }
        assert_eq!(loop_feedback(Some(1),None,true),(Some(1),None));
        assert!(command_draft(&Command::Play("pause")).is_none());
    }
}
