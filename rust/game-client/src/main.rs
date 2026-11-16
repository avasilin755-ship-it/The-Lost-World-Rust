//! The Lost World — integrated Rust game client (Windows/Linux prototype).
//! Real original mission & map inputs + terrain generation + GD/OD rendering.
//! Fixed-step gameplay: selection, A*, gathering, transport to storage,
//! construction from ODB costs, local snapshots and minimap.
//!
//! This playable scaffold is not yet full faithful original simulation.

mod resources;
mod localization;
mod classic_hud;
mod classic_ui;

use game_core::{
    mission::{GameMap,Mission,parse_dvm,parse_mdb},
    rules::{parse_odb,merge_rules,rule_id},
    sim::{Simulation,Entity,Task,animal_cycle_phase,unit_idle_phase},
    terrain::{Terrain,generate_base},
};
use macroquad::prelude::*;
use resources::{WorldArt,TerrainArt,SpriteMotion,project,unproject,world_to_screen};
use localization::{Language,Key,tr};
use classic_hud::{hud_width,hud_x,hud_scale,panel_point};
use classic_ui::{ClassicUi,HudPage};
use std::{collections::HashMap,env,fs,path::{Path,PathBuf}};

// In the original 1024x768 game, the world occupies all the height.
// Only the 192-pixel race panel reserves screen space on the right.
const TOP:f32=0.0;
const FOOT:f32=0.0;
const PAN_SPEED:f32=500.0;
const TICK_SECONDS:f32=0.10;
const MAX_TICKS_PER_FRAME:usize=5;
const MOVE_INTERPOLATION_TICKS:f32=6.0;
const UI_FONT_LOCATIONS:&[&str]=&[
    "./assets/fonts/NotoSans-Regular.ttf",
    "C:/Windows/Fonts/segoeui.ttf",
    "C:/Windows/Fonts/arial.ttf",
    "C:/Windows/Fonts/tahoma.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
];
/// Game runs on Windows, so installed OS fonts are used without bundling
/// proprietary Microsoft TTF files with the source distribution.
fn load_ui_font()->Option<Font>{
    let configured=env::var("TLW_FONT").ok();
    let paths=configured.as_deref().into_iter().chain(UI_FONT_LOCATIONS.iter().copied());
    let chars:Vec<char>="АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ        абвгдеёжзийклмнопрстуфхцчшщъыьэюя0123456789        :-—,.!?()/%+= F10FPS".chars().filter(|c|!c.is_whitespace()).collect();
    for path in paths{
        if let Ok(data)=fs::read(path){
            if let Ok(mut font)=load_ttf_font_from_bytes(&data){
                for size in [14,16,18,20,22,26,29]{
                    font.populate_font_cache(&chars,size);
                }
                font.set_filter(FilterMode::Linear);
                println!("UI font with Cyrillic enabled: {path}");
                return Some(font)
            }
        }
    }
    eprintln!("Russian UI font is missing: set TLW_FONT to a TTF with Cyrillic glyphs");
    None
}

#[derive(Clone,Copy,Debug)]
struct SpriteTween{
    from:Vec2,
    to:Vec2,
    start_tick:u64,
}
fn wrapped_step(from:(i32,i32),to:(i32,i32),size:i32)->Vec2{
    let wrap=|delta:i32|{
        if delta>size/2{delta-size}
        else if delta < -size/2{delta+size}
        else{delta}
    };
    vec2(wrap(to.0-from.0) as f32,wrap(to.1-from.1) as f32)
}
fn nearest_periodic_axis(value:f32,reference:f32,size:f32)->f32{
    value+((reference-value)/size).round()*size
}
fn tween_cell(tween:&SpriteTween,tick:u64,sub_tick:f32)->Vec2{
    let elapsed=(tick.saturating_sub(tween.start_tick) as f32+sub_tick)
        /MOVE_INTERPOLATION_TICKS;
    tween.from.lerp(tween.to,elapsed.clamp(0.0,1.0))
}
/// Static minimap ground is uploaded to the GPU once per loaded map.
/// Previously the client issued up to 65k individual mini-rect draws per frame.
fn minimap_ground_texture(terrain:&Terrain)->Texture2D{
    let n=terrain.size as u16;
    let mut image=Image::gen_image_color(n,n,Color::from_rgba(71,104,49,255));
    for y in 0..terrain.size{
        for x in 0..terrain.size{
            let color=if terrain.water(x as i32,y as i32){
                Color::from_rgba(30,77,132,255)
            }else{
                let h=terrain.height[terrain.index(x as i32,y as i32)] as f32;
                let factor=((h-128.0)*0.55).clamp(-23.0,23.0) as i32;
                let add=|v:i32| (v+factor).clamp(0,255) as u8;
                Color::from_rgba(add(71),add(104),add(49),255)
            };
            image.set_pixel(x as u32,y as u32,color);
        }
    }
    let texture=Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Nearest);
    texture
}
fn window_conf()->Conf{
    Conf{
        window_title:"THE LOST WORLD — RUST EDITION".into(),
        window_width:1400,
        window_height:860,
        window_resizable:true,
        ..Default::default()
    }
}
fn dir_file(dir:&Path,file:&str)->PathBuf{
    // Relative only. Mission data can use DOS backslashes; respect local folder.
    let name=file.replace('\\',"/");
    let relative=Path::new(&name).file_name()
        .unwrap_or_default().to_string_lossy().into_owned();
    dir.join(relative)
}
fn load_mission(
    data_dir:&Path,mission_file:&str,number:usize,
)->Result<(Mission,GameMap),String>{
    let mdb=dir_file(data_dir,mission_file);
    let missions=parse_mdb(&fs::read(&mdb)
        .map_err(|e|format!("{}: {e}",mdb.display()))?)
        .map_err(|e|format!("Invalid mission database: {e}"))?;
    let mission=missions.get(number).ok_or_else(||format!(
        "Mission {number} missing ({} available)",missions.len()
    ))?.clone();
    let mut filename=mission.map_file.clone();
    if !filename.to_ascii_lowercase().ends_with(".dvm"){
        filename.push_str(".dvm")
    }
    let map_file=dir_file(data_dir,&filename);
    let bytes=fs::read(&map_file)
        .map_err(|e|format!("{}: {e}",map_file.display()))?;
    let map=parse_dvm(
        &bytes,
        if mission.seed==-2 {Some(mission.size_bits)}else{None},
        mission.addon,
    ).map_err(|e|format!("Cannot parse map {}: {e}",map_file.display()))?;
    println!(
        "MISSION {} : {} | terrain seed {} | map {} | {} objects",
        mission.order,mission.title,mission.seed,map_file.display(),map.objects.len()
    );
    Ok((mission,map))
}
fn human_worker_if_needed(sim:&mut Simulation,starts:&[(i32,i32)]){
    if !sim.selected_unit_ids().is_empty(){return}
    let mut centers=sim.objects.iter()
        .filter(|o|o.kind==1&&o.group==sim.human_race&&o.object_type==40)
        .map(|o|(o.x,o.y)).collect::<Vec<_>>();
    centers.extend(starts.iter().copied());
    let mut spawned=0usize;
    for (cx,cy) in centers{
        for distance in 1..=9{
            for delta in -distance..=distance {
                for &(dx,dy) in &[(distance,delta),(-distance,delta),(delta,distance),(delta,-distance)]{
                    let n=sim.terrain.size as i32;
                    let x=(cx+dx).rem_euclid(n);
                    let y=(cy+dy).rem_euclid(n);
                    if sim.terrain.water(x,y)||sim.object_at(x,y).is_some(){
                        continue
                    }
                    let id=sim.next_id;
                    sim.next_id+=1;
                    // Original completed town halls release a builder and
                    // two porters. Until full indoor-unit logic is ported,
                    // place those three outside at nearby walkable cells.
                    let role=if spawned==0{11}else{9};
                    sim.objects.push(Entity{
                        id,kind:0,group:sim.human_race,object_type:role,
                        owner:sim.human_race,x,y,hp:100,complete:true,
                        construction:100,task:Task::Idle,carrying:0,facing:4
                    });
                    spawned+=1;
                    println!("Initial worker {spawned}/3 (type {role}) at {x},{y}");
                    if spawned>=3{return}
                }
            }
        }
    }
}

