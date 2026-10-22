//! Original, font-independent vector artwork and controls for the native panel.
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};

use crate::model::GlassStyle;

pub const GREEN: Color32 = Color32::from_rgb(35, 131, 102);
pub const WINDOW_RADIUS:f32=24.0;
pub const DETAIL_SIZE:Vec2=Vec2::new(388.0,540.0);
pub const DETAIL_RADIUS:f32=30.0;

pub fn main_frame(dark:bool,material:GlassStyle,detail:bool)->egui::Frame {
    // Keep the pixel alpha silhouette round even before the native region is ready.
    egui::Frame::new().fill(if detail {Color32::TRANSPARENT}else{glass_fill(dark,material)})
        .corner_radius(CornerRadius::same(WINDOW_RADIUS as u8)).inner_margin(20.0)
}

#[cfg(test)]
mod material_tests {
    use super::*;
    #[test]
    fn every_material_keeps_round_pixel_corners_and_detail_has_no_outer_fill() {
        for dark in [false,true] {
            for material in [GlassStyle::Solid,GlassStyle::Glass,GlassStyle::Frosted] {
                let frame=main_frame(dark,material,false);
                assert_eq!(frame.corner_radius,CornerRadius::same(WINDOW_RADIUS as u8));
                assert_eq!(frame.fill.a()==255,material==GlassStyle::Solid);
                assert_eq!(main_frame(dark,material,true).fill,Color32::TRANSPARENT);
            }
        }
    }
}

pub fn ink(dark: bool) -> Color32 {
    if dark { Color32::from_rgb(232, 239, 235) } else { Color32::from_rgb(34, 48, 43) }
}

pub fn secondary(dark: bool) -> Color32 {
    if dark { Color32::from_rgb(170, 185, 178) } else { Color32::from_rgb(96, 115, 105) }
}

pub fn surface(dark: bool) -> Color32 {
    if dark { Color32::from_rgb(35, 45, 41) } else { Color32::from_rgb(255, 255, 253) }
}

pub fn line(dark: bool) -> Color32 {
    if dark { Color32::from_rgb(56, 70, 62) } else { Color32::from_rgb(226, 233, 226) }
}

pub fn accent(dark: bool) -> Color32 {
    if dark { Color32::from_rgb(117, 212, 170) } else { GREEN }
}

#[derive(Clone, Copy)]
pub enum Icon { Close, Back, Settings, Search, More, Power, Refresh, Star, Hide, Detail, Lock }

pub fn icon(p: &Painter, c: Pos2, icon: Icon, size: f32, color: Color32) {
    let k = size / 24.0;
    let s = Stroke::new(1.7 * k, color);
    let point = |x: f32, y: f32| c + egui::vec2(x, y) * k;
    let segment = |a: [f32; 2], b: [f32; 2]| { p.line_segment([point(a[0], a[1]), point(b[0], b[1])], s); };
    match icon {
        Icon::Close => { segment([-5., -5.], [5., 5.]); segment([-5., 5.], [5., -5.]); }
        Icon::Back => { segment([2., -6.], [-4., 0.]); segment([-4., 0.], [2., 6.]); }
        Icon::More => { for x in [-5., 0., 5.] { p.circle_filled(point(x, 0.), 1.35 * k, color); } }
        Icon::Power => {
            let points = (0..=28).map(|i| { let a = (-55.0 + i as f32 * 290.0 / 28.0).to_radians(); point(a.cos() * 6.0, a.sin() * 6.0) }).collect();
            p.add(Shape::line(points, s));
            segment([0., -8.], [0., -1.]);
        }
        Icon::Search => { p.circle_stroke(point(-1.5, -1.5), 5.5 * k, s); segment([3., 3.], [7., 7.]); }
        Icon::Settings => {
            p.circle_stroke(c, 5.8 * k, s); p.circle_stroke(c, 2.0 * k, s);
            for i in 0..8 { let a = i as f32 * std::f32::consts::TAU / 8.0; segment([a.cos()*6., a.sin()*6.], [a.cos()*8., a.sin()*8.]); }
        }
        Icon::Refresh => {
            let points = (0..=26).map(|i| { let a = (-30.0 + i as f32 * 290.0 / 26.0).to_radians(); point(a.cos()*6.5, a.sin()*6.5) }).collect();
            p.add(Shape::line(points, s)); segment([6., -7.], [6., -2.]); segment([6., -2.], [1., -2.]);
        }
        Icon::Star => {
            let points = (0..10).map(|i| { let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0; let r = if i%2 == 0 { 7.5 } else { 3.6 }; point(a.cos()*r, a.sin()*r) }).collect();
            p.add(Shape::closed_line(points, s));
        }
        Icon::Hide => {
            let pts = vec![point(-8.,0.),point(-4.,-4.),point(0.,-5.5),point(4.,-4.),point(8.,0.),point(4.,4.),point(0.,5.5),point(-4.,4.)];
            p.add(Shape::closed_line(pts,s)); p.circle_stroke(c,2.4*k,s); segment([-8.,-8.],[8.,8.]);
        }
        Icon::Detail => {
            p.rect_stroke(Rect::from_center_size(c,Vec2::new(13.,16.)*k),3,s,egui::StrokeKind::Inside);
            segment([-3.,-3.],[3.,-3.]);segment([-3.,1.],[3.,1.]); segment([-3.,5.],[0.,5.]);
        }
        Icon::Lock => {
            p.rect_stroke(Rect::from_center_size(point(0.,3.),Vec2::new(12.,10.)*k),3,s,egui::StrokeKind::Inside);
            p.add(Shape::line(vec![point(-4.,-1.),point(-4.,-5.),point(-2.,-7.),point(2.,-7.),point(4.,-5.),point(4.,-1.)],s));
        }
    }
}

