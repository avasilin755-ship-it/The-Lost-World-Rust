//! The Lost World — integrated Rust game client (Windows/Linux prototype).
//! Real original mission & map inputs + terrain generation + GD/OD rendering.
//! Fixed-step gameplay: selection, A*, gathering, transport to storage,
//! construction from ODB costs, local snapshots and minimap.
//!
//! This playable scaffold is not yet full faithful original simulation.

mod resources;

use game_core::{
    mission::{GameMap,Mission,parse_dvm,parse_mdb},
    rules::{parse_odb,merge_rules,rule_id},
    sim::{Simulation,Entity,Task},
    terrain::{Terrain,generate_base},
};
use macroquad::prelude::*;
use resources::{WorldArt,TerrainArt,project,unproject,world_to_screen};
use std::{env,fs,path::{Path,PathBuf}};

const TOP:f32=91.0;
const SIDE:f32=290.0;
const FOOT:f32=55.0;
const PAN_SPEED:f32=500.0;
const TICK_SECONDS:f32=0.10;
const MAX_TICKS_PER_FRAME:usize=5;
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
                    sim.objects.push(Entity{
                        id,kind:0,group:sim.human_race,object_type:11,
                        owner:sim.human_race,x,y,hp:100,complete:true,
                        construction:100,task:Task::Idle,carrying:0
                    });
                    println!("Demo spawn: builder at {x},{y} because the map has no surface units");
                    return
                }
            }
        }
    }
}

