use super::*;
use crate::speaker_service::{Command, Snapshot};
use crate::time_range::TimeRange;

fn player(ui:&mut egui::Ui, device:&Device, dark:bool, material:GlassStyle, busy:bool, status:Option<&str>)->Option<Action> {
    let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),124.0),Sense::hover());
    design::control_surface(ui.painter(),rect,dark,material);
    design::text(ui.painter(),rect.min+egui::vec2(16.0,12.0),"播放状态",11.0,muted(dark),rect.width()-32.0);
    design::text(ui.painter(),rect.min+egui::vec2(16.0,35.0),status.unwrap_or(&presentation::speaker_status(device)),21.0,design::ink(dark),rect.width()-32.0);
    if let Some(error)=device.properties.iter().find(|p|p.key=="playing-state").and_then(|p|p.error.as_deref()) {
        ui.interact(egui::Rect::from_min_size(rect.min+egui::vec2(12.,10.),Vec2::new(rect.width()-24.,58.)),Id::new(("speaker-read-error",&device.id)),Sense::hover()).on_hover_text(error);
    }
    let row=egui::Rect::from_min_size(rect.min+egui::vec2(12.0,77.0),Vec2::new(rect.width()-24.0,35.0));
    let mut controls=clipped_child(ui,egui::UiBuilder::new().id_salt(("speaker-transport",&device.id)).max_rect(row),row);
    let actions:Vec<_>=["previous","play","pause","stop","next"].into_iter()
        .filter_map(|key|device.actions.iter().find(|a|a.key==key)).collect();
    let mut clicked=None;
    if actions.is_empty() { controls.label(RichText::new("此型号未提供播放控制").size(11.0).color(muted(dark))); }
    else { controls.add_enabled_ui(device.online&&!busy,|ui| {
        ui.columns(actions.len(),|columns| {
            for (column,action) in columns.iter_mut().zip(actions) {
                let button=egui::Button::new(RichText::new(match action.key.as_str(){"previous"=>"上一首","play"=>"播放","pause"=>"暂停","stop"=>"停止","next"=>"下一首",_=>&action.name}).size(11.0))
                    .fill(design::surface(dark)).corner_radius(11).truncate();
                if column.add_sized([column.available_width(),32.0],button).clicked() {clicked=Some(action.clone());}
            }
        });
    }); }
    clicked
}

fn phone_player(ui:&mut egui::Ui,device:&Device,state:&Snapshot,dark:bool,material:GlassStyle,busy:bool)->Option<Command> {
    let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),124.0),Sense::hover());
    design::control_surface(ui.painter(),rect,dark,material);
    design::text(ui.painter(),rect.min+egui::vec2(16.,10.),if busy {"正在确认音箱状态…"}else{"当前播放"},11.,muted(dark),rect.width()-32.);
    let status=if state.at==0 {"正在读取音箱状态…".into()}else{state.state_text()};
    design::text(ui.painter(),rect.min+egui::vec2(16.,30.),&status,19.,design::ink(dark),rect.width()-32.);
    let title=if state.title.is_empty(){"当前音源未提供歌曲信息"}else{&state.title};
    design::text(ui.painter(),rect.min+egui::vec2(16.,57.),title,10.5,muted(dark),rect.width()-32.);
    let row=egui::Rect::from_min_size(rect.min+egui::vec2(12.,82.),Vec2::new(rect.width()-24.,32.));
    let mut clicked=None;
    let stop_rect=egui::Rect::from_min_size(rect.right_top()+egui::vec2(-61.,7.),Vec2::new(48.,22.));
    let mut stop=clipped_child(ui,egui::UiBuilder::new().id_salt(("speaker-phone-stop",&device.id)).max_rect(stop_rect),stop_rect);
    if stop.add_enabled(!busy&&Command::Play("stop").validate(device,state).is_ok(),egui::Button::new(RichText::new("停止").size(10.)).small()).clicked(){clicked=Some(Command::Play("stop"));}
    let mut controls=clipped_child(ui,egui::UiBuilder::new().id_salt(("speaker-phone-transport",&device.id)).max_rect(row),row);
    let (main_label,main_key)=if state.status==Some(1){("暂停","pause")}else{(state.play_label(),"play")};
    controls.add_enabled_ui(!busy,|ui|ui.columns(3,|columns| {
        for (column,(label,key)) in columns.iter_mut().zip([("上一首","prev"),(main_label,main_key),("下一首","next")]) {
            let command=Command::Play(key);
            let permission=command.validate(device,state);
            let reason=if busy {"正在回读确认，请稍候"}else{permission.as_ref().err().map(String::as_str).unwrap_or("通过小爱服务控制，修改后回读确认")};
            let button=egui::Button::new(RichText::new(label).size(11.5)).fill(design::surface(dark)).corner_radius(15).truncate().min_size(Vec2::new(column.available_width(),30.));
            if column.add_enabled(permission.is_ok(),button).on_disabled_hover_text(reason).clicked(){clicked=Some(command);}
        }
    }));
    clicked
}