pub fn icon_button(ui: &mut Ui, kind: Icon, tip: &str, dark: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.), Sense::click());
    icon_button_at(ui, &response, rect, kind, dark);
    response.on_hover_text(tip)
}

pub fn icon_button_at(ui: &Ui, response: &Response, rect: Rect, kind: Icon, dark: bool) {
    let hover = ui.ctx().animate_bool(response.id.with("hover"), response.hovered());
    let bg = if dark { Color32::from_rgb(55,70,61) } else { Color32::from_rgb(224,234,225) };
    let press=ui.ctx().animate_bool(response.id.with("press"),response.is_pointer_button_down_on());
    ui.painter().rect_filled(rect.shrink(press*1.2), 10, bg.gamma_multiply((hover * 0.75+press*0.12).min(1.0)));
    icon(ui.painter(),rect.center(),kind,20.0-press,secondary(dark));
}

pub fn text(p: &Painter, pos: Pos2, value: &str, size: f32, color: Color32, width: f32) {
    let mut job = egui::text::LayoutJob::simple(value.to_owned(), FontId::proportional(size), color, width.max(1.));
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    p.galley(pos, p.layout_job(job), color);
}

pub fn menu_item(ui: &mut Ui, kind: Icon, label: &str, dark: bool, enabled: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(216., 32.), Sense::click());
    let color = if enabled { ink(dark) } else { secondary(dark).gamma_multiply(0.7) };
    if response.hovered() && enabled { ui.painter().rect_filled(rect,8,if dark { Color32::from_rgb(49,66,55) } else { Color32::from_rgb(236,244,236) }); }
    icon(ui.painter(),egui::pos2(rect.left()+17.,rect.center().y),kind,18.,if enabled { secondary(dark) } else { color });
    text(ui.painter(),rect.min+egui::vec2(36.,7.),label,12.,color,rect.width()-44.);
    response
}

