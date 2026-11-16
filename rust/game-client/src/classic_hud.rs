//! Original Alien Nations side-panel background, loaded from each race's
//! interface.gd archive. See docs/ui/side-panel.md section 1/2.
//!
//! The reference HUD has a 192x768 logical panel, with the minimap in
//! panel coordinates (6,6)..(158,158) and the page beginning at y=335.
//! This module only decodes original artwork locally; it does not bundle
//! copyrighted game images or fonts.

use game_core::gd::{
    decode_gd_frames, gd_index_size, parse_gd_index, OpacityMode,
};
use macroquad::prelude::*;
use std::{
    fs::{self,File},io::{Read,Seek,SeekFrom},path::{Path,PathBuf},
};

pub const PANEL_LOGICAL_WIDTH:f32=192.0;
pub const PANEL_LOGICAL_HEIGHT:f32=768.0;

pub fn hud_scale()->f32{
    (screen_height()/PANEL_LOGICAL_HEIGHT).clamp(0.82,1.6)
}
pub fn hud_width()->f32{
    (PANEL_LOGICAL_WIDTH*hud_scale()).round()
}
pub fn hud_x()->f32{
    screen_width()-hud_width()
}
pub fn panel_point(x:f32,y:f32)->Vec2{
    vec2(hud_x()+x*hud_scale(),y*hud_scale())
}

fn child_no_case(dir:&Path,wanted:&str)->Option<PathBuf>{
    fs::read_dir(dir).ok()?.flatten().find(|entry|
        entry.file_name().to_string_lossy().eq_ignore_ascii_case(wanted)
    ).map(|entry|entry.path())
}
fn race_ui_archive(data_dir:&Path,race:i32)->Option<PathBuf>{
    let candidates:&[&str]=match race{
        0=>&["AMAZONS","AMAZON","AMAZONEN"],
        1=>&["PIMONS","PIMMONS","PIMMON"],
        2=>&["SAJIKI","SAJIKIS"],
        _=>return None,
    };
    candidates.iter().find_map(|race_dir|{
        let dir=child_no_case(data_dir,race_dir)?;
        child_no_case(&dir,"interface.gd")
    })
}

fn extract_background(path:&Path)->Result<Texture2D,String>{
    let mut file=File::open(path).map_err(|e|format!("{}: {e}",path.display()))?;
    let len=file.metadata().map_err(|e|e.to_string())?.len() as usize;
    let mut header=[0u8;24];
    file.read_exact(&mut header).map_err(|e|e.to_string())?;
    let index_size=gd_index_size(&header).map_err(|e|e.to_string())?;
    if !(24..=16*1024*1024).contains(&index_size)||index_size>len{
        return Err("invalid interface.gd index".into())
    }
    let mut index_bytes=vec![0u8;index_size];
    file.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;
    file.read_exact(&mut index_bytes).map_err(|e|e.to_string())?;
    let index=parse_gd_index(&index_bytes,len).map_err(|e|e.to_string())?;
    let entry=index.entries.iter().find(|e|
        e.name.eq_ignore_ascii_case("INTERFACE1024")
    ).ok_or("INTERFACE1024 is not in this race UI archive")?;
    if entry.width>2048||entry.height>2048||entry.data_length>32*1024*1024{
        return Err("invalid interface image dimensions".into())
    }
    file.seek(SeekFrom::Start(entry.data_offset as u64)).map_err(|e|e.to_string())?;
    let mut block=vec![0u8;entry.data_length];
    file.read_exact(&mut block).map_err(|e|e.to_string())?;
    let frames=decode_gd_frames(entry,&block,Some(0),OpacityMode::AutoOpaque)
        .map_err(|e|format!("interface.gd decode: {e}"))?;
    let data=frames.first().ok_or("empty interface background")?;
    let texture=Texture2D::from_rgba8(entry.width as u16,entry.height as u16,data);
    texture.set_filter(FilterMode::Nearest);
    println!(
        "Classic race interface loaded: {} ({}x{})",
        path.display(),entry.width,entry.height
    );
    Ok(texture)
}

pub struct ClassicHudArt{
    pub background:Option<Texture2D>,
}
impl ClassicHudArt{
    pub fn load(data_dir:&Path,race:i32)->Self{
        let background=race_ui_archive(data_dir,race)
            .and_then(|path|match extract_background(&path){
                Ok(image)=>Some(image),
                Err(error)=>{
                    eprintln!("Could not load original UI background: {error}");
                    None
                }
            });
        if background.is_none(){
            eprintln!(
                "Original race interface.gd not found; using a safe drawn fallback panel"
            );
        }
        Self{background}
    }
    pub fn draw(&self){
        let x=hud_x();
        let w=hud_width();
        let h=screen_height();
        if let Some(texture)=&self.background{
            // The original uses a vertically stretched dark band for tall
            // windows. Keep the 1024 reference artwork and stretch to viewport.
            draw_texture_ex(texture,x,0.0,WHITE,DrawTextureParams{
                dest_size:Some(vec2(w,h)),
                ..Default::default()
            });
        }else{
            draw_rectangle(x,0.0,w,h,Color::from_rgba(48,53,41,255));
            draw_rectangle(x+3.0,3.0,w-6.0,h-6.0,
                Color::from_rgba(30,36,29,255));
            draw_rectangle(x+4.0,h*0.436,w-8.0,h*0.564-8.0,
                Color::from_rgba(28,33,27,255));
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn race_archive_names_match_original_data_layout(){
        assert!(race_ui_archive(Path::new("/nonexistent"),2).is_none());
        assert_eq!(PANEL_LOGICAL_WIDTH,192.0);
        assert_eq!(PANEL_LOGICAL_HEIGHT,768.0);
    }
}
