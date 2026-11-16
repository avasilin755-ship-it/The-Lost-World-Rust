//! Original base-game seeded terrain reconstruction (MSVC rand, hills, lakes, light).
//! Source: src/world/terrain.ts and docs/formats/terrain-gen.md.
//! The Gold Edition island generator is different; stored Gold terrain can be
//! imported with Terrain::from_stored. This is not the Gold free-play generator.

use std::fmt;
pub const CLASS_WATER:u8=11;
pub const CLASS_PATH:u8=10;
pub const CLASS_SETS:[&str;12]=[
    "wiese","gelbgras","schnee","rock","waldboden","waldboden",
    "gruenwald","waldboden","gelbgras","gruenwald","weg","wasser1"
];
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum TerrainError{Invalid(&'static str)}
impl fmt::Display for TerrainError{
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{write!(f,"{self:?}")}
}
impl std::error::Error for TerrainError{}

#[derive(Debug,Clone,Copy)]
pub struct MsRand{pub state:u32}
impl MsRand{
    pub fn new(seed:u32)->Self{Self{state:seed}}
    pub fn next_rand(&mut self)->u32{
        self.state=self.state.wrapping_mul(214013).wrapping_add(2531011);
        (self.state>>16)&0x7fff
    }
}
#[derive(Debug,Clone)]
pub struct Terrain{
    pub size_bits:u32,pub size:usize,pub height:Vec<u16>,
    pub class:Vec<u8>,pub tile:Vec<u8>,pub light:Vec<u8>,pub rng_state:u32,
}
impl Terrain{
    pub fn index(&self,x:i32,y:i32)->usize{
        let mask=self.size as i32-1;
        ((y&mask) as usize)*self.size+(x&mask) as usize
    }
    pub fn at(&self,x:i32,y:i32)->u8{self.class[self.index(x,y)]}
    pub fn water(&self,x:i32,y:i32)->bool{self.at(x,y)==CLASS_WATER}

    /// Stored Gold missions save heights and global tile IDs; caller supplies
    /// the Gold tile-index to class mapping from loaded .os set counts.
    pub fn from_stored(
        size_bits:u32,height:Vec<u16>,tile:Vec<u8>,class_lookup:&[u8;256],
    )->Result<Self,TerrainError>{
        if !matches!(size_bits,7|8|9){return Err(TerrainError::Invalid("invalid size bits"))}
        let size=1usize<<size_bits;
        if height.len()!=size*size||tile.len()!=size*size{
            return Err(TerrainError::Invalid("incorrect terrain size"));
        }
        let class=tile.iter().map(|&t|class_lookup[t as usize]).collect();
        let mut this=Self{size_bits,size,height,class,tile,light:vec![0;size*size],rng_state:0};
        this.compute_light();
        Ok(this)
    }
    pub fn compute_light(&mut self){
        for y in 0..self.size as i32{
            for x in 0..self.size as i32{
                let d1=i32::from(self.height[self.index(x-1,y)])-
                    i32::from(self.height[self.index(x+1,y)]);
                let d2=i32::from(self.height[self.index(x,y+1)])-
                    i32::from(self.height[self.index(x,y-1)]);
                let i=self.index(x,y);
                self.light[i]=light_value(d1,d2);
            }
        }
    }
}

pub fn light_value(d1:i32,d2:i32)->u8{
    let a=f64::from(d1); let b=f64::from(d2);
    let s1=(a*a+1024.0).sqrt();
    let p=(-32.0/s1) as f32;
    let u=a/s1;
    let s2=(b*b+1024.0).sqrt();
    let q=(-32.0/s2) as f32;
    let ny=(-(f64::from(q)*u)) as f32;
    let nz=f64::from(q)*f64::from(p);
    let sun_d=1.0/f64::from(2.0f64.sqrt() as f32);
    let sun_f=sun_d as f32;
    let v=((nz*sun_d+0.0)+f64::from(sun_f)*f64::from(ny))*320.0-64.0;
    (v.trunc() as i32).clamp(128,239) as u8
}

fn index(size:usize,x:i32,y:i32)->usize{
    let mask=size as i32-1;
    ((y&mask) as usize)*size+(x&mask) as usize
}
fn lake_level(size:usize,height:&[u16],cls:&[u8],x:i32,y:i32,r:i32)->u16{
    let (mut lo,mut hi)=(0x7fffu16,0u16);
    for dy in -r..=r{
        for dx in -r..=r{
            let i=index(size,x+dx,y+dy);
            let h=height[i];
            if h<lo{lo=h}if h>hi{hi=h}
            if h>250||cls[i]==CLASS_WATER{return 0}
        }
    }
    if hi-lo>20{0}else{lo}
}
pub fn generate_base(seed:u32,bits:u32,pick:&[[u8;256];12])->Result<Terrain,TerrainError>{
    if !matches!(bits,7|8|9){return Err(TerrainError::Invalid("size bits must be 7..9"))}
    let n=1usize<<bits;
    let mask=n as i32-1;
    let count=n*n;
    let mut rand=MsRand::new(seed);
    let mut height=vec![0u16;count];
    let mut cls=vec![0u8;count];
    for (i,c) in cls.iter_mut().enumerate(){
        *c=((((i>>bits)&i)&1)|8) as u8;
    }
    for _ in 0..((n>>4)*(n>>4)){
        let mut x=(rand.next_rand() as i32)&mask;
        let mut y=(rand.next_rand() as i32)&mask;
        let steps=(rand.next_rand()&7) as usize;
        let mut radius=(rand.next_rand()&31) as i32;
        let variant=cls[index(n,x,y)]&1;
        for _ in 0..steps{
            let outer=6*radius;
            for dy in -outer..=outer{
                for dx in -outer..=outer{
                    let d2=dx*dx+dy*dy;
                    let v=if d2<64{
                        2*radius-((d2*radius)>>6)
                    }else{(radius<<6)/d2};
                    if v==0{continue}
                    let i=index(n,x+dx,y+dy);
                    height[i]=height[i].wrapping_add(((v*4)/5) as u16);
                    let h=height[i];
                    let band=if h>250{2}else if h>200{4}else if h>130{6}else{0};
                    if band!=(cls[i]&0xfe){cls[i]=band+variant}
                }
            }
            let rr=radius|1;
            x+=rand.next_rand() as i32%rr-(radius>>1);
            y+=rand.next_rand() as i32%rr-(radius>>1);
            radius=(radius-15+(rand.next_rand()%31) as i32).clamp(0,31);
        }
    }

    for _ in 0..((n>>6)*(n>>6)){
        let rad=(rand.next_rand()%3+2) as i32;
        let r=rad+1;
        let mut site=None;
        // Original rerolls until the candidate is suitable. A guard prevents
        // pathological inputs from hanging the application forever.
        for _ in 0..100_000 {
            let x=(rand.next_rand() as i32)&mask;
            let y=(rand.next_rand() as i32)&mask;
            let level=lake_level(n,&height,&cls,x,y,r);
            if level!=0 {site=Some((x,y,level));break}
        }
        let Some((x,y,level))=site else{
            return Err(TerrainError::Invalid("no valid lake location"));
        };
        for dy in -r..=r{
            for dx in -r..=r{
                let d2=dx*dx+dy*dy;
                if d2>r*r{continue}
                let i=index(n,x+dx,y+dy);
                height[i]=level;
                if d2<=rad*rad&&((dx!=rad&&dy!=rad)||(rand.next_rand()&1)!=0){
                    cls[i]=CLASS_WATER
                }
            }
        }
        let e=rad+5;
        for dy in -e..=e{
            for dx in -e..=e{
                let cx=x+dx;let cy=y+dy;
                let i=index(n,cx,cy);
                if cls[i]==CLASS_WATER{continue}
                let mut shore=false;
                for ay in -1..=1{
                    for ax in -1..=1{
                        if (ax!=0||ay!=0)&&cls[index(n,cx+ax,cy+ay)]==CLASS_WATER{
                            shore=true;
                            break
                        }
                    }
                    if shore{break}
                }
                if shore{cls[i]=CLASS_PATH}
            }
        }
    }

    let mut tile=vec![0u8;count];
    for i in 0..count{
        let t=(rand.next_rand()&255) as usize;
        tile[i]=pick[cls[i] as usize][t];
    }
    let mut terrain=Terrain{
        size_bits:bits,size:n,height,class:cls,tile,
        light:vec![0u8;count],rng_state:rand.state
    };
    terrain.compute_light();
    Ok(terrain)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn microsoft_rand_known_sequence(){
        let mut r=MsRand::new(1);
        assert_eq!(r.next_rand(),41);
        assert_eq!(r.next_rand(),18467);
        assert_eq!(r.next_rand(),6334);
    }
    #[test]
    fn flat_light_and_slope_examples(){
        assert_eq!(light_value(0,0),162);
        assert_eq!(light_value(8,0),210);
        assert_eq!(light_value(-8,0),128);
    }
    #[test]
    fn generate_reproducible_original_base_map(){
        let picks=[[0u8;256];12];
        let a=generate_base(1234,7,&picks).unwrap();
        let b=generate_base(1234,7,&picks).unwrap();
        assert_eq!(a.height,b.height);
        assert_eq!(a.class,b.class);
        assert_eq!(a.rng_state,b.rng_state);
        assert_eq!(a.height.len(),128*128);
    }
}
