//! Original Alien Nations mission (MDB) and map (DVM) readers.
//! Spec and TS parity reference: docs/formats/mdb.md, dvm.md, addon.md and src/formats/mission.ts.

use std::fmt;
pub const MDB_RECORD_SIZE: usize = 3864;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionError { Truncated, Invalid(&'static str) }
impl fmt::Display for MissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for MissionError {}

fn slice(b: &[u8], off: usize, len: usize) -> Result<&[u8], MissionError> {
    b.get(off..off.checked_add(len).ok_or(MissionError::Invalid("offset overflow"))?)
        .ok_or(MissionError::Truncated)
}
fn i32_at(b: &[u8], o: usize) -> Result<i32, MissionError> {
    Ok(i32::from_le_bytes(slice(b,o,4)?.try_into().unwrap()))
}
fn u32_at(b: &[u8], o: usize) -> Result<u32, MissionError> {
    Ok(u32::from_le_bytes(slice(b,o,4)?.try_into().unwrap()))
}
fn u16_at(b: &[u8], o: usize) -> Result<u16, MissionError> {
    Ok(u16::from_le_bytes(slice(b,o,2)?.try_into().unwrap()))
}
fn cp1252(b: &[u8]) -> String {
    const HIGH: [u32;32] = [
        0x20ac,0x81,0x201a,0x192,0x201e,0x2026,0x2020,0x2021,
        0x2c6,0x2030,0x160,0x2039,0x152,0x8d,0x17d,0x8f,
        0x90,0x2018,0x2019,0x201c,0x201d,0x2022,0x2013,0x2014,
        0x2dc,0x2122,0x161,0x203a,0x153,0x9d,0x17e,0x178,
    ];
    b.iter().take_while(|&&x|x!=0).map(|&x|{
        char::from_u32(if (0x80..=0x9f).contains(&x){HIGH[(x-0x80) as usize]}else{x as u32})
            .unwrap_or('\u{fffd}')
    }).collect::<String>().trim_end().to_owned()
}
#[derive(Debug, Clone)]
pub struct RaceSetup { pub present: bool, pub computer: bool, pub gold: i32, pub goods: [i32;9] }
#[derive(Debug, Clone)]
pub struct Goal { pub kind:i32,pub defeat:bool,pub p1:i32,pub p2:i32,pub p3:i32 }
#[derive(Debug, Clone)]
pub struct Mission {
    pub order:i32,pub title:String,pub briefing:String,pub map_file:String,
    pub human_race:i32,pub races:[RaceSetup;3],pub goals:Vec<Goal>,
    pub seed:i32,pub size_bits:u32,pub addon:bool,
}
pub fn parse_mdb(bytes:&[u8])->Result<Vec<Mission>,MissionError>{
    if bytes.is_empty() || bytes.len()%MDB_RECORD_SIZE!=0 || bytes.len()/MDB_RECORD_SIZE>1024 {
        return Err(MissionError::Invalid("invalid MDB record count"));
    }
    let mut result=Vec::new();
    for b in bytes.chunks_exact(MDB_RECORD_SIZE) {
        let mut human_race=-1;
        let mut races: [RaceSetup;3] = std::array::from_fn(|_|RaceSetup{
            present:false,computer:false,gold:0,goods:[0;9]
        });
        for (race,dest) in races.iter_mut().enumerate(){
            let p=0xb1c+race*0x154;
            dest.present=i32_at(b,p)?!=0;
            dest.computer=i32_at(b,p+4)?!=0;
            dest.gold=i32_at(b,p+0xc)?;
            for (good,stock) in dest.goods.iter_mut().enumerate(){
                *stock=i32_at(b,p+0x1c+good*8)?;
            }
            if dest.present && !dest.computer && human_race<0 {human_race=race as i32}
        }
        let mut goals=Vec::new();
        for i in 0..10 {
            let p=0xa54+i*20;
            let kind=i32_at(b,p)?;
            if kind!=0 {goals.push(Goal {
                kind,defeat:i32_at(b,p+4)?!=0,p1:i32_at(b,p+8)?,
                p2:i32_at(b,p+12)?,p3:i32_at(b,p+16)?
            });}
        }
        let bits=u32_at(b,0xa4c)?;
        if !matches!(bits,7|8|9){return Err(MissionError::Invalid("invalid map size"))}
        result.push(Mission {
            order:i32_at(b,0)?,title:cp1252(slice(b,4,64)?),
            briefing:cp1252(slice(b,0x44,2500)?),
            map_file:cp1252(slice(b,0xa08,64)?),human_race,races,goals,
            seed:i32_at(b,0xa48)?,size_bits:bits,addon:i32_at(b,0xa50)?>=1
        });
    }
    result.sort_by_key(|m|m.order);
    Ok(result)
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct MapObject {
    pub kind:i32,pub group:i32,pub object_type:i32,pub owner:i32,
    pub x:i32,pub y:i32,pub z:i32
}
#[derive(Debug,Clone)]
pub struct StoredTerrain {pub size_bits:u32,pub height:Vec<u16>,pub tile:Vec<u8>}
#[derive(Debug,Clone)]
pub struct GameMap {
    pub objects:Vec<MapObject>,pub terrain:Option<StoredTerrain>,
    pub camera:(i32,i32),pub starts:Vec<(i32,i32)>
}
pub fn parse_dvm(bytes:&[u8],stored_bits:Option<u32>,addon_cargo:bool)->Result<GameMap,MissionError>{
    if slice(bytes,0,4)?!=b"SMVD"{return Err(MissionError::Invalid("invalid DVM magic"))}
    if u32_at(bytes,4)?!=100{return Err(MissionError::Invalid("invalid DVM version"))}
    let count=u32_at(bytes,8)? as usize;
    if count>250_000{return Err(MissionError::Invalid("unreasonable DVM object count"))}
    let mut pos=12usize;
    let terrain=match stored_bits{
        None=>None,
        Some(bits@(7|8|9))=>{
            let size=(1usize<<bits).pow(2);
            slice(bytes,pos,size*3)?;
            let mut height=Vec::with_capacity(size);
            let mut tile=Vec::with_capacity(size);
            for _ in 0..size{
                height.push(u16_at(bytes,pos)?);
                tile.push(slice(bytes,pos+2,1)?[0]);
                pos+=3;
            }
            Some(StoredTerrain{size_bits:bits,height,tile})
        }
        _=>return Err(MissionError::Invalid("invalid stored terrain dimensions"))
    };
    let mut objects=Vec::with_capacity(count);
    for _ in 0..count {
        slice(bytes,pos,28)?;
        let obj=MapObject{
            kind:i32_at(bytes,pos)?,group:i32_at(bytes,pos+4)?,
            object_type:i32_at(bytes,pos+8)?,owner:i32_at(bytes,pos+12)?,
            x:i32_at(bytes,pos+16)?,y:i32_at(bytes,pos+20)?,
            z:i32_at(bytes,pos+24)?
        };
        pos+=28;
        if addon_cargo && obj.kind==0 && obj.group!=6 && obj.object_type==67 {
            let n=u32_at(bytes,pos)? as usize;
            if n>100_000{return Err(MissionError::Invalid("ship cargo too long"))}
            pos=pos.checked_add(4+n*4).ok_or(MissionError::Invalid("cargo overflow"))?;
            if pos>bytes.len(){return Err(MissionError::Truncated)}
        }
        objects.push(obj);
    }
    let mut starts=Vec::new();
    for k in 0..5{
        let n=u32_at(bytes,pos)? as usize;
        pos+=4;
        if n>100_000{return Err(MissionError::Invalid("point list too long"))}
        let length=n.checked_mul(12).ok_or(MissionError::Invalid("points overflow"))?;
        slice(bytes,pos,length)?;
        if k==4 {
            for i in 0..n {
                starts.push((i32_at(bytes,pos+i*12)?,i32_at(bytes,pos+i*12+4)?));
            }
        }
        pos+=length;
    }
    let camera=(i32_at(bytes,pos)?,i32_at(bytes,pos+4)?);
    Ok(GameMap{objects,terrain,camera,starts})
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn mdb_order_and_races(){
        let mut m=vec![0u8;MDB_RECORD_SIZE*2];
        m[0..4].copy_from_slice(&2i32.to_le_bytes());
        m[4..8].copy_from_slice(b"Two\0");
        m[0xa48..0xa4c].copy_from_slice(&25i32.to_le_bytes());
        m[0xa4c..0xa50].copy_from_slice(&7u32.to_le_bytes());
        m[0xb1c..0xb20].copy_from_slice(&1i32.to_le_bytes());
        let o=MDB_RECORD_SIZE;
        m[o..o+4].copy_from_slice(&1i32.to_le_bytes());
        m[o+4..o+8].copy_from_slice(b"One\0");
        m[o+0xa48..o+0xa4c].copy_from_slice(&(-2i32).to_le_bytes());
        m[o+0xa4c..o+0xa50].copy_from_slice(&8u32.to_le_bytes());
        m[o+0xa50..o+0xa54].copy_from_slice(&1i32.to_le_bytes());
        let r=parse_mdb(&m).unwrap();
        assert_eq!(r[0].title,"One");
        assert!(r[0].addon);
        assert_eq!(r[1].human_race,0);
        assert_eq!(r[1].seed,25);
    }
    #[test]
    fn dvm_object_and_start_list(){
        let mut b=b"SMVD".to_vec();
        b.extend_from_slice(&100u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        for v in [1i32,2,37,0,11,12,0]{b.extend_from_slice(&v.to_le_bytes())}
        for k in 0..5 {
            b.extend_from_slice(&(if k==4 {1u32}else{0u32}).to_le_bytes());
            if k==4 {for v in [5i32,6,0]{b.extend_from_slice(&v.to_le_bytes())}}
        }
        for v in [7i32,8]{b.extend_from_slice(&v.to_le_bytes())}
        let m=parse_dvm(&b,None,false).unwrap();
        assert_eq!(m.objects[0].object_type,37);
        assert_eq!(m.starts,vec![(5,6)]);
        assert_eq!(m.camera,(7,8));
        assert!(parse_dvm(b"xx",None,false).is_err());
    }
}
