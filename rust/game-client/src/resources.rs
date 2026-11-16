//! Original local game resources in a standalone Macroquad client:
//! GD sprite archives + OD descriptors, OS ground sets, native 64x44 projection.

use game_core::{
    gd::{GdEntry, OpacityMode, decode_gd_frames, gd_index_size, parse_gd_index},
    od::{OdAnchor, OdIndex, OdObject, parse_od},
    os::parse_os,
    terrain::Terrain,
};
use macroquad::prelude::*;
use std::{
    collections::{HashMap, HashSet},fs::{self,File},
    io::{Read,Seek,SeekFrom},path::{Path,PathBuf},
};

pub const CLASS_SETS_BASE:[&str;12]=[
    "wiese","gelbgras","schnee","rock","waldboden","waldboden",
    "gruenwald","waldboden","gelbgras","gruenwald","weg","wasser1"
];
pub const CLASS_SETS_ADDON:[&str;11]=[
    "weg","weg","schnee","rock","wiese","waldboden",
    "gruenwald","gelbgras","weg","wasser1","wasser1"
];

pub fn project(x:f32,y:f32,height:u16)->Vec2{
    vec2((y-x)*32.0,(x+y)*22.0-(height as f32-128.0))
}
pub fn unproject(screen:Vec2)->Vec2{
    let a=screen.x/32.0;let b=screen.y/22.0;
    vec2((b-a)*0.5,(b+a)*0.5)
}
pub fn world_to_screen(
    terrain:&Terrain,x:f32,y:f32,camera:Vec2,zoom:f32,origin:Vec2,
)->Vec2{
    let n=terrain.size as i32;
    let h=terrain.height[terrain.index(
        (x.floor() as i32).rem_euclid(n),(y.floor() as i32).rem_euclid(n)
    )];
    (project(x,y,h)-camera)*zoom+origin
}
pub struct TileSet {
    pub textures:Vec<Texture2D>,
    pub pick:[u8;256],
    /// True per-tile RGB mean from the original 64x64 OS artwork.
    pub minimap_rgb:Vec<[u8;3]>,
}
pub struct TerrainArt{
    sets:HashMap<String,TileSet>,
    class_names:Vec<&'static str>,
    starts:Vec<usize>,
    pub water_class:u8,
}

const MASKS:[u8;14]=[
    0b1100,0b1010,0b0011,0b0101,0b1000,0b0010,0b0001,
    0b0100,0b1110,0b1011,0b0111,0b1101,0b0110,0b1001
];