pub fn device_art(p: &Painter, rect: Rect, kind: &str, dark: bool) {
    let pastel = match kind {
        "light" => Color32::from_rgb(249, 237, 202),
        "fan" => Color32::from_rgb(224, 237, 243),
        "sensor" => Color32::from_rgb(236, 231, 245),
        "speaker" => Color32::from_rgb(224, 231, 244),
        "mosquito" => Color32::from_rgb(237, 233, 213),
        _ => Color32::from_rgb(228, 238, 227),
    };
    p.rect_filled(rect,16,if dark { pastel.gamma_multiply(0.25) } else { pastel });
    let c=rect.center(); let k=rect.width()/56.;
    let point=|x:f32,y:f32| c+egui::vec2(x,y)*k;
    let white=if dark { Color32::from_rgb(204,218,211) } else { Color32::from_rgb(255,255,252) };
    let edge=if dark { Color32::from_rgb(145,164,153) } else { Color32::from_rgb(183,198,183) };
    let s=Stroke::new(1.3*k,edge);
    match kind {
        "light" => {
            p.add(Shape::ellipse_filled(point(0.,12.),egui::vec2(19.,4.)*k,Color32::from_black_alpha(9)));
            p.rect_filled(Rect::from_center_size(point(0.,1.),egui::vec2(36.,12.)*k),6,edge);
            p.add(Shape::ellipse_filled(point(0.,-4.),egui::vec2(18.,8.)*k,white));
            p.add(Shape::ellipse_filled(point(0.,5.),egui::vec2(15.,3.)*k,Color32::from_rgb(253,221,139)));
            p.line_segment([point(-9.,11.),point(-12.,15.)],Stroke::new(k,edge));
            p.line_segment([point(9.,11.),point(12.,15.)],Stroke::new(k,edge));
        }
        "fan" => {
            p.add(Shape::ellipse_filled(point(0.,20.),egui::vec2(13.,3.5)*k,edge));
            p.line_segment([point(0.,6.),point(0.,18.)],Stroke::new(4.*k,white));
            p.circle_filled(point(0.,-5.),16.*k,white); p.circle_stroke(point(0.,-5.),16.*k,s);
            p.circle_stroke(point(0.,-5.),12.*k,Stroke::new(0.8*k,edge));
            for i in 0..3 { let a=i as f32 * std::f32::consts::TAU/3.;
                let v=egui::vec2(a.cos(),a.sin()); let v2=egui::vec2((a+0.7).cos(),(a+0.7).sin());
                p.add(Shape::convex_polygon(vec![point(0.,-5.),point(v.x*11.,v.y*11.-5.),point(v2.x*9.,v2.y*9.-5.)],edge,Stroke::NONE));
            }
            p.circle_filled(point(0.,-5.),2.5*k,GREEN);
        }
        "sensor" => {
            p.rect_filled(Rect::from_center_size(point(1.,2.),egui::vec2(35.,36.)*k),10,Color32::from_black_alpha(12));
            p.rect_filled(Rect::from_center_size(point(0.,0.),egui::vec2(34.,36.)*k),10,white);
            let screen=Rect::from_center_size(point(0.,-3.),egui::vec2(23.,17.)*k);
            p.rect_filled(screen,4,Color32::from_rgb(220,229,222));
            p.text(screen.center(),Align2::CENTER_CENTER,"°C",FontId::proportional(13.*k),Color32::from_rgb(88,110,99));
            p.circle_filled(point(0.,12.),1.5*k,GREEN);
        }
        "plug" => {
            p.rect_filled(Rect::from_center_size(point(1.,2.),egui::vec2(33.,35.)*k),10,Color32::from_black_alpha(10));
            p.rect_filled(Rect::from_center_size(point(0.,0.),egui::vec2(32.,34.)*k),10,white);
            p.circle_stroke(point(0.,-2.),10.*k,s);
            for x in [-4.,4.] { p.line_segment([point(x,-6.),point(x,-1.)],Stroke::new(2.*k,edge)); }
            p.line_segment([point(-2.,4.),point(2.,4.)],Stroke::new(2.*k,edge));
            p.circle_filled(point(0.,13.),1.7*k,GREEN);
        }
        "switch" => {
            let body=Rect::from_center_size(c,egui::vec2(38.,34.)*k);
            p.rect_filled(body,7,white);
            p.rect_stroke(body,7,s,egui::StrokeKind::Inside);
            for x in [-12.,0.,12.] {
                p.rect_filled(Rect::from_center_size(point(x,0.),egui::vec2(9.,27.)*k),3,if dark { edge.gamma_multiply(0.45) } else { Color32::from_rgb(237,242,235) });
                p.line_segment([point(x-2.,8.),point(x+2.,8.)],Stroke::new(1.5*k,GREEN));
            }
        }
        "speaker" => {
            let body=Rect::from_center_size(point(0.,1.),egui::vec2(29.,34.)*k);
            p.rect_filled(body,8,white);
            p.add(Shape::ellipse_filled(point(0.,-13.),egui::vec2(14.5,5.)*k,edge));
            p.add(Shape::ellipse_filled(point(0.,-13.),egui::vec2(10.,2.4)*k,if dark { GREEN } else { Color32::from_rgb(170,212,219) }));
            for y in [-4.,1.,6.,11.] { for x in [-8.,-4.,0.,4.,8.] { p.circle_filled(point(x,y),0.7*k,edge); } }
        }
        "mosquito" => {
            p.rect_filled(Rect::from_center_size(point(0.,5.),egui::vec2(22.,26.)*k),6,white);
            p.rect_filled(Rect::from_center_size(point(0.,11.),egui::vec2(17.,8.)*k),3,Color32::from_rgb(192,213,185));
            p.rect_filled(Rect::from_center_size(point(0.,-10.),egui::vec2(31.,15.)*k),6,white);
            p.rect_stroke(Rect::from_center_size(point(0.,-10.),egui::vec2(31.,15.)*k),6,s,egui::StrokeKind::Inside);
            for x in [-7.,-2.,3.,8.] { p.line_segment([point(x,-13.),point(x,-8.)],Stroke::new(k,edge)); }
            p.circle_filled(point(11.,-3.),1.4*k,GREEN);
        }
        "climate" => {
            let body=Rect::from_center_size(point(0.,-4.),egui::vec2(40.,22.)*k);
            p.rect_filled(body,5,white);
            p.rect_stroke(body,5,s,egui::StrokeKind::Inside);
            p.line_segment([point(-16.,1.),point(16.,1.)],s);
            p.circle_filled(point(13.,-9.),1.2*k,GREEN);
            for x in [-9.,0.,9.] {
                p.add(Shape::line(vec![point(x,9.),point(x-2.,13.),point(x+1.,17.)],Stroke::new(1.3*k,edge)));
            }
        }
        _ => {
            p.rect_filled(Rect::from_center_size(c,egui::vec2(25.,36.)*k),7,white);
            for y in [-8.,-3.,2.] {p.line_segment([point(-6.,y),point(6.,y)],s);}
            p.circle_filled(point(0.,12.),2.*k,GREEN);
        }
    }
}