#[derive(Default)]
struct Selection{
    unit:Option<u32>,
    object:Option<u32>,
    build:Option<i32>,
    error:Option<(String,f64)>,
}
struct Game {
    simulation:Simulation,
    ground:TerrainArt,
    sprites:WorldArt,
    mission:Mission,
    camera:Vec2,
    zoom:f32,
    selected:Selection,
    accumulator:f32,
    minimap:bool,
    stored:bool,
    save_file:PathBuf,
    toast:Option<(String,f64)>,
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
        Ok(Self{
            simulation,ground,sprites,mission,camera,zoom:1.0,
            selected:Selection::default(),accumulator:0.0,
            minimap:true,stored:map.terrain.is_some(),save_file,toast:None,
        })
    }

    fn center(&self)->Vec2{
        vec2((screen_width()-SIDE)*0.5,screen_height()*0.52)
    }
    fn screen(&self,x:f32,y:f32)->Vec2{
        world_to_screen(&self.simulation.terrain,x,y,self.camera,self.zoom,self.center())
    }
    fn pick_cell(&self,screen:Vec2)->(i32,i32){
        let world=(screen-self.center())/self.zoom+self.camera;
        let p=unproject(world);
        let n=self.simulation.terrain.size as i32;
        ((p.x.round() as i32).rem_euclid(n),(p.y.round() as i32).rem_euclid(n))
    }
    fn notify(&mut self,message:impl Into<String>){
        let message=message.into();
        println!("{message}");
        self.toast=Some((message,get_time()+4.0));
    }
    fn handle_mouse_click(&mut self,mouse:Vec2,left:bool){
        if mouse.y<TOP||mouse.y>screen_height()-FOOT||
            mouse.x>=screen_width()-SIDE{return}
        let target=self.pick_cell(mouse);
        if left{
            if let Some(ty)=self.selected.build{
                if let Some(unit)=self.selected.unit{
                    let oid=rule_id(self.simulation.human_race,ty);
                    match self.simulation.build(unit,oid,target){
                        Ok(id)=>{
                            self.selected.object=Some(id);
                            self.notify(format!("Construction started: type {ty}"))
                        }
                        Err(e)=>self.notify(format!("Cannot build: {e}")),
                    }
                }else{self.notify("Select your builder before placing a building")}
                return
            }
            // Pick the closest displayed sprite anchor above the terrain.
            // This accepts clicks on the upper parts of tall objects.
            let hit=self.simulation.objects.iter().rev().filter_map(|o|{
                let p=self.screen(o.x as f32,o.y as f32);
                let dx=(mouse.x-p.x).abs();
                let dy=p.y-mouse.y;
                (dx<45.0*self.zoom && (-18.0*self.zoom..=190.0*self.zoom).contains(&dy))
                    .then_some((o.id,(dx+dy.abs()*0.4) as i32))
            }).min_by_key(|(_,score)|*score).map(|(id,_)|id)
                .or_else(||self.simulation.object_at(target.0,target.1));
            self.selected.object=hit;
            self.selected.unit=hit.filter(|&id|self.simulation.unit_owned(id));
            return
        }
        let Some(unit)=self.selected.unit else{
            self.notify("Select one of your units with LEFT CLICK");return
        };
        let target_id=self.simulation.object_at(target.0,target.1);
        let result=if let Some(t)=target_id{
            match self.simulation.entity(t){
                Some(o) if o.kind==2=>self.simulation.order_gather(unit,t),
                _=>self.simulation.order_move(unit,target)
            }
        }else{self.simulation.order_move(unit,target)};
        match result{
            Ok(())=>self.notify(format!("Order accepted: [{},{}]",target.0,target.1)),
            Err(e)=>self.notify(format!("Order rejected: {e}"))
        }
    }

    fn update(&mut self){
        let dt=get_frame_time().min(0.10);
        if is_key_pressed(KeyCode::Escape){
            self.selected.build=None;
            self.selected.error=None;
        }
        if is_key_pressed(KeyCode::P){self.simulation.paused=!self.simulation.paused}
        if is_key_pressed(KeyCode::F4){self.minimap=!self.minimap}
        if is_key_pressed(KeyCode::F5){
            let result=fs::create_dir_all("saves")
                .and_then(|_|fs::write(&self.save_file,self.simulation.save_snapshot()));
            match result{Ok(())=>self.notify("Snapshot saved (F9 to restore)"),
                Err(e)=>self.notify(format!("Save failed: {e}"))}
        }
        if is_key_pressed(KeyCode::F9){
            let result=fs::read(&self.save_file)
                .map_err(|e|e.to_string())
                .and_then(|b|self.simulation.load_snapshot(&b).map_err(|e|e.to_string()));
            match result{
                Ok(())=>self.notify("Snapshot restored"),
                Err(e)=>self.notify(format!("Load failed: {e}"))
            }
        }
        if is_key_pressed(KeyCode::Key1){self.selected.build=Some(37)}
        if is_key_pressed(KeyCode::Key2){self.selected.build=Some(40)}
        if is_key_pressed(KeyCode::Key3){self.selected.build=Some(41)}
        if is_key_pressed(KeyCode::Key0){self.selected.build=None}
        let pan=PAN_SPEED*dt/self.zoom;
        if is_key_down(KeyCode::W)||is_key_down(KeyCode::Up){self.camera.y-=pan}
        if is_key_down(KeyCode::S)||is_key_down(KeyCode::Down){self.camera.y+=pan}
        if is_key_down(KeyCode::A)||is_key_down(KeyCode::Left){self.camera.x-=pan}
        if is_key_down(KeyCode::D)||is_key_down(KeyCode::Right){self.camera.x+=pan}
        let (mx,my)=mouse_position();
        let mouse=vec2(mx,my);
        let (_wx,wy)=mouse_wheel();
        if wy!=0.0&&mx<screen_width()-SIDE{
            let before=(mouse-self.center())/self.zoom+self.camera;
            self.zoom=(self.zoom+wy.signum()*0.15).clamp(0.5,2.75);
            self.camera=before-(mouse-self.center())/self.zoom;
        }
        if is_mouse_button_down(MouseButton::Middle){
            self.camera-=mouse_delta_position()/self.zoom;
        }
        if is_mouse_button_pressed(MouseButton::Left){
            self.handle_mouse_click(mouse,true)
        }
        if is_mouse_button_pressed(MouseButton::Right){
            self.handle_mouse_click(mouse,false)
        }
        self.accumulator+=dt;
        for _ in 0..MAX_TICKS_PER_FRAME{
            if self.accumulator<TICK_SECONDS{break}
            self.accumulator-=TICK_SECONDS;
            self.simulation.step();
        }
        if self.accumulator>1.0{
            self.accumulator=TICK_SECONDS
        }
    }

    fn draw_world(&mut self){
        let sw=screen_width();
        let sh=screen_height();
        let width=sw-SIDE;
        let origin=self.center();
        // Derive a rectangle of world cells from the visible screen
        // region. This is far smaller than traversing a full 512x512 map.
        let center=unproject(self.camera);
        let extent=((width+sh)/32.0/self.zoom).ceil() as i32+12;
        let range=extent.clamp(22,110);
        let cx=center.x.round() as i32;
        let cy=center.y.round() as i32;
        let n=self.simulation.terrain.size as i32;
        let x0=(cx-range).max(0);
        let y0=(cy-range).max(0);
        let x1=(cx+range).min(n-2);
        let y1=(cy+range).min(n-2);
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

        // Objects sorted by ground depth, then by x-screen order;
        // all terrain renders before world objects.
        let mut visible:Vec<(i32,i32,usize)>=self.simulation.objects.iter()
            .enumerate().filter_map(|(index,obj)|{
                let p=self.screen(obj.x as f32,obj.y as f32);
                if p.x< -350.0||p.x>width+350.0
                    ||p.y< -350.0||p.y>sh+350.0{return None}
                Some((obj.x+obj.y,obj.y-obj.x,index))
            }).collect();
        visible.sort_by_key(|v|(v.0,v.1));
        for (_,_,index) in visible{
            let obj=&self.simulation.objects[index];
            let position=self.screen(obj.x as f32,obj.y as f32);
            let oid=rule_id(obj.group,obj.object_type);
            let chosen=self.selected.object==Some(obj.id);
            if chosen{
                draw_circle_lines(position.x,position.y,
                    21.0*self.zoom,2.0,YELLOW);
            }
            if let Some(art)=self.sprites.get(oid){
                art.draw(position,self.zoom,time)
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
    }

    fn minimap(&self,x:f32,y:f32,width:f32,height:f32){
        let n=self.simulation.terrain.size as f32;
        let step=if n>256.0{4}else if n>128.0{2}else{1};
        let step=step as usize;
        let s=step as f32;
        for cy in (0..self.simulation.terrain.size).step_by(step){
            for cx in (0..self.simulation.terrain.size).step_by(step){
                let color=if self.simulation.terrain.water(cx as i32,cy as i32){
                    Color::from_rgba(30,77,132,255)
                }else{
                    Color::from_rgba(71,104,49,255)
                };
                draw_rectangle(x+cx as f32/n*width,y+cy as f32/n*height,
                    s/n*width+0.1,s/n*height+0.1,color)
            }
        }
        for obj in &self.simulation.objects{
            if obj.kind==1{
                let color=if obj.group==self.simulation.human_race{GOLD}else{RED};
                draw_rectangle(x+obj.x as f32/n*width,
                    y+obj.y as f32/n*height,3.0,3.0,color)
            }
        }
    }

    fn draw_hud(&self){
        let sw=screen_width();let sh=screen_height();
        let side_x=sw-SIDE;
        draw_rectangle(0.0,0.0,sw,TOP,Color::from_rgba(20,29,25,255));
        draw_text("THE LOST WORLD  |  RUST EDITION",20.0,38.0,29.0,GOLD);
        draw_text(
            &format!("GOLD: {}     WOOD: {}     STONE: {}     FOOD: {}     TICK: {}     {}",
                self.simulation.gold,self.simulation.goods[1],
                self.simulation.goods[2],self.simulation.goods[4],
                self.simulation.tick,
                if self.simulation.paused{"PAUSED"}else{"RUNNING"}),
            20.0,72.0,22.0,WHITE
        );
        draw_rectangle(side_x,TOP,SIDE,sh-TOP,Color::from_rgba(26,36,30,255));
        draw_text("CAMPAIGN",side_x+16.0,TOP+35.0,23.0,GOLD);
        let title=&self.mission.title;
        let title=if title.chars().count()>30 {
            title.chars().take(27).collect::<String>()+"..."
        }else{title.clone()};
        draw_text(&title,side_x+16.0,TOP+66.0,18.0,WHITE);
        draw_text(
            &format!("Map: {}x{}  |  Race {}",
                self.simulation.terrain.size,self.simulation.terrain.size,
                self.simulation.human_race),
            side_x+16.0,TOP+92.0,16.0,LIGHTGRAY
        );
        draw_line(side_x+10.0,TOP+110.0,sw-10.0,TOP+110.0,1.0,GRAY);
        draw_text("BUILD MENU",side_x+16.0,TOP+142.0,22.0,GOLD);
        for (i,ty,name) in [(0,37,"1 - HOUSE"),(1,40,"2 - TOWN HALL"),
            (2,41,"3 - WAREHOUSE")]{
            let y=TOP+170.0+i as f32*43.0;
            let active=self.selected.build==Some(ty);
            draw_rectangle(side_x+12.0,y,SIDE-24.0,34.0,
                if active{Color::from_rgba(88,93,52,255)}
                else{Color::from_rgba(49,60,43,255)});
            draw_text(name,side_x+23.0,y+24.0,20.0,WHITE);
        }
        draw_text("0 - CANCEL BUILD",side_x+17.0,TOP+323.0,17.0,LIGHTGRAY);
        draw_text("SELECTED",side_x+16.0,TOP+372.0,22.0,GOLD);
        if let Some(id)=self.selected.object{
            if let Some(o)=self.simulation.entity(id){
                let name=self.simulation.rule(o)
                    .map_or("UNKNOWN",|r|r.name.as_str());
                let shown=if name.chars().count()>24{
                    name.chars().take(24).collect::<String>()
                }else{name.to_owned()};
                draw_text(&shown,side_x+16.0,TOP+403.0,19.0,WHITE);
                draw_text(&format!("Type {} | HP {} | {},{}",
                    o.object_type,o.hp,o.x,o.y),
                    side_x+16.0,TOP+428.0,17.0,LIGHTGRAY);
                let task=match &o.task {
                    Task::Idle=>"Idle",
                    Task::Walking{..}=>"Moving",
                    Task::Gathering{..}=>"Gathering",
                    Task::Delivering{..}=>"Delivering",
                    Task::Constructing{..}=>"Building",
                };
                draw_text(&format!("Task: {task}"),side_x+16.0,TOP+454.0,
                    17.0,YELLOW);
            }
        }else{
            draw_text("Click a unit or object",side_x+16.0,TOP+403.0,18.0,WHITE)
        }
        if self.minimap && sh>710.0{
            let max=(sh-TOP-FOOT-508.0).clamp(90.0,170.0);
            let map_y=sh-FOOT-max-20.0;
            self.minimap(side_x+22.0,map_y,SIDE-44.0,max);
        }
        draw_rectangle(0.0,sh-FOOT,sw,FOOT,Color::from_rgba(17,25,22,255));
        draw_text(
            "LMB SELECT / PLACE    RMB ORDER / HARVEST    WASD PAN    WHEEL ZOOM    1-3 BUILD    0 CANCEL",
            14.0,sh-30.0,19.0,LIGHTGRAY
        );
        draw_text(
            "P PAUSE    F4 MINIMAP    F5 SAVE    F9 LOAD    middle-drag PAN",
            14.0,sh-10.0,16.0,GRAY
        );
        if let Some((message,until))=&self.toast{
            if get_time()<*until{
                let box_width=message.len().min(70) as f32*11.0+20.0;
                draw_rectangle(18.0,TOP+20.0,box_width,42.0,
                    Color::from_rgba(18,26,22,240));
                draw_text(message,27.0,TOP+49.0,20.0,YELLOW);
            }
        }
    }
    async fn run(mut self){
        loop{
            self.update();
            clear_background(Color::from_rgba(26,42,37,255));
            self.draw_world();
            self.draw_hud();
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