impl TerrainArt{
    pub fn load(dir:&Path,addon:bool)->Result<Self,String>{
        let class_names:Vec<_>=if addon{
            CLASS_SETS_ADDON.to_vec()
        }else{CLASS_SETS_BASE.to_vec()};
        let mut required:HashSet<&str>=class_names.iter().copied().collect();
        required.extend(["wasser1","wasser2","wasser3","wasser4",
            "transition1","transition2","transition3","transition4"]);
        let mut sets=HashMap::new();
        for name in required{
            let path=dir.join(format!("{name}.os"));
            let bytes=fs::read(&path)
                .map_err(|e|format!("Cannot open {}: {e}",path.display()))?;
            let tiles=parse_os(&bytes)
                .map_err(|e|format!("Cannot decode {}: {e}",path.display()))?;
            let mut textures=Vec::with_capacity(tiles.tiles.len());
            let mut minimap_rgb=Vec::with_capacity(tiles.tiles.len());
            for tile in tiles.tiles{
                let mut sum=[0u64;3];
                let pixel_count=(tile.width*tile.height) as u64;
                for px in tile.rgba.chunks_exact(4){
                    for channel in 0..3{sum[channel]+=u64::from(px[channel])}
                }
                minimap_rgb.push([
                    (sum[0]/pixel_count) as u8,
                    (sum[1]/pixel_count) as u8,
                    (sum[2]/pixel_count) as u8,
                ]);
                let texture=Texture2D::from_rgba8(
                    tile.width as u16,tile.height as u16,&tile.rgba
                );
                texture.set_filter(FilterMode::Nearest);
                textures.push(texture);
            }
            sets.insert(name.to_owned(),TileSet{
                textures,pick:tiles.pick,minimap_rgb
            });
        }
        let mut start=0usize;
        let mut starts=Vec::new();
        for name in &class_names{
            starts.push(start);
            start+=sets[*name].textures.len();
        }
        if start>256{return Err("Original global tile indices exceed 256".into())}
        Ok(Self{sets,class_names,starts,water_class:if addon{9}else{11}})
    }
    pub fn pick_tables(&self)->[[u8;256];12]{
        let mut p=[[0u8;256];12];
        for (i,name) in self.class_names.iter().enumerate(){
            p[i]=self.sets[*name].pick;
        }
        p
    }
    pub fn global_class_lookup(&self)->[u8;256]{
        let mut result=[0u8;256];
        for (c,name) in self.class_names.iter().enumerate(){
            let start=self.starts[c];
            let length=self.sets[*name].textures.len();
            for value in &mut result[start..start+length]{
                *value=c as u8;
            }
        }
        result
    }
    /// Original minimap uses the mean RGB of each loaded .os tile,
    /// not a binary green/blue approximation. Gold maps store global
    /// palette indices; base game maps store indices local to class.
    pub fn minimap_rgb(&self,terrain:&Terrain,x:i32,y:i32,stored:bool)->[u8;3]{
        let idx=terrain.index(x,y);
        let class=terrain.class[idx] as usize;
        let name=self.class_names.get(class).copied().unwrap_or("wiese");
        let tiles=&self.sets[name].minimap_rgb;
        let variant=terrain.tile[idx] as usize;
        let local=if stored{
            variant.saturating_sub(*self.starts.get(class).unwrap_or(&0))
        }else{variant};
        tiles[local%tiles.len()]
    }
    fn get(&self,name:&str,index:usize)->&Texture2D{
        let set=&self.sets[name];
        &set.textures[index%set.textures.len()]
    }
    /// Screen projection for this cell's four vertices. Heights can differ.
    pub fn draw_cell(
        &self,terrain:&Terrain,x:i32,y:i32,stored:bool,
        camera:Vec2,zoom:f32,origin:Vec2,time:f64,selected:bool,
    ){
        let corners=[(x,y),(x+1,y),(x,y+1),(x+1,y+1)]; // top,left,right,bottom
        let corners_class=[
            terrain.at(x,y),terrain.at(x+1,y),terrain.at(x,y+1),terrain.at(x+1,y+1)
        ];
        let mask=(u8::from(corners_class[3]>=self.water_class))
            |(u8::from(corners_class[2]>=self.water_class)<<1)
            |(u8::from(corners_class[1]>=self.water_class)<<2)
            |(u8::from(corners_class[0]>=self.water_class)<<3);
        let class=corners_class[3] as usize;
        let variant=terrain.tile[terrain.index(x+1,y+1)] as usize;
        let variant=if stored{
            variant.saturating_sub(*self.starts.get(class).unwrap_or(&0))
        }else{variant};
        let frame=(time.floor() as usize)%4;
        const WATER_NAMES:[&str;4]=["wasser1","wasser2","wasser3","wasser4"];
        const COAST_NAMES:[&str;4]=[
            "transition1","transition2","transition3","transition4"
        ];
        let texture=if mask>0&&mask<15{
            let n=MASKS.iter().position(|&m|m==mask).unwrap_or(0);
            self.get(COAST_NAMES[frame],n)
        }else if mask==15{
            if class>=self.water_class as usize+1||class==9&&self.water_class==9{
                self.get("wasser1",variant)
            }else{
                self.get(WATER_NAMES[frame],variant)
            }
        }else{
            let name=self.class_names.get(class).copied().unwrap_or("wiese");
            self.get(name,variant)
        };

        let coord=|i:usize| {
            let (px,py)=corners[i];
            let h=terrain.height[terrain.index(px,py)];
            (project(px as f32,py as f32,h)-camera)*zoom+origin
        };
        let top=coord(0);let left=coord(1);let right=coord(2);let bottom=coord(3);
        let mesh_for=|tex:&Texture2D,alphas:[u8;4]|{
            let shade=|x:i32,y:i32,alpha:u8|{
                let light=terrain.light[terrain.index(x,y)];
                Color::from_rgba(light,light,light,alpha)
            };
            Mesh{
                vertices:vec![
                    Vertex::new(top.x,top.y,0.0,1.0,1.0,shade(x,y,alphas[0])),
                    Vertex::new(left.x,left.y,0.0,0.0,1.0,shade(x+1,y,alphas[1])),
                    Vertex::new(right.x,right.y,0.0,1.0,0.0,shade(x,y+1,alphas[2])),
                    Vertex::new(bottom.x,bottom.y,0.0,0.0,0.0,shade(x+1,y+1,alphas[3]))
                ],
                indices:vec![0,1,2,1,2,3],
                texture:Some(tex.clone()),
            }
        };
        draw_mesh(&mesh_for(texture,[255;4]));
        // The initial Rust renderer painted the whole cell using only its
        // bottom corner's terrain class. This produced the harsh stairstep
        // blocks visible in the November playtest. Blend alternate original
        // OS materials by the matching vertex weights. Coastline quads keep
        // their dedicated original transition images.
        if mask==0{
            let mut seen=[false;12];
            seen[class]=true;
            for (corner,&other_class) in corners_class.iter().enumerate(){
                let other=other_class as usize;
                if other>=self.class_names.len()||seen[other]{continue}
                seen[other]=true;
                let name=self.class_names[other];
                let (px,py)=corners[corner];
                let tile=terrain.tile[terrain.index(px,py)] as usize;
                let tile=if stored {
                    tile.saturating_sub(self.starts[other])
                }else{tile};
                let tex=self.get(name,tile);
                let alphas=corners_class.map(|c|
                    if c==other_class{255}else{0}
                );
                draw_mesh(&mesh_for(tex,alphas));
            }
        }
        if selected{
            let c=Color::from_rgba(254,224,92,180);
            for (a,b) in [(top,left),(left,bottom),(bottom,right),(right,top)]{
                draw_line(a.x,a.y,b.x,b.y,1.6*zoom,c)
            }
        }
    }
}