pub fn power_button(ui: &mut Ui, id: Id, rect: Rect, on: bool, enabled: bool, busy: bool, dark: bool) -> Response {
    let response = ui.interact(rect,id,if enabled { Sense::click() } else { Sense::hover() });
    let t=ui.ctx().animate_bool(id.with("state"),on);
    let off=if dark { Color32::from_rgb(53,67,57) } else { Color32::from_rgb(237,243,236) };
    let fill=off.lerp_to_gamma(GREEN,t);
    ui.painter().circle_filled(rect.center(),rect.width()/2.,fill);
    if response.hovered() && enabled {ui.painter().circle_stroke(rect.center(),rect.width()/2.+2.,Stroke::new(1.0_f32,accent(dark).gamma_multiply(0.4)));}
    if busy {
        // Paint in place: ui.put changes the parent cursor and removes its spacing.
        let animate=ui.style().animation_time>0.0;
        let angle=if animate { ui.input(|input| input.time) as f32 * 5.0 } else { -std::f32::consts::FRAC_PI_2 };
        let points=(0..=24).map(|index| { let a=angle+index as f32/24.0*4.7; rect.center()+egui::vec2(a.cos(),a.sin())*7.5 }).collect();
        ui.painter().add(Shape::line(points,Stroke::new(1.8_f32,if on {Color32::WHITE}else{accent(dark)})));
        if animate { ui.ctx().request_repaint_after(std::time::Duration::from_millis(16)); }
    }
    else {icon(ui.painter(),rect.center(),Icon::Power,21.,if on { Color32::WHITE } else { secondary(dark) });}
    response.on_hover_text(if !enabled && !busy {"设备离线或没有可用电源状态"} else if on {"关闭设备"} else {"打开设备"})
}

pub fn popup_frame(dark: bool) -> egui::Frame {
    egui::Frame::new().fill(surface(dark)).stroke(Stroke::new(1.0_f32,line(dark)))
        .corner_radius(CornerRadius::same(16)).inner_margin(8.)
        .shadow(egui::epaint::Shadow { offset:[0,8],blur:28,spread:0,color:Color32::from_black_alpha(25) })
}

