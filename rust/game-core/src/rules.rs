//! ODB object rules (building footprints, costs, residents and unit speeds).
//! Matches the original 396-byte records in src/formats/odb.ts and docs/formats/odb.md.

use std::collections::HashMap;
use std::fmt;
pub const ODB_RECORD_SIZE:usize=0x18c;
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum RuleError{ Invalid(&'static str) }
impl fmt::Display for RuleError{
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { write!(f,"{self:?}") }
}
impl std::error::Error for RuleError{}
fn num(b:&[u8],o:usize)->i32{
    i32::from_le_bytes(b[o..o+4].try_into().unwrap())
}
#[derive(Debug,Clone)]
pub struct ObjectRule{
    pub id:u32,pub group:i32,pub kind:i32,pub object_type:i32,pub name:String,
    pub size_x:i32,pub size_y:i32,pub door_dx:i32,pub door_dy:i32,
    pub hp:i32,pub radius:i32,pub residents:i32,pub flags:u32,
    pub class:i32,pub step_ticks:[i32;4],pub money:i32,
    pub operator:i32,pub invention:i32,pub made_at:i32,
    pub cost:[i32;9],pub product:i32,pub amount:i32,pub weapon:i32
}
pub type Rules=HashMap<u32,ObjectRule>;
pub fn rule_id(group:i32,object_type:i32)->u32{
    ((group as u32)<<16)|(object_type as u32 & 0xffff)
}
fn name(b:&[u8])->String{
    b.iter().take_while(|&&c|c!=0)
        .map(|&c|char::from_u32(c as u32).unwrap_or('?'))
        .collect::<String>().trim_end().to_owned()
}
pub fn parse_odb(bytes:&[u8])->Result<Rules,RuleError>{
    if bytes.is_empty()||bytes.len()%ODB_RECORD_SIZE!=0||bytes.len()/ODB_RECORD_SIZE>10000{
        return Err(RuleError::Invalid("invalid ODB record count"));
    }
    let mut rules=HashMap::new();
    for b in bytes.chunks_exact(ODB_RECORD_SIZE){
        let group=num(b,0xa0);
        let object_type=num(b,0xa8);
        let mut costs=[0;9];
        for (i,cost) in costs.iter_mut().enumerate(){*cost=num(b,0xf0+i*4)}
        let mut step_ticks=[0;4];
        for (i,time) in step_ticks.iter_mut().enumerate(){*time=num(b,0xc4+i*4)}
        let id=rule_id(group,object_type);
        let rule=ObjectRule{
            id,group,kind:num(b,0xa4),object_type,name:name(&b[0x14c..0x18c]),
            size_y:num(b,0xc),size_x:num(b,0x10),
            door_dx:num(b,0x94),door_dy:num(b,0x98),
            hp:num(b,0xac),radius:num(b,0xb0),residents:num(b,0xb4),
            flags:num(b,0xb8) as u32,class:num(b,0xc0),step_ticks,
            money:num(b,0xd4),operator:num(b,0xd8),invention:num(b,0xe4),
            made_at:num(b,0xe8),cost:costs,product:num(b,0x11c),
            amount:num(b,0x120),weapon:num(b,0x144),
        };
        rules.insert(id,rule);
    }
    Ok(rules)
}
pub fn merge_rules(base:&mut Rules,addon:Rules){
    base.extend(addon);
}
#[cfg(test)]
mod tests{
    use super::*;
    fn fixture()->Vec<u8>{
        let mut b=vec![0u8;ODB_RECORD_SIZE];
        b[0xa0..0xa4].copy_from_slice(&2i32.to_le_bytes());
        b[0xa4..0xa8].copy_from_slice(&1i32.to_le_bytes());
        b[0xa8..0xac].copy_from_slice(&37i32.to_le_bytes());
        b[0xc..0x10].copy_from_slice(&3i32.to_le_bytes());
        b[0x10..0x14].copy_from_slice(&4i32.to_le_bytes());
        b[0xf4..0xf8].copy_from_slice(&10i32.to_le_bytes());
        b[0x14c..0x151].copy_from_slice(b"House");
        b
    }
    #[test]
    fn decodes_footprints_costs_and_names(){
        let r=parse_odb(&fixture()).unwrap();
        let h=r.get(&rule_id(2,37)).unwrap();
        assert_eq!((h.size_x,h.size_y),(4,3));
        assert_eq!(h.kind,1);
        assert_eq!(h.cost[1],10);
        assert_eq!(h.name,"House");
    }
    #[test]
    fn rejects_malformed_record_lengths(){
        assert!(parse_odb(&[0;ODB_RECORD_SIZE-1]).is_err());
    }
}
