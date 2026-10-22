use super::*;
use crate::overview::ActivityFilter;

impl HomePanelApp {
    pub(super) fn activity_candidates(&self)->Vec<Device> {
        self.devices.iter().filter(|d|self.device_in_selected_home(d) &&
            !self.preferences.hidden_devices.contains(&d.id)).cloned().collect()
    }

    pub(super) fn draw_activity(&mut self,ui:&mut egui::Ui) {
        let now=crate::now_ms();
        let candidates=self.activity_candidates();
        ui.horizontal(|ui| {
            ui.set_height(42.0);
            if design::icon_button(ui,Icon::Back,"返回我的设备",self.dark).clicked() {self.page=Page::Dashboard;}
            ui.label(RichText::new(self.activity_filter.title()).size(21.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center),|ui| {
                if design::icon_button(ui,Icon::Close,"收起到托盘",self.dark).clicked() {self.hide_panel(ui.ctx());}
            });
        });
        ui.add(egui::Label::new(RichText::new("当前家庭 · 按房间查看，点卡片进入控制").size(11.0).color(muted(self.dark))).wrap());
        ui.add_space(12.0);
        ui.columns(3,|columns| {
            for (column,filter) in columns.iter_mut().zip([ActivityFilter::On,ActivityFilter::Offline,ActivityFilter::Unknown]) {
                let count=candidates.iter().filter(|d|filter.includes(d,now)).count();
                let selected=self.activity_filter==filter;
                let name=match filter {ActivityFilter::On=>"已开启",ActivityFilter::Offline=>"离线",ActivityFilter::Unknown=>"待确认"};
                let button=egui::Button::new(RichText::new(format!("{name} {count}")).size(12.0)
                    .color(if selected {Color32::WHITE}else{muted(self.dark)}))
                    .fill(if selected {design::accent(self.dark)}else{design::surface(self.dark)}).corner_radius(16);
                if column.add_sized([column.available_width(),33.0],button).clicked() {self.activity_filter=filter;}
            }
        });
        ui.add_space(9.0);
        ui.label(RichText::new("缓存、读取失败或过期的开关状态列为“待确认”").size(10.0).color(muted(self.dark)));
        ui.add_space(10.0);
        let mut devices:Vec<_>=candidates.into_iter().filter(|d|self.activity_filter.includes(d,now)).collect();
        devices.sort_by(|a,b|a.room.cmp(&b.room).then_with(||a.name.cmp(&b.name)));
        let max_height=(ui.available_height()-33.0).max(100.0);
        egui::ScrollArea::vertical().id_salt(("activity-list",self.activity_filter as u8))
            .max_height(max_height).auto_shrink([false,false]).show(ui,|ui| {
                if devices.is_empty() {
                    ui.add_space(55.0);
                    ui.vertical_centered(|ui| {
                        brand_mark(ui,46.0);ui.add_space(14.0);
                        ui.label(RichText::new(match self.activity_filter {
                            ActivityFilter::On=>"暂无确认已开启的设备",ActivityFilter::Offline=>"暂无离线设备",ActivityFilter::Unknown=>"暂无待确认设备",
                        }).size(16.0).strong());
                        if self.activity_filter==ActivityFilter::On {
                            ui.label(RichText::new("状态正在依次刷新，也可查看“待确认”").size(11.0).color(muted(self.dark)));
                        }
                    });
                }
                let mut start=0;
                while start<devices.len() {
                    let room=devices[start].room.clone();
                    let end=start+devices[start..].iter().take_while(|d|d.room==room).count();
                    ui.label(RichText::new(format!("{}  {}",if room.is_empty(){"未分配房间"}else{&room},end-start)).size(12.0).strong().color(muted(self.dark)));
                    ui.add_space(5.0);
                    for row in devices[start..end].chunks(2) {
                        ui.columns(2,|columns| {
                            for (column,device) in columns.iter_mut().zip(row) {self.draw_device_card(column,device);}
                        });
                        ui.add_space(10.0);
                    }
                    start=end;
                }
            });
        ui.horizontal(|ui| {
            ui.label(RichText::new(if self.demo {"演示预览 · 模拟设备"}else if !self.reading.is_empty(){"正在更新设备状态…"}else{"设备回报状态 · 自动刷新"}).size(10.0).color(muted(self.dark)));
            ui.with_layout(Layout::right_to_left(Align::Center),|ui| {
                if design::icon_button(ui,Icon::Refresh,"重新查询状态",self.dark).clicked() {self.poll_schedule.forget();self.refresh_devices();}
            });
        });
    }
}