pub fn settings_frame(dark: bool, material: GlassStyle) -> egui::Frame {
    let fill = if material == GlassStyle::Solid { surface(dark) } else {
        let [r,g,b,_] = surface(dark).to_array();
        Color32::from_rgba_unmultiplied(r,g,b,if material == GlassStyle::Glass { 158 } else { 204 })
    };
    egui::Frame::new().fill(fill).stroke(Stroke::new(1.0_f32,line(dark)))
        .corner_radius(16).inner_margin(14.)
}

pub fn toggle(ui: &mut Ui, value: &mut bool) -> Response {
    let (rect, mut response)=ui.allocate_exact_size(Vec2::new(38.,22.),Sense::click());
    if response.clicked() { *value=!*value; response.mark_changed(); }
    let t=ui.ctx().animate_bool(response.id,*value);
    let off=if ui.visuals().dark_mode {Color32::from_rgb(65,81,71)} else {Color32::from_rgb(218,228,221)};
    ui.painter().rect_filled(rect,20,off.lerp_to_gamma(GREEN,t));
    ui.painter().circle_filled(egui::pos2(rect.left()+11.+16.*t,rect.center().y),8.,Color32::WHITE);
    response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::Checkbox,ui.is_enabled(),*value,""));
    response
}

pub fn glass_fill(dark: bool, material: GlassStyle) -> Color32 {
    match (dark, material) {
        (false, GlassStyle::Glass) => Color32::from_rgba_unmultiplied(238, 247, 255, 154),
        (true, GlassStyle::Glass) => Color32::from_rgba_unmultiplied(20, 30, 43, 172),
        (false, GlassStyle::Frosted) => Color32::from_rgba_unmultiplied(245, 249, 253, 176),
        (true, GlassStyle::Frosted) => Color32::from_rgba_unmultiplied(27, 35, 43, 202),
        (false, GlassStyle::Solid) => Color32::from_rgb(245, 249, 253),
        (true, GlassStyle::Solid) => Color32::from_rgb(27, 35, 43),
    }
}

pub fn paint_glass(p: &Painter, rect: Rect, dark: bool, material: GlassStyle) {
    let shadow = egui::epaint::Shadow { offset: [0, 10], blur: 32, spread: 0, color: Color32::from_black_alpha(if dark { 54 } else { 24 }) };
    p.add(shadow.as_shape(rect, 30));
    p.rect_filled(rect, 30, glass_fill(dark, material));
    let clear = material == GlassStyle::Glass;
    let highlight = Color32::from_white_alpha(if dark { if clear { 100 } else { 45 } } else { 220 });
    p.rect_stroke(rect.shrink(0.5), 30, Stroke::new(if clear { 1.2_f32 } else { 1.0_f32 }, highlight), egui::StrokeKind::Inside);
    // Static bevels have no extra texture, blur pass, timer or background animation.
    p.line_segment([rect.min + egui::vec2(31.0, 1.0), egui::pos2(rect.right() - 31.0, rect.top() + 1.0)], Stroke::new(1.0_f32, Color32::from_white_alpha(if clear { 220 } else if dark { 22 } else { 160 })));
    if clear {
        p.rect_stroke(rect.shrink(2.0), 28, Stroke::new(0.6_f32, Color32::from_white_alpha(36)), egui::StrokeKind::Inside);
    }
}

pub fn control_surface(p: &Painter, rect: Rect, dark: bool, material: GlassStyle) {
    let alpha = match (dark, material) {
        (false, GlassStyle::Glass) => 64,
        (true, GlassStyle::Glass) => 18,
        (false, _) => 122,
        (true, _) => 12,
    };
    p.rect_filled(rect, 19, Color32::from_white_alpha(alpha));
    p.rect_stroke(rect, 19, Stroke::new(1.0_f32, Color32::from_white_alpha(if dark { 35 } else { 170 })), egui::StrokeKind::Inside);
}

pub fn round_icon(ui: &mut Ui, id: Id, rect: Rect, kind: Icon, tip: &str, dark: bool) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    let t = ui.ctx().animate_bool(id.with("hover"), response.hovered());
    let press=ui.ctx().animate_bool(id.with("press"),response.is_pointer_button_down_on());
    ui.painter().circle_filled(rect.center(), rect.width() / 2.0-press, if dark { Color32::from_white_alpha((16.0 + t * 15.0) as u8) } else { Color32::from_white_alpha((115.0 + t * 60.0) as u8) });
    icon(ui.painter(), rect.center(), kind, 20.0-press, secondary(dark));
    response.on_hover_text(tip)
}