struct GdArchive{
    path:PathBuf,
    gd:HashMap<String,GdEntry>,
    od:HashMap<u32,OdObject>,
}
impl GdArchive{
    fn open(path:PathBuf)->Result<Self,String>{
        let mut file=File::open(&path).map_err(|e|e.to_string())?;
        let len=file.metadata().map_err(|e|e.to_string())?.len() as usize;
        let mut header=[0u8;24];
        file.read_exact(&mut header).map_err(|e|e.to_string())?;
        let index_size=gd_index_size(&header).map_err(|e|e.to_string())?;
        if index_size<24||index_size>64*1024*1024||index_size>len{
            return Err(format!("Invalid GD index: {}",path.display()))
        }
        let mut buf=vec![0u8;index_size];
        file.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;
        file.read_exact(&mut buf).map_err(|e|e.to_string())?;
        let index=parse_gd_index(&buf,len).map_err(|e|e.to_string())?;
        let od_path=path.with_extension("OD");
        let od:OdIndex=parse_od(&fs::read(&od_path)
            .map_err(|e|format!("{}: {e}",od_path.display()))?)
            .map_err(|e|e.to_string())?;
        Ok(Self{
            path,
            gd:index.entries.into_iter().map(|e|(e.name.clone(),e)).collect(),
            od:od.objects.into_iter().map(|e|(e.id,e)).collect()
        })
    }
    fn sprite(&self,name:&str,anchor:OdAnchor,flags:u32,speed:i32)->Result<Layer,String>{
        let entry=self.gd.get(name).ok_or_else(||format!("Missing GD sprite {name}"))?;
        if entry.frames==0||entry.frames>128||entry.width>4096||entry.height>4096
            ||entry.data_length>64*1024*1024||entry.data_length==0 {
            return Err(format!("Unsupported sprite shape: {name}"))
        }
        // Use the real animation sequence, rather than truncating every row
        // after the twelfth frame (some original units use longer rows).
        let frame_limit=entry.frames.min(32);
        if entry.width.checked_mul(entry.height)
            .and_then(|v|v.checked_mul(frame_limit))
            .and_then(|v|v.checked_mul(4)).unwrap_or(usize::MAX)>48*1024*1024 {
            return Err("Sprite exceeds texture budget".into())
        }
        let mut file=File::open(&self.path).map_err(|e|e.to_string())?;
        file.seek(SeekFrom::Start(entry.data_offset as u64))
            .map_err(|e|e.to_string())?;
        let mut bytes=vec![0u8;entry.data_length];
        file.read_exact(&mut bytes).map_err(|e|e.to_string())?;
        let rgba=decode_gd_frames(entry,&bytes,Some(frame_limit-1),OpacityMode::Preserve)
            .map_err(|e|e.to_string())?;
        // First-frame 1-bit alpha mask for original-style pixel-precise
        // mouse hit tests, stored at 1/8 the size of a byte alpha plane.
        let width=entry.width as usize;
        let height=entry.height as usize;
        let mut alpha_bits=vec![0u64;(width*height+63)/64];
        if let Some(first)=rgba.first(){
            for (i,pixel) in first.chunks_exact(4).enumerate(){
                if pixel[3]>192{alpha_bits[i/64]|=1u64<<(i%64)}
            }
        }
        let mut textures=Vec::with_capacity(rgba.len());
        for data in rgba{
            let texture=Texture2D::from_rgba8(
                entry.width as u16,entry.height as u16,&data
            );
            texture.set_filter(FilterMode::Nearest);
            textures.push(texture);
        }
        Ok(Layer{textures,offset:vec2(
            (entry.left-anchor.x) as f32,(entry.top-anchor.y) as f32
        ),alpha_bits,width,height,flags,speed})
    }
    fn art(&self,id:u32,action:u8,facing:u8)->Option<SpriteArt>{
        let object=self.od.get(&id)?;
        // The OD kind-0 table stores five actions, each with eight
        // directions: MG (0), ST (1), WO (2), ID (3), CA (4).
        // Building kind 1 has BU in slot 0 and MS in slot 1.
        let candidates=slot_candidates(object.kind,action,facing);
        let slot=candidates.iter().find_map(|&index|
            object.slots.get(index).filter(|slot|!slot.is_empty())
        )?;
        let mut layers=Vec::new();
        for layer in slot.iter().take(6){
            if let Ok(img)=self.sprite(&layer.name,object.anchor,layer.flags,layer.speed){
                layers.push(img);
            }
        }
        if layers.is_empty(){None}else{Some(SpriteArt{layers})}
    }
}
/// Native nature starting animation action, from ODB class/product.
/// Trees MUST show CA0 (a standing tree), rather than MS0 (felled).
/// The first frame of bushes and stone MS0 is their intact stage.
pub fn nature_initial_action(class:i32,product:i32)->u8{
    match class{
        8=>4,        // standing tree (CA0)
        9|13=>0,     // berry bushes and stone (MS0)
        10..=12=>1,  // growing crops / mushrooms (ST0)
        _=>match product{
            0=>1,1=>4,_=>0
        },
    }
}