fn clock(ui:&mut egui::Ui,key:&str,index:usize,label:&str,time:&mut [u8;3]) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0));
        for (component,limit) in [24,60,60].into_iter().enumerate() {
            if component>0 {ui.label(":");}
            egui::ComboBox::from_id_salt((key,index,component)).selected_text(format!("{:02}",time[component]))
                .width(43.0).height(200.0).show_ui(ui,|ui| {
                    let frame=ui.ctx().cumulative_frame_nr();
                    let memory=Id::new((key,index,component,"time-open-frame"));
                    let opening=ui.ctx().data(|data|data.get_temp::<u64>(memory)).is_none_or(|last|last.saturating_add(1)<frame);
                    ui.ctx().data_mut(|data|data.insert_temp(memory,frame));
                    for value in 0..limit {
                        let selected=time[component]==value;
                        let response=ui.selectable_value(&mut time[component],value,format!("{value:02}"));
                        if opening&&selected {response.scroll_to_me(Some(Align::Center));}
                    }
                });
        }
    });
}

impl HomePanelApp {
    pub(super) fn draw_speaker_controls(&mut self,ui:&mut egui::Ui,device:&Device) {
        let connected=self.uses_speaker_service(device);
        if connected {
            // A connected speaker has one playback channel. Its displayed state,
            // transport buttons and volume must all use the same service.
            let state=self.mina_states.get(&device.id).cloned().unwrap_or_default();
            let busy=self.device_busy(&device.id)||self.mina_pending.contains(&device.id);
            if let Some(command)=phone_player(ui,device,&state,self.dark,self.material(),busy){self.run_speaker_command(device,command);}
            self.draw_phone_volume(ui,device,&state);
        }else if let Some(action)=player(ui,device,self.dark,self.material(),self.device_busy(&device.id),None) {self.run_action(&device.id,&action);}
        if !crate::speaker_service::eligible(device) {
            for property in device.properties.iter().filter(|p|presentation::speaker_daily(p)&&p.key!="playing-state"){self.draw_property(ui,device,property);}
            return;
        }
        // Keep one reserved connection row, so polling success/failure cannot move sliders.
        let (hint_rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),70.),Sense::hover());
        let mut hint=ui.new_child(egui::UiBuilder::new().id_salt(("speaker-service-hint",&device.id)).max_rect(hint_rect));
        hint.set_clip_rect(ui.clip_rect().intersect(hint_rect));
        let missing=device.properties.iter().any(|p|p.key=="playing-state"&&(p.error.is_some()||p.value.is_none()));
        let message=if connected {"播放与音量已使用小爱服务；更多播放功能和连接状态可在小爱页查看。"}
            else if missing {"米家未返回播放状态。连接小爱服务后，可使用歌曲、音量和播放控制。"}
            else {"当前使用米家公开控制。连接小爱服务，可使用歌曲、进度及循环等功能。"};
        hint.add(egui::Label::new(RichText::new(message).size(10.5).color(muted(self.dark))).wrap());
        if hint.button(if connected {"更多音箱功能"}else{"连接小爱音箱服务"}).clicked(){self.detail_tab=super::DetailTab::Speaker;}
        for property in device.properties.iter().filter(|p|presentation::speaker_daily(p)&&p.key!="playing-state"&&!(connected&&p.key=="volume")) {
            self.draw_property(ui,device,property);
        }
    }

    fn draw_phone_volume(&mut self,ui:&mut egui::Ui,device:&Device,state:&Snapshot) {
        let draft_key=format!("mina:{}:volume",device.id);
        let mut value=self.control_drafts.get(&draft_key).and_then(Value::as_f64).or(state.volume.map(|v|v as f64)).unwrap_or(0.);
        let busy=self.device_busy(&device.id)||self.mina_pending.contains(&device.id);
        let permission=Command::Volume(value.round() as u8).validate(device,state);
        let enabled=!busy&&state.volume.is_some()&&permission.is_ok();
        let reason=if busy {"正在回读确认，请稍候"}else{permission.as_ref().err().map(String::as_str).unwrap_or("松手提交，修改后回读确认")};
        let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),88.),Sense::hover());
        design::control_surface(ui.painter(),rect,self.dark,self.material());
        design::text(ui.painter(),rect.min+egui::vec2(16.,12.),"音量",12.,design::ink(self.dark),rect.width()-120.);
        let label=if state.volume.is_some(){format!("{}{}%",if state.current(){""}else{"上次 "},value.round())}else if self.mina_reading.contains(&device.id){"正在读取…".into()}else{"状态未获取".into()};
        ui.painter().text(rect.right_top()+egui::vec2(-16.,18.),Align2::RIGHT_CENTER,label,egui::FontId::proportional(12.),muted(self.dark));
        let slider_rect=egui::Rect::from_min_max(rect.min+egui::vec2(14.,39.),rect.max-egui::vec2(14.,9.));
        let mut controls=clipped_child(ui,egui::UiBuilder::new().id_salt(("speaker-phone-volume",&device.id)).max_rect(slider_rect),slider_rect);
        let response=controls.add_enabled_ui(enabled,|ui|design::glass_slider(ui,Id::new(&draft_key),&mut value,&[0.,100.,1.],true,self.dark)).inner.on_disabled_hover_text(reason);
        if response.changed(){self.control_drafts.insert(draft_key,json!(value.round() as u8));}
        if enabled&&(response.drag_stopped()||(response.changed()&&!ui.input(|i|i.pointer.any_down()))){self.run_speaker_command(device,Command::Volume(value.round() as u8));}
    }

    pub(super) fn draw_time_property(&mut self,ui:&mut egui::Ui,device:&Device,property:&Property) {
        let key=property_control_key(device,property);
        let shown=displayed_property(property,&self.control_drafts,&key);
        let parsed=shown.value.as_ref().and_then(Value::as_str).and_then(TimeRange::parse);
        let height=if parsed.is_some() {157.0} else {75.0};
        let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),height),Sense::hover());
        design::control_surface(ui.painter(),rect,self.dark,self.material());
        design::text(ui.painter(),rect.min+egui::vec2(16.0,12.0),"启用时段",12.0,design::ink(self.dark),rect.width()-32.0);
        let Some(mut period)=parsed else {
            design::text(ui.painter(),rect.min+egui::vec2(16.0,40.0),"时段暂不可读，请刷新后设置",11.0,muted(self.dark),rect.width()-32.0);
            return;
        };
        let enabled=device.online&&!self.device_busy(&device.id)&&property.error.is_none();
        let field_rect=egui::Rect::from_min_size(rect.min+egui::vec2(14.0,36.0),Vec2::new(rect.width()-28.0,72.0));
        let mut fields=clipped_child(ui,egui::UiBuilder::new().id_salt((&key,"times")).max_rect(field_rect),field_rect);
        fields.add_enabled_ui(enabled,|ui| {
            clock(ui,&key,0,"开始",&mut period.start);
            clock(ui,&key,1,"结束",&mut period.end);
        });
        let original=property.value.as_ref().and_then(Value::as_str).and_then(TimeRange::parse);
        let changed=original.as_ref()!=Some(&period);
        if changed {self.control_drafts.insert(key.clone(),json!(period.wire()));}
        design::text(ui.painter(),rect.min+egui::vec2(16.0,125.0),if self.device_busy(&device.id) {"正在确认…"} else {"修改后点保存"},10.0,muted(self.dark),rect.width()-118.0);
        let button_rect=egui::Rect::from_min_size(egui::pos2(rect.right()-92.0,rect.bottom()-37.0),Vec2::new(76.0,27.0));
        let mut button=clipped_child(ui,egui::UiBuilder::new().id_salt((&key,"save-time")).max_rect(button_rect),button_rect);
        if button.add_enabled(enabled&&changed,egui::Button::new("保存时段").min_size(button_rect.size()).corner_radius(10)).clicked() {
            self.write_property(&device.id,property,json!(period.wire()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn phone_click(state:&Snapshot,busy:bool)->Option<Command> {
        let ctx=egui::Context::default();
        let device=Device {kind:"speaker".into(),online:true,spec_status:"ready".into(),actions:vec![Action{key:"pause".into(),..Default::default()}],..Default::default()};
        let mut at=egui::Pos2::ZERO;
        let mut result=None;
        for phase in 0..3 {
            let events=match phase {
                1=>vec![egui::Event::PointerMoved(at),egui::Event::PointerButton{pos:at,button:egui::PointerButton::Primary,pressed:true,modifiers:egui::Modifiers::NONE}],
                2=>vec![egui::Event::PointerButton{pos:at,button:egui::PointerButton::Primary,pressed:false,modifiers:egui::Modifiers::NONE}],
                _=>Vec::new(),
            };
            let _=ctx.run(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(460.,620.))),events,..Default::default()},|ctx| {
                egui::CentralPanel::default().show(ctx,|ui| {
                    at=egui::pos2(ui.max_rect().center().x,ui.cursor().min.y+97.);
                    result=phone_player(ui,&device,state,false,GlassStyle::Frosted,busy);
                });
            });
        }
        result
    }

    #[test]
    fn connected_player_clicks_phone_command_and_blocks_stale_or_pending_state() {
        let state=Snapshot{at:crate::now_ms(),player_at:crate::now_ms(),status:Some(1),..Default::default()};
        assert!(matches!(phone_click(&state,false),Some(Command::Play("pause"))));
        assert!(phone_click(&state,true).is_none());
        assert!(phone_click(&Snapshot::default(),false).is_none());
    }

    #[test]
    fn player_status_and_pending_controls_keep_one_fixed_card() {
        let ctx=egui::Context::default();let mut measured=Vec::new();
        for busy in [false,true,false] {
            let _=ctx.run(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(460.0,620.0))),..Default::default()},|ctx| {
                egui::CentralPanel::default().show(ctx,|ui| {
                    let device=Device {kind:"speaker".into(),online:true,actions:vec![Action{key:"play".into(),..Default::default()},Action{key:"pause".into(),..Default::default()}],..Default::default()};
                    assert!(player(ui,&device,false,GlassStyle::Frosted,busy,None).is_none());
                    measured.push(ui.allocate_exact_size(Vec2::new(300.0,88.0),Sense::hover()).0);
                });
            });
        }
        assert_eq!(measured[0],measured[1]);assert_eq!(measured[1],measured[2]);
    }
}