pub fn segmented_tabs(ui:&mut Ui,id:Id,labels:&[&str],selected:&mut usize,dark:bool) {
    if labels.is_empty(){return;}
    *selected=(*selected).min(labels.len()-1);
    let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),32.0),Sense::hover());
    ui.painter().rect_filled(rect,16,Color32::from_white_alpha(if dark{12}else{48}));
    let width=rect.width()/labels.len() as f32;
    let position=ui.ctx().animate_value_with_time(id.with("selection"),*selected as f32,ui.style().animation_time);
    let pill=Rect::from_min_size(rect.min+egui::vec2(position*width+2.0,2.0),Vec2::new(width-4.0,28.0));
    ui.painter().rect_filled(pill,14,surface(dark));
    ui.painter().rect_stroke(pill,14,Stroke::new(0.6_f32,line(dark)),egui::StrokeKind::Inside);
    for (index,label) in labels.iter().enumerate() {
        let item=Rect::from_min_size(rect.min+egui::vec2(index as f32*width,0.0),Vec2::new(width,32.0));
        if ui.interact(item,id.with(index),Sense::click()).clicked(){*selected=index;}
        text(ui.painter(),item.min+egui::vec2((width-24.0)/2.0,8.0),label,12.0,if *selected==index{accent(dark)}else{secondary(dark)},width-8.0);
    }
}

#[cfg(test)]
mod segmented_tests {
    use super::*;
    #[test]
    fn selection_changes_without_changing_the_segment_row_height() {
        let ctx=egui::Context::default();let mut selected=0;let mut origin=egui::Pos2::ZERO;
        for step in 0..5 {
            let pos=origin+egui::vec2(260.0,15.0);
            let events=match step {
                2|3=>vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:step==2,modifiers:Default::default()}],
                _=>vec![],
            };
            let _=ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(360.0,120.0))),time:Some(step as f64*0.1),events,..Default::default()},|ctx| {
                egui::CentralPanel::default().show(ctx,|ui| {
                    origin=ui.cursor().min;let before=ui.cursor().min.y;
                    segmented_tabs(ui,Id::new("tabs-test"),&["控制","设置","信息"],&mut selected,false);
                    assert!((ui.cursor().min.y-before-32.0-ui.spacing().item_spacing.y).abs()<0.1);
                });
            });
        }
        assert_eq!(selected,2);
    }
}

