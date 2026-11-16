//! Port of original 192x768 in-game HUD layout.
//! Source: docs/ui/side-panel.md, src/client/hud.ts and hud-pages.ts.
//! Logical panel coordinates are scaled as a unit with the game window.
//! Interaction here is deliberately restricted to implemented game commands.

use crate::{
    classic_hud::{ClassicHudArt,hud_scale,panel_point,hud_x},
    Game,
};
use game_core::rules::rule_id;
use macroquad::prelude::*;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum HudPage{Town,Build,Selection,Settings}
pub struct ClassicUi{
    pub art:ClassicHudArt,
    pub page:HudPage,
    pub show_units:bool,
    pub show_buildings:bool,
    pub show_resources:bool,
}
impl ClassicUi{
    pub fn new(data_dir:&std::path::Path,race:i32)->Self{
        Self{
            art:ClassicHudArt::load(data_dir,race),page:HudPage::Build,
            show_units:true,show_buildings:true,show_resources:false
        }
    }
}
fn pos(x:f32,y:f32)->Vec2{panel_point(x,y)}
fn rect(x:f32,y:f32,w:f32,h:f32,color:Color){
    let p=pos(x,y);
    let scale=hud_scale();
    draw_rectangle(p.x,p.y,w*scale,h*scale,color)
}
fn border(x:f32,y:f32,w:f32,h:f32,color:Color){
    let p=pos(x,y);let s=hud_scale();
    draw_rectangle_lines(p.x,p.y,w*s,h*s,1.2*s,color)
}
fn txt(g:&Game,label:&str,x:f32,y:f32,size:f32,color:Color){
    let p=pos(x,y);let s=hud_scale();
    g.draw_ui(label,p.x,p.y,(size*s).clamp(11.0,26.0),color)
}
fn line(label:&str,max:usize)->String{
    if label.chars().count()>max{
        label.chars().take(max.saturating_sub(3)).collect::<String>()+"..."
    }else{label.to_owned()}
}
fn button(
    g:&Game,title:&str,x:f32,y:f32,w:f32,h:f32,selected:bool,enabled:bool
){
    rect(x,y,w,h,if selected{
        Color::from_rgba(103,101,57,232)
    }else if !enabled{
        Color::from_rgba(35,41,34,220)
    }else{
        Color::from_rgba(57,66,48,224)
    });
    border(x,y,w,h,if selected{GOLD}else{Color::from_rgba(93,90,65,255)});
    txt(g,title,x+5.0,y+h*0.67,(h*0.47).clamp(10.0,14.0),
        if enabled{WHITE}else{GRAY});
}

fn buildable(g:&Game,ty:i32)->bool{
    // The UI must not advertise a second town hall while the simulation
    // correctly rejects founding another town in this single-town client.
    if ty==40&&g.simulation.objects.iter().any(|o|
        o.kind==1&&o.group==g.simulation.human_race&&o.object_type==40
    ){return false}
    let rule=g.simulation.rules.get(&rule_id(g.simulation.human_race,ty));
    // Original game allows placing a construction site without goods;
    // the request stays pending until its transporters can deliver them.
    rule.is_some_and(|r|r.kind==1&&r.group==g.simulation.human_race)
}
fn build_entries(g:&Game)->Vec<(i32,String,bool,String)>{
    let language_ru=g.lang==crate::localization::Language::Russian;
    // These are the only construction types implemented by the current
    // simulation; locked original production types are not fake buttons.
    [37,38,40,41].into_iter().filter_map(|ty|{
        let rule=g.simulation.rules.get(&rule_id(g.simulation.human_race,ty))?;
        let title=crate::localization::object_name(
            g.lang,&rule.name,1,ty,rule.class,
        ).into_owned();
        let cost=format!(
            "{}{}  {}{}  {}{}",
            if language_ru{"Д"}else{"W"},rule.cost[1].max(0),
            if language_ru{"К"}else{"S"},rule.cost[2].max(0),
            if language_ru{"Е"}else{"F"},rule.cost[4].max(0),
        );
        Some((ty,title,buildable(g,ty),cost))
    }).collect()
}