/// Return a prioritized list of descriptor indices for this object state.
/// Prefer an exact facing; fall back to the same action's direction zero,
/// then to walking in the same facing, never to a random nonempty action.
fn slot_candidates(kind:u32,action:u8,facing:u8)->[usize;4]{
    match kind{
        0=>{
            let direction=(facing&7) as usize;
            let base=(action.min(4) as usize)*8;
            [base+direction,base,direction,0]
        },
        1=>{
            let index=usize::from(action!=0);
            [index,index,0,0]
        },
        _=>[0;4]
    }
}
pub struct Layer {
    textures:Vec<Texture2D>,
    offset:Vec2,
    alpha_bits:Vec<u64>,
    width:usize,
    height:usize,
    flags:u32,
    speed:i32,
}
impl Layer{
    fn hit(&self,anchor:Vec2,zoom:f32,pixel:Vec2)->bool{
        if zoom<=0.0 || self.width==0{return false}
        let local=(pixel-anchor-self.offset*zoom)/zoom;
        let x=local.x.floor() as i32;
        let y=local.y.floor() as i32;
        if x<0||y<0||x>=self.width as i32||y>=self.height as i32{return false}
        let index=y as usize*self.width+x as usize;
        self.alpha_bits.get(index/64).is_some_and(|b|b&(1u64<<(index%64))!=0)
    }
}
/// Independent, stable animation timing per object and OD layer. The
/// first frame does not move until that layer's own ambient interval.
fn ambient_frame(id:u32,layer_index:usize,tick:u64,count:usize,speed:i32,flags:u32)->usize{
    if count<=1{return 0}
    let cycle=90+((id as u64*73+layer_index as u64*31)%180);
    let phase=(tick+id as u64*17+layer_index as u64*47)%cycle;
    let ticks_per_frame=(10/speed.max(1) as u64).max(1);
    let motion=if flags&1!=0{
        count.saturating_sub(1)*2
    }else{count};
    let idx=(phase/ticks_per_frame) as usize;
    if flags&2!=0 && idx>=motion{
        if flags&1!=0{0}else{count-1}
    }else if idx>=motion{
        0
    }else if flags&1!=0 && idx>=count{
        motion.saturating_sub(idx)
    }else{idx.min(count-1)}
}
/// Animation frames are driven by simulation state, never by global wall time.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum SpriteMotion{
    /// Idle units and untouched resources freeze at the initial stage.
    Still,
    /// Active units use a repeating walk/work cycle.
    Loop(u64),
    /// Resource depletion uses the original frames exactly once, at harvest end.
    Finish{elapsed:u64,duration:u64},
}
impl SpriteMotion{
    fn frame(self,count:usize)->usize{
        if count<=1{return 0}
        match self{
            Self::Still=>0,
            Self::Loop(tick)=>((tick/2) as usize)%count,
            Self::Finish{elapsed,duration}=>{
                let duration=duration.max(1);
                let elapsed=elapsed.min(duration-1);
                ((elapsed as u128*count as u128/duration as u128) as usize).min(count-1)
            }
        }
    }
}
pub struct SpriteArt {layers:Vec<Layer>}
impl SpriteArt {
    pub fn hit(&self,anchor:Vec2,zoom:f32,pixel:Vec2)->bool{
        // Every original layer participates in hit testing (including the door).
        self.layers.iter().any(|layer|layer.hit(anchor,zoom,pixel))
    }
    pub fn draw(&self,screen:Vec2,zoom:f32,motion:SpriteMotion,tint:Color,
        entity_id:u32,tick:u64,ambient_enabled:bool,work_active:bool){
        for (index,layer) in self.layers.iter().enumerate(){
            let n=layer.textures.len();
            if n==0{continue}
            let frame=if index==0{
                motion.frame(n)
            }else if layer.flags&0x10!=0{
                0 // door: wait for a real enter/leave event
            }else if layer.flags&0x20!=0{
                if work_active{SpriteMotion::Loop(tick).frame(n)}else{0}
            }else if layer.flags&0x40!=0{
                if ambient_enabled{
                    ambient_frame(entity_id,index,tick,n,layer.speed,layer.flags)
                }else{0}
            }else{0};
            let t=&layer.textures[frame];
            let pos=screen+layer.offset*zoom;
            draw_texture_ex(t,pos.x.floor(),pos.y.floor(),tint,
                DrawTextureParams{
                    dest_size:Some(t.size()*zoom),
                    ..Default::default()
                });
        }
    }
}