pub fn glass_slider(ui: &mut Ui, id: Id, value: &mut f64, range: &[f64], integer: bool, dark: bool) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::hover());
    let mut response = ui.interact(rect, id, Sense::click_and_drag());
    let rail = Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 28.0, 10.0));
    let previous = *value;
    if ui.is_enabled() {
        let previous_center = rail.left() + rail.width() * ((*value-range[0])/(range[1]-range[0])).clamp(0.0,1.0) as f32;
        if response.drag_started() {
            let offset = ui.input(|input| input.pointer.press_origin())
                .filter(|origin| (origin.x-previous_center).abs()<=12.0)
                .map(|origin| origin.x-previous_center).unwrap_or(0.0);
            ui.ctx().data_mut(|data| data.insert_temp(id.with("drag-offset"), offset));
        }
        if response.clicked() || response.dragged() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let offset = if response.dragged() { ui.ctx().data(|data| data.get_temp::<f32>(id.with("drag-offset"))).unwrap_or(0.0) } else { 0.0 };
                if response.dragged() || (pointer.x-previous_center).abs()>12.0 {
                    let fraction = ((pointer.x - offset - rail.left()) / rail.width()).clamp(0.0, 1.0) as f64;
                    *value = range[0] + fraction * (range[1] - range[0]);
                }
            }
        }
        if response.has_focus() {
            let step = range.get(2).copied().filter(|step| *step > 0.0).unwrap_or((range[1] - range[0]) / 100.0);
            ui.input_mut(|input| {
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) { *value -= step; }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) { *value += step; }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Home) { *value = range[0]; }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::End) { *value = range[1]; }
            });
        }
        // Snap before painting, so the thumb never corrects itself on the next frame.
        *value = crate::presentation::snap_value(*value, range, integer);
        if *value != previous { response.mark_changed(); }
    }
    let t = ((*value - range[0]) / (range[1] - range[0])).clamp(0.0, 1.0) as f32;
    let center = egui::pos2(egui::lerp(rail.x_range(), t), rect.center().y);
    let p = ui.painter();
    p.rect_filled(rail, 20, if dark { Color32::from_white_alpha(23) } else { Color32::from_black_alpha(13) });
    p.rect_filled(Rect::from_min_max(rail.min, egui::pos2(center.x.max(rail.left() + 5.0), rail.bottom())), 20, accent(dark));
    let glow = ui.ctx().animate_bool(id.with("press"), response.is_pointer_button_down_on());
    if glow > 0.0 { p.circle_filled(center, 17.0, accent(dark).gamma_multiply(glow * 0.12)); }
    p.circle_filled(center + egui::vec2(0.0, 2.0), 12.0, Color32::from_black_alpha(15));
    p.circle_filled(center, 12.0, if dark { Color32::from_rgb(223, 233, 238) } else { Color32::WHITE });
    p.circle_stroke(center, 12.0, Stroke::new(1.0_f32, if dark { Color32::from_white_alpha(100) } else { Color32::from_black_alpha(20) }));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, ui.is_enabled(), format!("{value:.1}")));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_power_indicator_cannot_move_following_controls() {
        let ctx=egui::Context::default();
        let mut measured=Vec::new();
        for busy in [false,true,false] {
            let _=ctx.run(egui::RawInput { screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(460.0,620.0))), ..Default::default() },|ctx| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui| {
                    ui.spacing_mut().item_spacing.y=10.0;
                    let (card,_)=ui.allocate_exact_size(Vec2::new(348.0,56.0),Sense::hover());
                    let button=Rect::from_center_size(egui::pos2(card.right()-30.0,card.center().y),Vec2::splat(36.0));
                    power_button(ui,Id::new("test-power"),button,true,!busy,busy,false);
                    measured.push(ui.allocate_exact_size(Vec2::new(348.0,88.0),Sense::hover()).0);
                });
            });
        }
        assert_eq!(measured[0],measured[1],"pending indicator changed parent cursor");
        assert_eq!(measured[1],measured[2]);
    }

    fn slider_frame(ctx: &egui::Context, value: &mut f64, events: Vec<egui::Event>) -> (Rect, Pos2) {
        let mut rect=Rect::NOTHING;
        let output=ctx.run(egui::RawInput { screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(320.0,100.0))), events, ..Default::default() }, |ctx| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                rect=glass_slider(ui,Id::new("test-slider"),value,&[0.0,100.0,5.0],true,false).rect;
            });
        });
        let center=output.shapes.iter().find_map(|shape| match &shape.shape {
            Shape::Circle(circle) if circle.radius==12.0 && circle.fill==Color32::WHITE => Some(circle.center),
            _ => None,
        }).expect("painted slider thumb");
        (rect,center)
    }

    #[test]
    fn slider_thumb_offset_and_snapped_paint_stay_stable_through_release() {
        let ctx=egui::Context::default();
        let mut value=50.0;
        slider_frame(&ctx,&mut value,vec![]);
        let (rect,center)=slider_frame(&ctx,&mut value,vec![]);
        let origin=center+egui::vec2(8.0,0.0);
        slider_frame(&ctx,&mut value,vec![egui::Event::PointerMoved(origin),egui::Event::PointerButton { pos:origin,button:egui::PointerButton::Primary,pressed:true,modifiers:egui::Modifiers::NONE }]);
        assert_eq!(value,50.0,"pressing the thumb edge must not jump");
        let position=origin+egui::vec2(14.0,0.0);
        let (_,during)=slider_frame(&ctx,&mut value,vec![egui::Event::PointerMoved(position)]);
        assert_eq!(value,55.0,"drag follows movement with the original thumb offset");
        let expected=rect.left()+14.0+(rect.width()-28.0)*(value as f32/100.0);
        assert!((during.x-expected).abs()<0.01,"paint uses the snapped value in the same frame");
        let (_,released)=slider_frame(&ctx,&mut value,vec![egui::Event::PointerButton { pos:position,button:egui::PointerButton::Primary,pressed:false,modifiers:egui::Modifiers::NONE }]);
        let (_,next)=slider_frame(&ctx,&mut value,vec![]);
        assert_eq!(value,55.0);
        assert_eq!(during,released);
        assert_eq!(released,next);
    }
}
