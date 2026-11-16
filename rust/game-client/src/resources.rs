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
            for tile in tiles.tiles{
                let texture=Texture2D::from_rgba8(
                    tile.width as u16,tile.height as u16,&tile.rgba
                );
                texture.set_filter(FilterMode::Nearest);
                textures.push(texture);
            }
            sets.insert(name.to_owned(),TileSet{textures,pick:tiles.pick});
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
        let texture=if mask>0&&mask<15{
            let n=MASKS.iter().position(|&m|m==mask).unwrap_or(0);
            self.get(&format!("transition{}",frame+1),n)
        }else if mask==15{
            if class>=self.water_class as usize+1||class==9&&self.water_class==9{
                self.get("wasser1",variant)
            }else{
                self.get(&format!("wasser{}",frame+1),variant)
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
        let shade=|x:i32,y:i32|{
            let value=terrain.light[terrain.index(x,y)];
            Color::from_rgba(value,value,value,255)
        };
        let mesh=Mesh{
            vertices:vec![
                Vertex::new(top.x,top.y,0.0,1.0,1.0,shade(x,y)),
                Vertex::new(left.x,left.y,0.0,0.0,1.0,shade(x+1,y)),
                Vertex::new(right.x,right.y,0.0,1.0,0.0,shade(x,y+1)),
                Vertex::new(bottom.x,bottom.y,0.0,0.0,0.0,shade(x+1,y+1))
            ],
            indices:vec![0,1,2,1,2,3],
            texture:Some(texture.clone()),
        };
        draw_mesh(&mesh);
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
    fn sprite(&self,name:&str,anchor:OdAnchor)->Result<Layer,String>{
        let entry=self.gd.get(name).ok_or_else(||format!("Missing GD sprite {name}"))?;
        if entry.frames==0||entry.frames>128||entry.width>4096||entry.height>4096
            ||entry.data_length>64*1024*1024||entry.data_length==0 {
            return Err(format!("Unsupported sprite shape: {name}"))
        }
        let frame_limit=entry.frames.min(12);
        if entry.width.checked_mul(entry.height)
            .and_then(|v|v.checked_mul(frame_limit))
            .and_then(|v|v.checked_mul(4)).unwrap_or(usize::MAX)>32*1024*1024 {
            return Err("Sprite exceeds texture budget".into())
        }
        let mut file=File::open(&self.path).map_err(|e|e.to_string())?;
        file.seek(SeekFrom::Start(entry.data_offset as u64))
            .map_err(|e|e.to_string())?;
        let mut bytes=vec![0u8;entry.data_length];
        file.read_exact(&mut bytes).map_err(|e|e.to_string())?;
        let rgba=decode_gd_frames(entry,&bytes,Some(frame_limit-1),OpacityMode::Preserve)
            .map_err(|e|e.to_string())?;
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
        )})
    }
    fn art(&self,id:u32)->Option<SpriteArt>{
        let object=self.od.get(&id)?;
        let preferred=if object.kind==1{1}else{0};
        let slot=object.slots.get(preferred)
            .filter(|s|!s.is_empty())
            .or_else(||object.slots.iter().find(|s|!s.is_empty()))?;
        let mut layers=Vec::new();
        for layer in slot.iter().take(6){
            if let Ok(img)=self.sprite(&layer.name,object.anchor){
                layers.push(img);
            }
        }
        if layers.is_empty(){None}else{Some(SpriteArt{layers})}
    }
}
pub struct Layer {
    textures:Vec<Texture2D>,
    offset:Vec2,
}
pub struct SpriteArt {layers:Vec<Layer>}
impl SpriteArt {
    pub fn draw(&self,screen:Vec2,zoom:f32,seconds:f64){
        for layer in &self.layers{
            let n=layer.textures.len();
            if n==0{continue}
            let t=&layer.textures[(seconds*8.0) as usize%n];
            let pos=screen+layer.offset*zoom;
            draw_texture_ex(t,pos.x.floor(),pos.y.floor(),WHITE,
                DrawTextureParams{
                    dest_size:Some(t.size()*zoom),
                    ..Default::default()
                });
        }
    }
}

pub struct WorldArt {
    archives:Vec<GdArchive>,
    cache:HashMap<u32,Option<SpriteArt>>,
}
impl WorldArt{
    pub fn open(base:PathBuf,addon:Option<PathBuf>)->Result<Self,String>{
        let mut archives=vec![GdArchive::open(base)?];
        if let Some(p)=addon {archives.push(GdArchive::open(p)?)}
        Ok(Self{archives,cache:HashMap::new()})
    }
    pub fn get(&mut self,oid:u32)->Option<&SpriteArt>{
        if !self.cache.contains_key(&oid){
            let image=self.archives.iter()
                .find_map(|archive|archive.art(oid));
            if self.cache.len()>=300{
                self.cache.clear();
            }
            self.cache.insert(oid,image);
        }
        self.cache.get(&oid).and_then(|s|s.as_ref())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isometric_projection_respects_source_coordinates(){
        assert_eq!(project(1.0,0.0,128),vec2(-32.0,22.0));
        assert_eq!(project(0.0,1.0,128),vec2(32.0,22.0));
        assert_eq!(unproject(vec2(32.0,22.0)),vec2(0.0,1.0));
        assert_eq!(MASKS.len(),14);
    }
}