pub struct WorldArt {
    archives:Vec<GdArchive>,
    cache:HashMap<(u32,u8,u8),(Option<SpriteArt>,u64)>,
    cache_clock:u64,
}
impl WorldArt{
    pub fn open(base:PathBuf,addon:Option<PathBuf>)->Result<Self,String>{
        let mut archives=vec![GdArchive::open(base)?];
        if let Some(p)=addon {archives.push(GdArchive::open(p)?)}
        Ok(Self{archives,cache:HashMap::new(),cache_clock:0})
    }
    pub fn cache_size(&self)->usize{self.cache.len()}
    pub fn has_slot(&self,oid:u32,action:u8,facing:u8)->bool{
        self.archives.iter().rev().any(|archive|
            archive.od.get(&oid).is_some_and(|obj|obj.kind==0 &&
                obj.slots.get(action as usize*8+(facing&7) as usize)
                    .is_some_and(|slot|!slot.is_empty())))
    }
    pub fn get(&mut self,oid:u32,action:u8,facing:u8)->Option<&SpriteArt>{
        let key=(oid,action.min(4),facing&7);
        self.cache_clock=self.cache_clock.wrapping_add(1);
        if !self.cache.contains_key(&key){
            // LRU eviction preserves nearby/hot sprite directions instead of
            // clearing the entire GPU cache when the limit is reached.
            if self.cache.len()>=512{
                if let Some(old)=self.cache.iter().min_by_key(|(_,(_,stamp))|*stamp)
                    .map(|(key,_)|*key){
                    self.cache.remove(&old);
                }
            }
            let image=self.archives.iter().rev()
                .find_map(|archive|archive.art(oid,key.1,key.2));
            self.cache.insert(key,(image,self.cache_clock));
        }
        if let Some((_,stamp))=self.cache.get_mut(&key){
            *stamp=self.cache_clock;
        }
        self.cache.get(&key).and_then(|(art,_)|art.as_ref())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mask_and_ambient_animation_regressions(){
        let layer=Layer{
            textures:Vec::new(),offset:vec2(-2.0,-3.0),
            alpha_bits:vec![1u64<<5],width:4,height:4,flags:0,speed:7
        };
        assert!(layer.hit(vec2(100.0,100.0),1.0,vec2(99.0,98.0)));
        assert!(!layer.hit(vec2(100.0,100.0),1.0,vec2(98.0,98.0)));
        assert!(!layer.hit(vec2(100.0,100.0),1.0,vec2(200.0,200.0)));
        for tick in 0..100{
            assert!(ambient_frame(100,1,tick,12,7,0x42)<12);
        }
    }
    #[test]
    fn nature_uses_original_intact_resource_animation_slot(){
        assert_eq!(nature_initial_action(8,1),4); // standing tree CA0
        assert_eq!(nature_initial_action(9,2),0); // berries MS0
        assert_eq!(nature_initial_action(13,2),0); // rock MS0
        assert_eq!(nature_initial_action(10,0),1); // crop ST0
        assert_eq!(nature_initial_action(0,1),4); // ODB product fallback
        assert_eq!(nature_initial_action(0,-1),0);
    }
    #[test]
    fn original_od_action_direction_indexing(){
        // Eight facing rows per action in the native OD kind-0 table.
        assert_eq!(slot_candidates(0,0,0),[0,0,0,0]);
        assert_eq!(slot_candidates(0,0,6),[6,0,6,0]);
        assert_eq!(slot_candidates(0,2,5),[21,16,5,0]);
        assert_eq!(slot_candidates(0,4,0),[32,32,0,0]); // standing tree
        assert_eq!(slot_candidates(0,4,7),[39,32,7,0]);
        assert_eq!(slot_candidates(1,0,3),[0,0,0,0]); // construction BU
        assert_eq!(slot_candidates(1,1,3),[1,1,0,0]); // completed MS
        assert_eq!(slot_candidates(2,4,7),[0,0,0,0]);
    }
    #[test]
    fn idle_resource_never_cycles_through_death_frames(){
        for count in 1..=16{
            assert_eq!(SpriteMotion::Still.frame(count),0);
        }
        assert_eq!(SpriteMotion::Loop(0).frame(4),0);
        assert_eq!(SpriteMotion::Loop(6).frame(4),3);
        assert_eq!(SpriteMotion::Finish{elapsed:0,duration:12}.frame(6),0);
        assert_eq!(SpriteMotion::Finish{elapsed:11,duration:12}.frame(6),5);
        assert_eq!(SpriteMotion::Finish{elapsed:100,duration:12}.frame(6),5);
    }
    #[test]
    fn isometric_projection_respects_source_coordinates(){
        assert_eq!(project(1.0,0.0,128),vec2(-32.0,22.0));
        assert_eq!(project(0.0,1.0,128),vec2(32.0,22.0));
        assert_eq!(unproject(vec2(32.0,22.0)),vec2(0.0,1.0));
        assert_eq!(MASKS.len(),14);
    }
}