pub(super) fn overview_card(ui:&mut egui::Ui,counts:[usize;4],dark:bool)->Option<ActivityFilter> {
    let [online,running,rooms,offline]=counts;
        let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 88.0), Sense::click());
        let p = ui.painter();
        let fill = if dark { Color32::from_rgb(40,63,49) } else { Color32::from_rgb(224,240,233) };
        p.rect_filled(rect,20,fill);
        let accent = design::accent(dark);
        design::text(p,rect.min+egui::vec2(16.,13.),"家庭概览",10.5,muted(dark),150.);
        let title = if online+offline==0 { "连接你的家".into() } else {format!("{running} 台设备已开启")};
        design::text(p,rect.min+egui::vec2(16.,31.),&title,20.,design::ink(dark),255.);
        design::text(p,rect.min+egui::vec2(16.,62.),&format!("{online} 台在线   ·   {rooms} 个房间"),11.,muted(dark),240.);
        design::icon(p,rect.min+egui::vec2(262.,41.),Icon::Detail,15.,accent);
        let c=egui::pos2(rect.right()-45.,rect.top()+39.);
        p.circle_filled(c,27.,accent.gamma_multiply(0.08));
        let s=Stroke::new(1.6_f32,accent);
        p.add(egui::Shape::line(vec![c+egui::vec2(-17.,-1.),c+egui::vec2(0.,-15.),c+egui::vec2(17.,-1.)],s));
        p.rect_stroke(egui::Rect::from_center_size(c+egui::vec2(0.,6.),Vec2::new(24.,22.)),5,s,egui::StrokeKind::Inside);
        p.rect_filled(egui::Rect::from_center_size(c+egui::vec2(0.,10.),Vec2::new(6.,12.)),2,accent.gamma_multiply(0.3));
        let state = if offline > 0 {format!("{offline} 台离线 ›")} else if online+offline==0 {"等待连接".into()} else {"全部在线".into()};
        p.text(egui::pos2(c.x,rect.bottom()-13.),Align2::CENTER_CENTER,state,egui::FontId::proportional(9.5),accent);
        let offline_rect=egui::Rect::from_min_max(egui::pos2(rect.right()-93.,rect.top()+56.),rect.max);
        let offline_clicked=ui.interact(offline_rect,Id::new("overview-offline"),Sense::click()).on_hover_text("查看离线设备").clicked();
        let clicked=response.on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("查看当前家庭已开启的设备；缓存、失败或过期状态单独列为待确认").clicked();
    if offline_clicked || clicked {Some(if offline_clicked {ActivityFilter::Offline}else{ActivityFilter::On})}else{None}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overview_and_offline_badge_open_their_respective_lists() {
        for (offset,expected) in [(egui::vec2(100.0,40.0),ActivityFilter::On),(egui::vec2(370.0,75.0),ActivityFilter::Offline)] {
            let ctx=egui::Context::default();
            let mut origin=egui::Pos2::ZERO;
            let mut intent=None;
            for step in 0..4 {
                let pos=origin+offset;
                let events=match step {
                    2=>vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}],
                    3=>vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}],
                    _=>vec![],
                };
                let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(460.0,620.0))),events,..Default::default()},|ctx| {
                    egui::CentralPanel::default().show(ctx,|ui| {origin=ui.cursor().min;intent=overview_card(ui,[22,7,5,3],false);});
                });
            }
            assert_eq!(intent,Some(expected));
        }
    }
}