#[derive(Default)]
struct Selection{
    unit:Option<u32>,
    units:Vec<u32>,
    object:Option<u32>,
    build:Option<i32>,
    error:Option<(String,f64)>,
}
struct Game {
    simulation:Simulation,
    ground:TerrainArt,
    sprites:WorldArt,
    minimap_ground:Texture2D,
    mission:Mission,
    camera:Vec2,
    zoom:f32,
    selected:Selection,
    accumulator:f32,
    minimap:bool,
    stored:bool,
    save_file:PathBuf,
    toast:Option<(String,f64)>,
    left_press:Option<Vec2>,
    right_press:Option<Vec2>,
    last_click:Option<(u32,f64)>,
    ui_font:Option<Font>,
    lang:Language,
    ui:ClassicUi,
    last_positions:HashMap<u32,(i32,i32)>,
    tweens:HashMap<u32,SpriteTween>,
}
impl Game{
    fn new(
        data_dir:&Path,addon_dir:&Path,mission_name:&str,
        index:usize,
    )->Result<Self,String>{
        let (mission,map)=load_mission(data_dir,mission_name,index)?;
        let base_gd=data_dir.join("dv.gd");
        let addon_path=addon_dir.join("ADDON.GD");
        let addon_gd=addon_path.exists().then_some(addon_path);
        let addon=mission.addon;
        if addon&&addon_gd.is_none(){
            return Err("Mission requires ADDON.GD and ADDON.OD; supply extracted MissionPack directory".into())
        }
        let ground=TerrainArt::load(data_dir,addon)?;
        let picks=ground.pick_tables();
        let terrain=match mission.seed {
            -2=>{
                let stored=map.terrain.as_ref()
                    .ok_or("Gold mission has no embedded terrain")?;
                Terrain::from_stored(
                    stored.size_bits,stored.height.clone(),stored.tile.clone(),
                    &ground.global_class_lookup(),ground.water_class
                ).map_err(|e|format!("Stored Gold terrain: {e}"))?
            }
            seed if seed>=0=>generate_base(seed as u32,mission.size_bits,&picks)
                .map_err(|e|format!("Original terrain generator: {e}"))?,
            -1=>{
                eprintln!(
                    "NOTE: Free-play random terrain seed is provisional; saved campaign missions use original seeds"
                );
                generate_base(12345,mission.size_bits,&picks)
                    .map_err(|e|e.to_string())?
            }
            _=>return Err("Unsupported terrain seed".into())
        };
        let mut rules=parse_odb(&fs::read(data_dir.join("dv.odb"))
            .map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        if addon{
            let bytes=fs::read(addon_dir.join("ADDON.ODB"))
                .map_err(|e|format!("ADDON.ODB: {e}"))?;
            merge_rules(&mut rules,parse_odb(&bytes).map_err(|e|e.to_string())?);
        }
        let human=mission.human_race.max(0);
        let race=&mission.races[human as usize];
        let mut simulation=Simulation::from_map(
            terrain,rules,map.objects.clone(),human,race.gold,race.goods
        ).map_err(|e|e.to_string())?;
        human_worker_if_needed(&mut simulation,&map.starts);
        let minimap_ground=minimap_ground_texture(&simulation.terrain);
        let sprites=WorldArt::open(base_gd,addon_gd)?;
        let focus=simulation.selected_unit_ids().first()
            .and_then(|id|simulation.entity(*id))
            .map(|o|(o.x,o.y))
            .or_else(||simulation.objects.iter()
                .find(|o|o.kind==1&&o.group==human)
                .map(|o|(o.x,o.y)))
            .unwrap_or(map.camera);
        let h=simulation.terrain.height[simulation.terrain.index(focus.0,focus.1)];
        let camera=project(focus.0 as f32,focus.1 as f32,h);
        let save_file=PathBuf::from("saves")
            .join(format!("{}_{index}.tlwsnap",mission_name.trim_end_matches(".mdb")));
        let last_positions=simulation.objects.iter()
            .filter(|o|o.kind==0||o.kind==2
                &&simulation.rule(o).is_some_and(|r|r.class==7))
            .map(|o|(o.id,(o.x,o.y))).collect::<HashMap<_,_>>();
        let ui=ClassicUi::new(data_dir,human);
        let ui_font=load_ui_font();
        let lang=if ui_font.is_some(){Language::Russian}
            else{Language::English};
        Ok(Self{
            simulation,ground,sprites,minimap_ground,mission,camera,zoom:1.0,
            selected:Selection::default(),accumulator:0.0,
            minimap:true,stored:map.terrain.is_some(),save_file,toast:None,
            left_press:None,right_press:None,last_click:None,
            ui_font,lang,ui,last_positions,tweens:HashMap::new(),
        })
    }

    fn center(&self)->Vec2{
        vec2((screen_width()-hud_width())*0.5,screen_height()*0.5)
    }
    fn screen(&self,x:f32,y:f32)->Vec2{
        world_to_screen(&self.simulation.terrain,x,y,self.camera,self.zoom,self.center())
    }
    /// Visual-only interpolation. The authoritative fixed-tick simulation
    /// remains deterministic and all saved coordinates are exact cells.
    fn track_sprite_movements(&mut self){
        let size=self.simulation.terrain.size as i32;
        let tick=self.simulation.tick;
        for obj in &self.simulation.objects{
            let movable=obj.kind==0 || obj.kind==2
                &&self.simulation.rule(obj).is_some_and(|r|r.class==7);
            if !movable{continue}
            let to=(obj.x,obj.y);
            match self.last_positions.insert(obj.id,to){
                Some(from) if from!=to=>{
                    let origin=vec2(from.0 as f32,from.1 as f32);
                    self.tweens.insert(obj.id,SpriteTween{
                        from:origin,to:origin+wrapped_step(from,to,size),
                        start_tick:tick
                    });
                }
                _=>{}
            }
        }
        self.tweens.retain(|id,tween|self.last_positions.contains_key(id)
            &&tick.saturating_sub(tween.start_tick)<8);
    }
    fn reset_sprite_movements(&mut self){
        self.tweens.clear();
        self.last_positions=self.simulation.objects.iter()
            .filter(|o|o.kind==0||o.kind==2
                &&self.simulation.rule(o).is_some_and(|r|r.class==7))
            .map(|o|(o.id,(o.x,o.y))).collect();
    }
    fn visual_cell(&self,entity:&Entity)->Vec2{
        let cell=vec2(entity.x as f32,entity.y as f32);
        let Some(tween)=self.tweens.get(&entity.id) else{return cell};
        let fraction=if self.simulation.paused{0.0}
            else{(self.accumulator/TICK_SECONDS).clamp(0.0,1.0)};
        let v=tween_cell(tween,self.simulation.tick,fraction);
        let size=self.simulation.terrain.size as f32;
        vec2(v.x.rem_euclid(size),v.y.rem_euclid(size))
    }
    /// The original world wraps both axes. Draw the periodic copy nearest
    /// to the camera, so objects stay visible across the map boundary.
    fn nearest_periodic_cell(&self,cell:Vec2)->Vec2{
        let reference=unproject(self.camera);
        let size=self.simulation.terrain.size as f32;
        vec2(
            nearest_periodic_axis(cell.x,reference.x,size),
            nearest_periodic_axis(cell.y,reference.y,size),
        )
    }
    fn object_screen(&self,entity:&Entity)->Vec2{
        let cell=self.nearest_periodic_cell(self.visual_cell(entity));
        self.screen(cell.x,cell.y)
    }
    fn pick_cell(&self,screen:Vec2)->(i32,i32){
        let world=(screen-self.center())/self.zoom+self.camera;
        // Invert elevation as well as planar projection; the old picker
        // missed objects on raised ground by several cells.
        let mut p=unproject(world);
        for _ in 0..4{
            let h=self.simulation.terrain.height[
                self.simulation.terrain.index(p.x.round() as i32,p.y.round() as i32)
            ];
            p=unproject(world+vec2(0.0,h as f32-128.0));
        }
        let n=self.simulation.terrain.size as i32;
        ((p.x.round() as i32).rem_euclid(n),(p.y.round() as i32).rem_euclid(n))
    }
    fn draw_ui(&self,message:&str,x:f32,y:f32,size:f32,color:Color){
        draw_text_ex(message,x,y,TextParams{
            font:self.ui_font.as_ref(),
            font_size:size.round().clamp(8.0,255.0) as u16,
            color,..Default::default()
        });
    }
    fn notify(&mut self,message:impl Into<String>){
        let message=message.into();
        println!("{message}");
        self.toast=Some((message,get_time()+4.0));
    }
    /// OD action matching the visible object's gameplay state, for accurate
    /// hit-testing across roofs, units, and tree canopies.
    fn visual_action(&self,obj:&Entity)->u8{
        if obj.kind==1{return u8::from(obj.complete)}
        if obj.kind==2{
            if self.simulation.rule(obj).is_some_and(|r|r.class==7){
                let phase=animal_cycle_phase(obj.id,self.simulation.tick);
                return if (20..=115).contains(&phase){3}else{0};
            }
            return self.simulation.rule(obj).map_or(0,|r|
                resources::nature_initial_action(r.class,r.product))
        }
        if obj.kind==0{
            return match obj.task{
                Task::Walking{..} if obj.carrying>0=>4,
                Task::Gathering{..}|Task::Constructing{..}=>2,
                Task::Holding{..}|Task::Delivering{..}=>4,
                Task::Idle if obj.carrying==0
                    && unit_idle_phase(obj.id,self.simulation.tick)<20
                    && self.sprites.has_slot(
                        rule_id(obj.group,obj.object_type),3,obj.facing)=>3,
                _=>0,
            }
        }
        0
    }
    /// Hit the actual alpha shape, not just an object's anchor or a ground tile.
    /// Priority follows the original game: unit, nature, building, tree.
    fn hit_sprite(&mut self,mouse:Vec2,right:bool)->Option<u32>{
        let mut candidates:Vec<(usize,i32,u32,Vec2,u32,u8,u8)>=Vec::new();
        for o in &self.simulation.objects{
            let p=self.object_screen(o);
            if (p.x-mouse.x).abs()>480.0*self.zoom
                ||(p.y-mouse.y).abs()>480.0*self.zoom{continue}
            let class=self.simulation.rule(o).map_or(-1,|r|r.class);
            let priority=if right{
                match o.kind{
                    0 if o.group!=self.simulation.human_race=>0,
                    0=>1,
                    2 if class!=8=>2,
                    1 if o.group!=self.simulation.human_race=>3,
                    1=>4,
                    _=>5,
                }
            }else{
                match o.kind{
                    0 if self.simulation.unit_owned(o.id)=>0,
                    0=>1,
                    2 if class!=8=>2,
                    1 if o.group==self.simulation.human_race=>3,
                    1=>4,
                    _=>5,
                }
            };
            candidates.push((
                priority,-(o.x+o.y),o.id,p,
                rule_id(o.group,o.object_type),self.visual_action(o),o.facing
            ));
        }
        candidates.sort_unstable_by_key(|v|(v.0,v.1));
        for (_,_,id,p,oid,action,dir) in candidates{
            if self.sprites.get(oid,action,dir).is_some_and(|art|
                art.hit(p,self.zoom,mouse)){
                return Some(id)
            }
        }
        None
    }
    fn select_one(&mut self,hit:Option<u32>){
        let shift=is_key_down(KeyCode::LeftShift)||is_key_down(KeyCode::RightShift);
        let ctrl=is_key_down(KeyCode::LeftControl)||is_key_down(KeyCode::RightControl);
        let unit=hit.filter(|&id|self.simulation.unit_owned(id));
        if !shift && !ctrl{
            self.selected.units.clear();
        }
        if let Some(id)=unit{
            if ctrl {
                if let Some(index)=self.selected.units.iter().position(|&other|other==id){
                    self.selected.units.remove(index);
                }else if self.selected.units.len()<12{
                    self.selected.units.push(id);
                }
            }else if !self.selected.units.contains(&id)&&self.selected.units.len()<12{
                self.selected.units.push(id);
            }
            let now=get_time();
            if !shift&&!ctrl && self.last_click.is_some_and(|(previous,when)|
                previous==id && now-when<0.36){
                if let Some(origin)=self.simulation.entity(id){
                    let kind=origin.object_type;
                    let cell=(origin.x,origin.y);
                    let ids=self.simulation.objects.iter()
                        .filter(|o|o.kind==0&&self.simulation.unit_owned(o.id)
                            &&o.object_type==kind
                            &&(o.x-cell.0).abs()<=5&&(o.y-cell.1).abs()<=5)
                        .take(12).map(|o|o.id).collect::<Vec<_>>();
                    self.selected.units=ids;
                }
            }
            self.last_click=Some((id,now));
        }else{
            self.last_click=None;
        }
        self.selected.unit=self.selected.units.first().copied();
        self.selected.object=self.selected.unit.or(hit);
    }
    fn select_box(&mut self,a:Vec2,b:Vec2){
        let x0=a.x.min(b.x);let x1=a.x.max(b.x);
        let y0=a.y.min(b.y);let y1=a.y.max(b.y);
        let append=is_key_down(KeyCode::LeftShift)
            ||is_key_down(KeyCode::RightShift);
        if !append{self.selected.units.clear();}
        for o in &self.simulation.objects{
            if !self.simulation.unit_owned(o.id){continue}
            let p=self.object_screen(o);
            if p.x>=x0&&p.x<=x1&&p.y>=y0&&p.y<=y1
                &&!self.selected.units.contains(&o.id)
                &&self.selected.units.len()<12{
                self.selected.units.push(o.id);
            }
        }
        self.selected.unit=self.selected.units.first().copied();
        self.selected.object=self.selected.unit;
    }
    fn issue_order(&mut self,mouse:Vec2){
        let target=self.pick_cell(mouse);
        let target_id=self.hit_sprite(mouse,true);
        let selected=if self.selected.units.is_empty(){
            self.selected.unit.into_iter().collect::<Vec<_>>()
        }else{self.selected.units.clone()};
        if selected.is_empty(){
            self.notify(if self.lang==Language::Russian{
                "Выберите одного или нескольких рабочих ЛКМ"
            }else{"Select one or more workers with LEFT CLICK"});return
        }
        let mut accepted=0;
        let mut error=None;
        for unit in selected{
            let result=if let Some(t)=target_id{
                match self.simulation.entity(t){
                    Some(o) if o.kind==2=>self.simulation.order_gather(unit,t),
                    Some(o) if o.kind==1&&!o.complete
                        =>self.simulation.order_construct(unit,t),
                    _=>self.simulation.order_move(unit,target),
                }
            }else{self.simulation.order_move(unit,target)};
            match result{Ok(())=>accepted+=1,Err(e)=>error=Some(e)}
        }
        if accepted>0{
            self.notify(if self.lang==Language::Russian{
                format!("Приказ принят: {accepted} персонажей → {},{}",target.0,target.1)
            }else{format!("Order accepted: {accepted} units to {},{}",target.0,target.1)})
        }else if let Some(e)=error{
            let text=localization::error_name(self.lang,&e);
            self.notify(if self.lang==Language::Russian{
                format!("Приказ отклонён: {text}")
            }else{format!("Order rejected: {text}")})
        }
    }
    fn handle_mouse_click(&mut self,mouse:Vec2,left:bool){
        if let Some(cell)=self.minimap_cell(mouse){
            if left{
                let h=self.simulation.terrain.height[
                    self.simulation.terrain.index(cell.0,cell.1)];
                self.camera=project(cell.0 as f32,cell.1 as f32,h);
            }else{
                self.issue_cell_order(cell);
            }
            return
        }
        if classic_ui::click(self,mouse,left){return}
        if mouse.y<TOP||mouse.y>screen_height()-FOOT{return}
        let target=self.pick_cell(mouse);
        if left{
            if let Some(ty)=self.selected.build{
                if let Some(unit)=self.selected.unit{
                    let oid=rule_id(self.simulation.human_race,ty);
                    match self.simulation.build(unit,oid,target){
                        Ok(id)=>{
                            self.selected.object=Some(id);
                            self.notify(if self.lang==Language::Russian{
                                format!("Строительство начато: объект №{ty}")
                            }else{format!("Construction started: type {ty}")})
                        }
                        Err(e)=>{
                            let text=localization::error_name(self.lang,&e);
                            self.notify(if self.lang==Language::Russian{
                                format!("Нельзя построить: {text}")
                            }else{format!("Cannot build: {text}")})
                        },
                    }
                }else{
                    self.notify(if self.lang==Language::Russian{
                        "Сначала выберите строителя"
                    }else{"Select a builder before placing a building"})
                }
                return
            }
            let hit=self.hit_sprite(mouse,false);
            self.select_one(hit);
            if hit.is_some(){self.ui.page=HudPage::Selection;}
        }else if self.selected.build.is_some(){
            self.selected.build=None;
        }else{self.issue_order(mouse);}
    }
    fn issue_cell_order(&mut self,cell:(i32,i32)){
        let units=if self.selected.units.is_empty(){
            self.selected.unit.into_iter().collect::<Vec<_>>()
        }else{self.selected.units.clone()};
        if units.is_empty(){return}
        let mut count=0;
        for unit in units{
            if self.simulation.order_move(unit,cell).is_ok(){count+=1}
        }
        if count>0{
            self.notify(if self.lang==Language::Russian{
                format!("Приказ по мини-карте: {count} персонажей → {},{}",cell.0,cell.1)
            }else{format!("Minimap move: {count} units to {},{}",cell.0,cell.1)});
        }
    }

    fn quick_save(&mut self){
        let result=fs::create_dir_all("saves")
            .and_then(|_|fs::write(&self.save_file,self.simulation.save_snapshot()));
        match result{
            Ok(())=>self.notify(if self.lang==Language::Russian{
                "Игра сохранена (F9 — загрузить)"
            }else{"Snapshot saved (F9 to restore)"}),
            Err(e)=>self.notify(if self.lang==Language::Russian{
                format!("Ошибка сохранения: {e}")
            }else{format!("Save failed: {e}")}),
        }
    }
    fn quick_load(&mut self){
        let result=fs::read(&self.save_file)
            .map_err(|e|e.to_string())
            .and_then(|b|self.simulation.load_snapshot(&b).map_err(|e|e.to_string()));
        match result{
            Ok(())=>{
                self.reset_sprite_movements();
                self.notify(if self.lang==Language::Russian{
                    "Сохранение загружено"
                }else{"Snapshot restored"})
            },
            Err(e)=>self.notify(if self.lang==Language::Russian{
                format!("Ошибка загрузки: {e}")
            }else{format!("Load failed: {e}")}),
        }
    }
    fn toggle_language(&mut self){
        if self.ui_font.is_some()||self.lang==Language::Russian{
            self.lang=self.lang.toggle();
            self.notify(if self.lang==Language::Russian{
                "Язык интерфейса: русский"
            }else{"Interface language: English"});
        }
    }
    fn update(&mut self){
        let dt=get_frame_time().min(0.10);
        if is_key_pressed(KeyCode::Escape){
            if self.selected.build.is_some(){
                self.selected.build=None;
            }else{
                self.selected.unit=None;
                self.selected.units.clear();
                self.selected.object=None;
            }
            self.selected.error=None;
        }
        if is_key_pressed(KeyCode::P){self.simulation.paused=!self.simulation.paused}
        if is_key_pressed(KeyCode::F10){self.toggle_language()}
        if is_key_pressed(KeyCode::X){
            if let Some(unit)=self.selected.unit{
                match self.simulation.drop_cargo(unit){
                    Ok(())=>self.notify(if self.lang==Language::Russian{
                        "Груз сброшен, рабочий свободен"
                    }else{"Carried resources dropped; worker is free"}),
                    Err(e)=>{
                        let text=localization::error_name(self.lang,&e);
                        self.notify(if self.lang==Language::Russian{
                            format!("Нельзя сбросить груз: {text}")
                        }else{format!("Cannot drop cargo: {text}")})
                    },
                }
            }
        }
        if is_key_pressed(KeyCode::F4){self.minimap=!self.minimap}
        if is_key_pressed(KeyCode::F5){self.quick_save()}
        if is_key_pressed(KeyCode::F9){self.quick_load()}
        if is_key_pressed(KeyCode::Key1){self.selected.build=Some(37)}
        if is_key_pressed(KeyCode::Key2){self.selected.build=Some(40)}
        if is_key_pressed(KeyCode::Key3){self.selected.build=Some(41)}
        if is_key_pressed(KeyCode::Key0){self.selected.build=None}
        if is_key_pressed(KeyCode::PageUp){self.zoom=0.5}
        if is_key_pressed(KeyCode::PageDown){self.zoom=1.0}
        let pan=PAN_SPEED*dt/self.zoom;
        if is_key_down(KeyCode::W)||is_key_down(KeyCode::Up){self.camera.y-=pan}
        if is_key_down(KeyCode::S)||is_key_down(KeyCode::Down){self.camera.y+=pan}
        if is_key_down(KeyCode::A)||is_key_down(KeyCode::Left){self.camera.x-=pan}
        if is_key_down(KeyCode::D)||is_key_down(KeyCode::Right){self.camera.x+=pan}
        let (mx,my)=mouse_position();
        let mouse=vec2(mx,my);
        let (_wx,wy)=mouse_wheel();
        if wy!=0.0&&mx<screen_width()-hud_width(){
            let before=(mouse-self.center())/self.zoom+self.camera;
            self.zoom=(self.zoom+wy.signum()*0.15).clamp(0.5,2.75);
            self.camera=before-(mouse-self.center())/self.zoom;
        }
        if is_mouse_button_down(MouseButton::Middle){
            self.camera-=mouse_delta_position()/self.zoom;
        }
        if is_mouse_button_pressed(MouseButton::Left){
            self.left_press=Some(mouse);
        }
        if is_mouse_button_released(MouseButton::Left){
            let start=self.left_press.take().unwrap_or(mouse);
            let delta=(mouse-start).abs();
            if delta.x>4.0||delta.y>4.0{
                if start.x<screen_width()-hud_width() && start.y>=TOP
                    && self.selected.build.is_none(){
                    self.select_box(start,mouse);
                }else{
                    self.handle_mouse_click(start,true);
                }
            }else{
                self.handle_mouse_click(start,true);
            }
        }
        if is_mouse_button_pressed(MouseButton::Right){
            self.right_press=Some(mouse);
        }
        if is_mouse_button_released(MouseButton::Right){
            let start=self.right_press.take().unwrap_or(mouse);
            let delta=(mouse-start).abs();
            if delta.x<10.0 && delta.y<10.0{
                self.handle_mouse_click(start,false);
            }
        }
        // Edge scrolling, as in the original RTS. The side panel is excluded.
        if mouse.x<screen_width()-hud_width() && mouse.y>=TOP && mouse.y<screen_height()-FOOT
            && !is_mouse_button_down(MouseButton::Left){
            let edge=15.0;
            if mouse.x<edge{self.camera.x-=pan}
            else if mouse.x>screen_width()-hud_width()-edge{self.camera.x+=pan}
            if mouse.y<TOP+edge{self.camera.y-=pan}
            else if mouse.y>screen_height()-FOOT-edge{self.camera.y+=pan}
        }
        self.accumulator+=dt;
        for _ in 0..MAX_TICKS_PER_FRAME{
            if self.accumulator<TICK_SECONDS{break}
            self.accumulator-=TICK_SECONDS;
            self.simulation.step();
            self.track_sprite_movements();
        }
        if self.accumulator>1.0{
            self.accumulator=TICK_SECONDS
        }
    }

    fn draw_world(&mut self){
        let sw=screen_width();
        let sh=screen_height();
        let width=sw-hud_width();
        let origin=self.center();
        // Derive a rectangle of world cells from the visible screen
        // region. This is far smaller than traversing a full 512x512 map.
        // Unproject the visible corners to avoid the old overdraw of 220x220
        // tiles, while retaining height slack for raised land.
        let corners=[
            vec2(-160.0,-260.0),vec2(width+160.0,-260.0),
            vec2(-160.0,sh+260.0),vec2(width+160.0,sh+260.0)
        ];
        let inv=corners.map(|point|unproject(
            (point-origin)/self.zoom+self.camera
        ));
        let x0=inv.iter().map(|v|v.x).fold(f32::INFINITY,f32::min).floor() as i32-18;
        let y0=inv.iter().map(|v|v.y).fold(f32::INFINITY,f32::min).floor() as i32-18;
        let x1=inv.iter().map(|v|v.x).fold(f32::NEG_INFINITY,f32::max).ceil() as i32+18;
        let y1=inv.iter().map(|v|v.y).fold(f32::NEG_INFINITY,f32::max).ceil() as i32+18;
        let n=self.simulation.terrain.size as i32;
        let time=get_time();
        for y in y0..=y1{
            for x in x0..=x1{
                let top=self.screen(x as f32,y as f32);
                if top.x< -140.0||top.x>width+140.0
                    ||top.y< -200.0||top.y>sh+200.0{continue}
                self.ground.draw_cell(
                    &self.simulation.terrain,x,y,self.stored,
                    self.camera,self.zoom,origin,time,false
                );
            }
        }

        // Animate world objects only when simulation jobs authorize playback.
        let active_harvest:std::collections::HashMap<u32,u16>=self.simulation.objects.iter()
            .filter_map(|o|match &o.task{
                Task::Gathering{tree,time}=>Some((*tree,*time)),
                _=>None,
            }).collect();
        // Objects sorted by ground depth, then by x-screen order;
        // all terrain renders before world objects.
        let mut visible:Vec<(i32,i32,usize)>=self.simulation.objects.iter()
            .enumerate().filter_map(|(index,obj)|{
                let p=self.object_screen(obj);
                if p.x< -350.0||p.x>width+350.0
                    ||p.y< -350.0||p.y>sh+350.0{return None}
                Some((p.y as i32,p.x as i32,index))
            }).collect();
        visible.sort_by_key(|v|(v.0,v.1));
        for (_,_,index) in visible{
            let obj=&self.simulation.objects[index];
            let position=self.object_screen(obj);
            let oid=rule_id(obj.group,obj.object_type);
            let chosen=self.selected.object==Some(obj.id)
                ||self.selected.units.contains(&obj.id);
            if chosen{
                draw_circle_lines(position.x,position.y,
                    21.0*self.zoom,2.0,YELLOW);
            }
            let (action,motion)=if obj.kind==2{
                // Original ODB + OD initial display rules:
                // class 8 (trees) begins at action 4 / CA0 (standing).
                // classes 10-12 (crops) begin at action 1 / ST0.
                // bushes (9), rocks (13), animals, rubble begin at MS0.
                let action=self.simulation.rule(obj).map_or(0,|rule|
                    resources::nature_initial_action(rule.class,rule.product));
                if self.simulation.rule(obj).is_some_and(|r|r.class==7){
                    let phase=animal_cycle_phase(obj.id,self.simulation.tick);
                    // Real animal idle uses the original ID3 row. MG uses
                    // one of the eight walking directions while wandering.
                    if (1..=8).contains(&phase){
                        (0,SpriteMotion::Loop(self.simulation.tick))
                    }else if (20..=115).contains(&phase){
                        (3,SpriteMotion::Loop(self.simulation.tick))
                    }else{
                        (0,SpriteMotion::Still)
                    }
                }else{
                    let motion=match active_harvest.get(&obj.id).copied(){
                        Some(t) if t>=48=>SpriteMotion::Finish{
                            elapsed:(t-48) as u64,duration:12
                        },
                        _=>SpriteMotion::Still,
                    };
                    (action,motion)
                }
            }else if obj.kind==0{
                match &obj.task{
                    Task::Walking{..}=>(
                        if obj.carrying>0{4}else{0},
                        SpriteMotion::Loop(self.simulation.tick)
                    ),
                    Task::Gathering{time,..}=>(2,SpriteMotion::Loop(*time as u64)),
                    Task::Constructing{..}=>(2,SpriteMotion::Loop(self.simulation.tick)),
                    Task::Delivering{..}|Task::Holding{..}=>(4,SpriteMotion::Still),
                    Task::Idle=>{
                        if obj.carrying==0
                            && unit_idle_phase(obj.id,self.simulation.tick)<20
                            && self.sprites.has_slot(oid,3,obj.facing){
                            (3,SpriteMotion::Loop(self.simulation.tick))
                        }else{(0,SpriteMotion::Still)}
                    },
                }
            }else if obj.kind==1{
                if obj.complete{(1,SpriteMotion::Still)}
                else{(0,SpriteMotion::Finish{
                    elapsed:obj.construction as u64,duration:101
                })}
            }else{(0,SpriteMotion::Still)};
            if let Some(art)=self.sprites.get(oid,action,obj.facing){
                let tint=if obj.kind==1 && !obj.complete{
                    Color::from_rgba(225,225,193,120)
                }else{WHITE};
                art.draw(position,self.zoom,motion,tint,obj.id,
                    self.simulation.tick,obj.kind==1&&obj.complete,false)
            }else{
                // Some rare original object descriptors have no matching GD art.
                // Show a debug symbol instead of silently hiding game entities.
                let color=match obj.kind{0=>SKYBLUE,1=>ORANGE,_=>GREEN};
                draw_circle(position.x,position.y,5.0*self.zoom,color);
            }
            if obj.kind==1&&!obj.complete{
                let w=42.0*self.zoom;
                let x=position.x-w/2.0;
                let y=position.y+5.0*self.zoom;
                draw_rectangle(x,y,w,4.0,DARKGRAY);
                draw_rectangle(x,y,w*obj.construction as f32/100.0,4.0,YELLOW);
            }
        }
        // Preview each cell of the actual isometric construction footprint.
        if let (Some(ty),Some(unit))=(self.selected.build,self.selected.unit){
            let (mx,my)=mouse_position();
            if mx>=0.0&&mx<width&&my>=TOP&&my<sh-FOOT{
                let at=self.pick_cell(vec2(mx,my));
                let id=rule_id(self.simulation.human_race,ty);
                if let Some(rule)=self.simulation.rules.get(&id){
                    let valid=self.simulation.can_build(unit,id,at).is_ok();
                    let color=if valid{Color::from_rgba(85,222,102,230)}
                        else{Color::from_rgba(238,85,72,230)};
                    for dx in 0..rule.size_x.clamp(1,32){
                        for dy in 0..rule.size_y.clamp(1,32){
                            let x=at.0-dx;
                            let y=at.1-dy;
                            if x<0||y<0||x>=n-1||y>=n-1{continue}
                            let a=self.screen(x as f32,y as f32);
                            let b=self.screen((x+1) as f32,y as f32);
                            let c=self.screen((x+1) as f32,(y+1) as f32);
                            let d=self.screen(x as f32,(y+1) as f32);
                            for (p,q) in [(a,b),(b,c),(c,d),(d,a)]{
                                draw_line(p.x,p.y,q.x,q.y,2.0,color);
                            }
                        }
                    }
                }
            }
        }
    }

    fn minimap(&self,x:f32,y:f32,width:f32,height:f32){
        let n=self.simulation.terrain.size as f32;
        draw_texture_ex(&self.minimap_ground,x,y,WHITE,DrawTextureParams{
            dest_size:Some(vec2(width,height)),..Default::default()
        });
        for obj in &self.simulation.objects{
            if (obj.kind==1&&self.ui.show_buildings)
                ||(obj.kind==0&&self.ui.show_units)
                ||(obj.kind==2&&self.ui.show_resources){
                let color=if obj.group==self.simulation.human_race{GOLD}else{RED};
                draw_rectangle(x+obj.x as f32/n*width,
                    y+obj.y as f32/n*height,3.0,3.0,color)
            }
        }
        // Lightweight camera indicator, updated per frame.
        let centre=unproject(self.camera);
        draw_circle_lines(x+centre.x/n*width,y+centre.y/n*height,
            7.0,1.0,WHITE);
    }
    fn minimap_bounds(&self)->Option<(f32,f32,f32,f32)>{
        if !self.minimap{return None}
        let p=panel_point(6.0,6.0);
        let scale=hud_scale();
        Some((p.x,p.y,152.0*scale,152.0*scale))
    }
    fn minimap_cell(&self,p:Vec2)->Option<(i32,i32)>{
        let (x,y,w,h)=self.minimap_bounds()?;
        if p.x<x||p.y<y||p.x>=x+w||p.y>=y+h{return None}
        let n=self.simulation.terrain.size as f32;
        Some((((p.x-x)/w*n) as i32,((p.y-y)/h*n) as i32))
    }
    fn draw_selection_marquee(&self){
        if self.selected.build.is_some(){return}
        let Some(start)=self.left_press else{return};
        if start.x>=screen_width()-hud_width()||start.y<TOP{return}
        if !is_mouse_button_down(MouseButton::Left){return}
        let (x,y)=mouse_position();
        let end=vec2(x.min(screen_width()-hud_width()),y.min(screen_height()-FOOT));
        if (end-start).length()<4.0{return}
        let x=start.x.min(end.x);let y=start.y.min(end.y);
        let w=(end.x-start.x).abs();let h=(end.y-start.y).abs();
        draw_rectangle_lines(x,y,w,h,1.5,GOLD);
    }
    fn draw_hud(&self){
        classic_ui::draw(self);
    }
    async fn run(mut self){
        #[cfg(all(not(target_arch="wasm32"),not(target_os="android")))]
        let limit=std::time::Duration::from_secs_f64(
            1.0/env::var("TLW_MAX_FPS").ok()
                .and_then(|x|x.parse::<f64>().ok())
                .filter(|fps|fps.is_finite()&&*fps>=30.0&&*fps<=144.0)
                .unwrap_or(60.0)
        );
        loop{
            #[cfg(all(not(target_arch="wasm32"),not(target_os="android")))]
            let frame_start=std::time::Instant::now();
            self.update();
            clear_background(Color::from_rgba(26,42,37,255));
            self.draw_world();
            self.draw_selection_marquee();
            self.draw_hud();
            #[cfg(all(not(target_arch="wasm32"),not(target_os="android")))]
            {
                let elapsed=frame_start.elapsed();
                if elapsed<limit{
                    std::thread::sleep(limit-elapsed);
                }
            }
            next_frame().await;
        }
    }
}

#[macroquad::main(window_conf)]
async fn main(){
    let args:Vec<_>=env::args_os().skip(1).collect();
    if args.len()<2{
        eprintln!("Usage: the-lost-world <base-Data-dir> <MissionPack-assets-dir> [mission.mdb] [mission-index]");
        eprintln!("Example: cargo run --manifest-path rust/game-client/Cargo.toml -- \"C:\\GameAssets\\Base\\Data\" \"C:\\GameAssets\\MissionPack\" Sajikis.mdb 0");
        return
    }
    let data_dir=PathBuf::from(&args[0]);
    let addon_dir=PathBuf::from(&args[1]);
    let mission_name=args.get(2).map(|s|s.to_string_lossy().to_string())
        .unwrap_or_else(||"Sajikis.mdb".to_owned());
    let number=args.get(3).and_then(|s|s.to_string_lossy().parse::<usize>().ok())
        .unwrap_or(0);
    match Game::new(&data_dir,&addon_dir,&mission_name,number){
        Ok(game)=>game.run().await,
        Err(error)=>{
            eprintln!("Cannot launch game: {error}");
            loop{
                clear_background(Color::from_rgba(26,36,33,255));
                draw_text("THE LOST WORLD - LOAD ERROR",40.0,100.0,32.0,ORANGE);
                let chunks:Vec<_>=error.as_bytes().chunks(90).collect();
                for (i,chunk) in chunks.iter().enumerate(){
                    let message=String::from_utf8_lossy(chunk);
                    draw_text(&message,40.0,160.0+i as f32*35.0,21.0,WHITE);
                }
                draw_text("See PowerShell for details. Press ESC to exit.",40.0,300.0,20.0,LIGHTGRAY);
                if is_key_pressed(KeyCode::Escape){break}
                next_frame().await;
            }
        }
    }
}

#[cfg(test)]
mod motion_tests{
    use super::*;
    #[test]
    fn interpolates_one_original_grid_step_over_six_ticks(){
        let motion=SpriteTween{
            from:vec2(10.0,10.0),to:vec2(11.0,10.0),start_tick:100
        };
        assert_eq!(tween_cell(&motion,100,0.0),vec2(10.0,10.0));
        assert_eq!(tween_cell(&motion,103,0.0),vec2(10.5,10.0));
        assert_eq!(tween_cell(&motion,106,0.0),vec2(11.0,10.0));
        assert_eq!(tween_cell(&motion,110,0.0),vec2(11.0,10.0));
        assert_eq!(wrapped_step((127,3),(0,3),128),vec2(1.0,0.0));
        assert_eq!(wrapped_step((0,3),(127,3),128),vec2(-1.0,0.0));
    }
}