pub fn draw(g:&Game){
    g.ui.art.draw();
    // Original top minimap is always at (6,6) to (158,158).
    if g.minimap{
        let p=pos(6.0,6.0);
        let s=hud_scale();
        g.minimap(p.x,p.y,152.0*s,152.0*s);
    }
    for (i,(label,on)) in [
        ("B",g.ui.show_buildings),
        ("U",g.ui.show_units),
        ("R",g.ui.show_resources),
    ].iter().enumerate(){
        let y=[54.0,82.0,109.0][i];
        if !g.ui.art.draw_icon(["GEBAEUDE","EINHEITEN","HAENDLER"][i],
            166.0,y,20.0,20.0,0){
            button(g,label,166.0,y,20.0,20.0,*on,true);
        }
        if !on{
            rect(166.0,y,20.0,20.0,Color::from_rgba(15,19,15,175));
        }
    }
    if !g.ui.art.draw_icon("OPTIONEN",11.0,162.0,30.0,29.0,0){
        button(g,"M",11.0,162.0,30.0,29.0,
            g.ui.page==HudPage::Settings,true);
    }
    txt(g,if g.lang==crate::localization::Language::Russian{
        "МАСШТАБ"
    }else{"ZOOM"},55.0,172.0,11.0,LIGHTGRAY);
    // In the original, x=54..162 is the zoom slider with a RING handle.
    rect(56.0,184.0,102.0,2.0,Color::from_rgba(123,115,74,255));
    let ratio=(g.zoom-0.5)/2.25;
    let handle_x=55.0+ratio.clamp(0.0,1.0)*101.0;
    if !g.ui.art.draw_icon("RING",handle_x-10.0,172.0,21.0,21.0,0){
        let p=pos(handle_x,181.0);
        draw_circle(p.x,p.y,7.0*hud_scale(),GOLD);
    }

    let ru=g.lang==crate::localization::Language::Russian;
    txt(g,if ru{"ПОСЕЛЕНИЕ"}else{"SETTLEMENT"},19.0,213.0,13.0,GOLD);
    let hall=g.simulation.objects.iter()
        .find(|o|o.kind==1&&o.group==g.simulation.human_race&&o.object_type==40);
    if let Some(hall)=hall{
        txt(g,&format!("{}  ({},{})",
            if ru{"Город №1"}else{"Town #1"},hall.x,hall.y),
            20.0,237.0,12.0,WHITE);
    }else{
        txt(g,if ru{"Нет ратуши"}else{"No town hall"},20.0,237.0,12.0,LIGHTGRAY);
    }
    txt(g,&format!("{}: {}",
        if ru{"Золото"}else{"Gold"},g.simulation.gold),
        18.0,289.0,14.0,GOLD);
    // Every race has its own original treasury chest sprite.
    g.ui.art.draw_icon("SCHATZKISTE",130.0,274.0,39.0,28.0,0);

    let tabs=[
        (HudPage::Town,if ru{"Г"}else{"T"}),
        (HudPage::Build,if ru{"С"}else{"B"}),
        (HudPage::Selection,if ru{"О"}else{"I"}),
        (HudPage::Settings,if ru{"М"}else{"M"}),
    ];
    for (i,(page,label)) in tabs.iter().enumerate(){
        let x=12.0+i as f32*28.0;
        let sprite=["STADTSTATISTIK","GEBAEUDEBAUEN","INFO","OPTIONEN"][i];
        if !g.ui.art.draw_icon(sprite,x,300.0,30.0,30.0,0){
            button(g,label,x,300.0,30.0,30.0,g.ui.page==*page,true);
        }
        // The original bar does not tint its active page; its icons are
        // only tinted on hover. A small underline aids desktop navigation.
        if g.ui.page==*page{
            rect(x+2.0,330.0,26.0,2.0,GOLD);
        }
    }

    // PAGE_ORIGIN is (4,335) in the original HUD. The graphics archive
    // supplies this region's original background; text/buttons overlay it.
    rect(4.0,335.0,184.0,screen_height()/hud_scale()-335.0,
        Color::from_rgba(20,29,24,203));
    match g.ui.page{
        HudPage::Town=>{
            txt(g,if ru{"СВЕДЕНИЯ О ПОСЕЛЕНИИ"}else{"TOWN OVERVIEW"},
                10.0,361.0,13.0,GOLD);
            let labels=[
                (if ru{"Древесина"}else{"Wood"},g.simulation.goods[1]),
                (if ru{"Камень"}else{"Stone"},g.simulation.goods[2]),
                (if ru{"Пища"}else{"Food"},g.simulation.goods[4]),
                (if ru{"Железо"}else{"Iron"},g.simulation.goods[3]),
            ];
            for (i,(name,amount)) in labels.iter().enumerate(){
                let y=388.0+i as f32*31.0;
                rect(8.0,y-18.0,174.0,26.0,
                    Color::from_rgba(51,60,45,180));
                txt(g,name,13.0,y,13.0,WHITE);
                txt(g,&amount.to_string(),151.0,y,13.0,GOLD);
            }
            txt(g,&format!("{}: {}",
                if ru{"Жителей"}else{"Units"},
                g.simulation.selected_unit_ids().len()),
                13.0,540.0,13.0,WHITE);
            let capacity=game_core::economy::storage_capacity(
                g.simulation.objects.iter().filter(|o|o.kind==1)
                    .map(|o|(o.group,o.object_type,o.complete)),
                g.simulation.human_race
            );
            txt(g,&format!("{}: {} / {}",
                if ru{"Склад"}else{"Storage"},
                capacity,if ru{"вид"}else{"good"}),
                13.0,566.0,12.0,GOLD);
        }
        HudPage::Build=>{
            txt(g,if ru{"СТРОИТЕЛЬСТВО"}else{"BUILDINGS"},
                10.0,359.0,15.0,GOLD);
            for (i,(ty,title,enabled,cost)) in build_entries(g).iter().enumerate(){
                let y=372.0+i as f32*64.0;
                button(g,&line(title,22),8.0,y,176.0,31.0,
                    g.selected.build==Some(*ty),*enabled);
                txt(g,&format!("ID {ty}  |  {cost}"),
                    12.0,y+49.0,11.0,LIGHTGRAY);
            }
            txt(g,if ru{"ПКМ — отмена постройки"}else{"RMB cancels placing"},
                10.0,656.0,11.0,LIGHTGRAY);
        }
        HudPage::Selection=>{
            txt(g,if ru{"ВЫБРАННЫЙ ОБЪЕКТ"}else{"SELECTED OBJECT"},
                10.0,360.0,15.0,GOLD);
            if let Some(id)=g.selected.object{
                if let Some(obj)=g.simulation.entity(id){
                    let rule=g.simulation.rule(obj);
                    let name=crate::localization::object_name(
                        g.lang,
                        rule.map_or("Unknown",|r|r.name.as_str()),
                        obj.kind,obj.object_type,rule.map_or(-1,|r|r.class)
                    );
                    txt(g,&line(&name,24),10.0,390.0,13.0,WHITE);
                    txt(g,&format!("ID: {}   HP: {}",obj.object_type,obj.hp),
                        10.0,420.0,12.0,WHITE);
                    txt(g,&format!("X: {}    Y: {}",obj.x,obj.y),
                        10.0,447.0,12.0,LIGHTGRAY);
                    match obj.kind{
                        0=>{
                            txt(g,crate::localization::task_name(g.lang,&obj.task),
                                10.0,475.0,12.0,GOLD);
                            if obj.carrying>0{
                                txt(g,&format!("{}: {}",
                                    if ru{"Груз"}else{"Cargo"},obj.carrying),
                                    10.0,505.0,12.0,WHITE);
                            }
                        }
                        1=>{
                            let state=if !obj.complete{
                                if ru{"Строительство"}else{"Under construction"}
                            }else if ru{"Построено"}else{"Completed"};
                            txt(g,state,10.0,475.0,13.0,
                                if obj.complete{GREEN}else{ORANGE});
                            if let Some(r)=rule{
                                txt(g,&format!("{}: {} x {}",
                                    if ru{"Размер"}else{"Footprint"},
                                    r.size_x,r.size_y),
                                    10.0,502.0,12.0,LIGHTGRAY);
                                txt(g,&format!("{}: {}  {}: {}",
                                    if ru{"Древ."}else{"Wood"},r.cost[1],
                                    if ru{"Кам."}else{"Stone"},r.cost[2]),
                                    10.0,529.0,12.0,LIGHTGRAY);
                            }
                            if !obj.complete{
                                txt(g,&format!("{}: {}%",
                                    if ru{"Готовность"}else{"Progress"},
                                    obj.construction),
                                    10.0,562.0,12.0,ORANGE);
                                if let Some(site)=g.simulation.sites.get(&id){
                                    txt(g,if ru{"Материалы на площадке"}
                                        else{"Site materials delivered / needed"},
                                        10.0,585.0,12.0,GOLD);
                                    txt(g,&format!(
                                        "{}: {}/{}  {}: {}/{}",
                                        if ru{"Д"}else{"W"},
                                        site.stock[1],site.remaining[1],
                                        if ru{"К"}else{"S"},
                                        site.stock[2],site.remaining[2],
                                    ),10.0,610.0,12.0,WHITE);
                                    let missing=site.remaining.iter().zip(site.stock.iter())
                                        .any(|(&n,&stock)|n>stock);
                                    if missing{
                                        txt(g,if ru{"Ожидание доставки"}
                                            else{"Awaiting material delivery"},
                                            10.0,632.0,12.0,ORANGE);
                                    }
                                }
                                button(g,if ru{"ПРОДОЛЖИТЬ СТРОЙКУ"}
                                    else{"RESUME CONSTRUCTION"},
                                    8.0,652.0,176.0,35.0,false,true);
                            }else if obj.object_type==40||obj.object_type==41{
                                txt(g,if ru{"Хранение: 25 / вид"}
                                    else{"Storage: 25 per good"},
                                    10.0,564.0,12.0,WHITE);
                            }
                        }
                        2=>{
                            let class=rule.map_or(-1,|r|r.class);
                            let category=match class{
                                8=>if ru{"Дерево"}else{"Tree"},
                                9|10..=12=>if ru{"Растение"}else{"Plant"},
                                13=>if ru{"Камень"}else{"Stone"},
                                7=>if ru{"Животное"}else{"Animal"},
                                _=>if ru{"Природный объект"}else{"Nature"},
                            };
                            txt(g,category,10.0,475.0,13.0,GOLD);
                        }
                        _=>{}
                    }
                }
            }else{
                txt(g,if ru{"Выберите объект на карте"}else{"Select an object"},
                    10.0,395.0,12.0,WHITE);
            }
        }
        HudPage::Settings=>{
            txt(g,if ru{"ПАРАМЕТРЫ ИГРЫ"}else{"OPTIONS"},
                10.0,359.0,15.0,GOLD);
            button(g,if g.simulation.paused{
                if ru{"ВОЗОБНОВИТЬ"}else{"RESUME"}
            }else if ru{"ПАУЗА"}else{"PAUSE"},
                8.0,383.0,176.0,35.0,false,true);
            button(g,if ru{"СОХРАНИТЬ (F5)"}else{"SAVE (F5)"},
                8.0,435.0,176.0,35.0,false,true);
            button(g,if ru{"ЗАГРУЗИТЬ (F9)"}else{"LOAD (F9)"},
                8.0,487.0,176.0,35.0,false,true);
            button(g,if ru{"ЯЗЫК: РУССКИЙ"}else{"LANGUAGE: ENGLISH"},
                8.0,539.0,176.0,35.0,false,true);
        }
    }
    // Original timed messages appear at world-view top-left rather than in
    // a permanently reserved top status strip.
    let screen=screen_width();
    if let Some((message,until))=&g.toast{
        if get_time()<*until{
            let font_size=16.0*hud_scale();
            let width=measure_text(message,g.ui_font.as_ref(),
                font_size.clamp(12.0,25.0) as u16,1.0).width;
            let max=screen-hud_x();
            draw_rectangle(12.0,9.0,width.min(screen-max-32.0)+20.0,
                font_size+17.0,Color::from_rgba(21,30,24,230));
            g.draw_ui(message,21.0,12.0+font_size,font_size,GOLD);
        }
    }
    txt(g,if ru{"F10: язык  |  F5/F9: сохранить/загрузить"}else{
        "F10 language  |  F5/F9 save/load"
    },8.0,735.0,10.0,LIGHTGRAY);
}
fn in_rect(p:Vec2,x:f32,y:f32,w:f32,h:f32)->bool{
    p.x>=x&&p.x<x+w&&p.y>=y&&p.y<y+h
}
pub fn click(g:&mut Game,mouse:Vec2,left:bool)->bool{
    if mouse.x<hud_x(){return false}
    if !left {g.selected.build=None;return true}
    let s=hud_scale();
    let local=vec2((mouse.x-hud_x())/s,mouse.y/s);
    // First, panel fixed elements in source logical coords.
    if in_rect(local,166.0,54.0,20.0,20.0){
        g.ui.show_buildings=!g.ui.show_buildings;return true
    }
    if in_rect(local,166.0,82.0,20.0,20.0){
        g.ui.show_units=!g.ui.show_units;return true
    }
    if in_rect(local,166.0,109.0,20.0,20.0){
        g.ui.show_resources=!g.ui.show_resources;return true
    }
    if in_rect(local,11.0,162.0,30.0,30.0){
        g.ui.page=HudPage::Settings;return true
    }
    if in_rect(local,54.0,166.0,108.0,31.0){
        let ratio=((local.x-55.0)/102.0).clamp(0.0,1.0);
        g.zoom=0.5+ratio*2.25;return true
    }
    for (i,page) in [HudPage::Town,HudPage::Build,HudPage::Selection,HudPage::Settings]
        .iter().enumerate(){
        if in_rect(local,12.0+i as f32*28.0,300.0,30.0,30.0){
            g.ui.page=*page;return true
        }
    }
    if g.ui.page==HudPage::Build{
        for (i,(ty,_,enabled,_)) in build_entries(g).iter().enumerate(){
            if in_rect(local,8.0,372.0+i as f32*64.0,176.0,31.0){
                if *enabled{
                    g.selected.build=Some(*ty);
                }else{
                    g.notify(if g.lang==crate::localization::Language::Russian{
                        "Постройка недоступна из-за ограничений"
                    }else{"Building unavailable: placement restricted"});
                }
                return true
            }
        }
    }
    if g.ui.page==HudPage::Selection &&
        in_rect(local,8.0,652.0,176.0,35.0){
        if let Some(site)=g.selected.object{
            let can_resume=g.simulation.entity(site)
                .is_some_and(|o|o.kind==1&&!o.complete);
            if can_resume{
                let candidates:Vec<u32>=g.simulation.objects.iter()
                    .filter(|o|o.kind==0&&o.group==g.simulation.human_race
                        &&o.object_type==11&&matches!(o.task,game_core::sim::Task::Idle))
                    .map(|o|o.id).collect();
                let resumed=candidates.into_iter().any(|worker|
                    g.simulation.order_construct(worker,site).is_ok()
                );
                g.notify(if resumed{
                    if g.lang==crate::localization::Language::Russian{
                        "Строитель направлен на стройку"
                    }else{"Builder assigned to site"}
                }else if g.lang==crate::localization::Language::Russian{
                    "Нет свободного строителя с маршрутом"
                }else{"No idle builder has a route"});
            }
        }
        return true
    }
    if g.ui.page==HudPage::Settings{
        for (i,y) in [383.0,435.0,487.0,539.0].iter().enumerate(){
            if in_rect(local,8.0,*y,176.0,35.0){
                match i{
                    0=>g.simulation.paused=!g.simulation.paused,
                    1=>g.quick_save(),
                    2=>g.quick_load(),
                    3=>g.toggle_language(),
                    _=>{}
                }
                return true
            }
        }
    }
    true
}
