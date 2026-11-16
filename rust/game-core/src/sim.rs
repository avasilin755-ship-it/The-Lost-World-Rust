//! Deterministic playable simulation scaffold over original maps / ODB rules.
//!
//! This is an integrated vertical slice, not yet an exact recreation of the
//! 600-slot scheduler, original economy, jobs, AI, combat, or multiplayer.
//! Terrain, building footprints, costs and object classes use original data.
//! Commands and game state are independent of Macroquad.

use crate::mission::MapObject;
use crate::rules::{ObjectRule, Rules, rule_id};
use crate::terrain::Terrain;
use std::{cmp::Reverse, collections::{BinaryHeap, HashMap, HashSet}};

pub const GOOD_WOOD:usize=1;
pub const GOOD_STONE:usize=2;
pub const GOOD_FOOD:usize=4;
pub const MAX_ENTITIES:usize=50_000;
const NEIGHBOURS:[(i32,i32);8]=[
    (-1,-1),(-1,0),(-1,1),(0,-1),(0,1),(1,-1),(1,0),(1,1)
];
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum Task {
    Idle,
    Walking{steps:Vec<(i32,i32)>,after:After},
    Gathering{tree:u32,time:u16},
    Delivering{storage:u32,good:usize,amount:i32},
    Constructing{site:u32},
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum After {
    Idle, Gather(u32), Deliver(u32,usize,i32), Construct(u32),
}
#[derive(Debug,Clone)]
pub struct Entity {
    pub id:u32,pub kind:i32,pub group:i32,pub object_type:i32,pub owner:i32,
    pub x:i32,pub y:i32,pub hp:i32,pub complete:bool,pub construction:u16,
    pub task:Task,pub carrying:i32,
}
#[derive(Debug,Clone)]
pub struct Simulation {
    pub terrain:Terrain,
    pub rules:Rules,
    pub objects:Vec<Entity>,
    pub next_id:u32,
    pub human_race:i32,
    pub tick:u64,
    pub gold:i32,
    pub goods:[i32;9],
    pub paused:bool,
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum GameError{
    Invalid(&'static str),NoPath,MissingObject,NotOwned,NotEnoughGoods,Occupied,NotLand
}
impl std::fmt::Display for GameError{
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"{self:?}")}
}
impl std::error::Error for GameError{}

impl Simulation {
    pub fn from_map(
        terrain:Terrain,rules:Rules,objects:Vec<MapObject>,
        human_race:i32,gold:i32,goods:[i32;9],
    )->Result<Self,GameError>{
        if objects.len()>MAX_ENTITIES{return Err(GameError::Invalid("map has too many objects"))}
        let mut states=Vec::with_capacity(objects.len());
        for (i,obj) in objects.into_iter().enumerate(){
            let rule=rules.get(&rule_id(obj.group,obj.object_type));
            let hp=rule.map_or(10,|r|r.hp.max(1));
            states.push(Entity{
                id:i as u32+1,kind:obj.kind,group:obj.group,
                object_type:obj.object_type,owner:obj.owner,
                x:obj.x.rem_euclid(terrain.size as i32),
                y:obj.y.rem_euclid(terrain.size as i32),
                hp,complete:true,construction:100,
                task:Task::Idle,carrying:0
            });
        }
        let next_id=states.len() as u32+1;
        Ok(Self{
            terrain,rules,objects:states,next_id,human_race,
            gold,goods,paused:false,tick:0
        })
    }

    pub fn entity(&self,id:u32)->Option<&Entity>{self.objects.iter().find(|o|o.id==id)}
    pub fn entity_mut(&mut self,id:u32)->Option<&mut Entity>{
        self.objects.iter_mut().find(|o|o.id==id)
    }
    pub fn rule(&self,o:&Entity)->Option<&ObjectRule>{
        self.rules.get(&rule_id(o.group,o.object_type))
    }
    pub fn unit_owned(&self,id:u32)->bool{
        self.entity(id).is_some_and(|o|o.kind==0 && o.group==self.human_race)
    }
    pub fn entities_at(&self,x:i32,y:i32)->Vec<u32>{
        self.objects.iter().filter(|o|self.covers(o,x,y)).map(|o|o.id).collect()
    }

    /// In original ODB, a building's anchor is its bottom-right footprint corner.
    pub fn covers(&self,o:&Entity,x:i32,y:i32)->bool{
        self.in_footprint(o,x,y)
    }
    fn blocked(&self,x:i32,y:i32,ignore:Option<u32>)->bool{
        if self.terrain.water(x,y){return true}
        self.objects.iter().any(|o|{
            if Some(o.id)==ignore{return false}
            // Units can path around each other; all nature/buildings are static.
            if o.kind==0{return false}
            self.in_footprint(o,x,y)
        })
    }
    fn in_footprint(&self,o:&Entity,x:i32,y:i32)->bool{
        let (sx,sy)=if o.kind==1{
            self.rules.get(&rule_id(o.group,o.object_type))
                .map_or((1,1),|r|(r.size_x.clamp(1,32),r.size_y.clamp(1,32)))
        }else{(1,1)};
        let n=self.terrain.size as i32;
        (o.x-x).rem_euclid(n)<sx && (o.y-y).rem_euclid(n)<sy
    }
    pub fn object_at(&self,x:i32,y:i32)->Option<u32>{
        self.objects.iter().rev().find(|o|self.in_footprint(o,x,y)).map(|o|o.id)
    }

    fn distance_heuristic(&self,x:i32,y:i32,to:(i32,i32))->i32{
        let n=self.terrain.size as i32;
        let dx=(to.0-x).abs();let dy=(to.1-y).abs();
        let dx=dx.min(n-dx);let dy=dy.min(n-dy);
        10*(dx.max(dy)-dx.min(dy))+14*dx.min(dy)
    }
    pub fn find_path(
        &self,start:(i32,i32),goal:(i32,i32),ignore:Option<u32>,
    )->Result<Vec<(i32,i32)>,GameError>{
        let n=self.terrain.size as i32;
        let start=(start.0.rem_euclid(n),start.1.rem_euclid(n));
        let goal=(goal.0.rem_euclid(n),goal.1.rem_euclid(n));
        if start==goal{return Ok(Vec::new())}
        if self.blocked(goal.0,goal.1,ignore){return Err(GameError::Occupied)}
        let mut frontier:BinaryHeap<(Reverse<i32>,Reverse<i32>,(i32,i32))>=BinaryHeap::new();
        frontier.push((Reverse(self.distance_heuristic(start.0,start.1,goal)),Reverse(0),start));
        let mut best:HashMap<(i32,i32),i32>=HashMap::new();
        let mut parent=HashMap::new();
        best.insert(start,0);
        let mut visited=0usize;
        while let Some((_,Reverse(cost),cell))=frontier.pop(){
            if cost!=*best.get(&cell).unwrap_or(&i32::MAX){continue}
            if cell==goal {
                let mut path=Vec::new();
                let mut pos=goal;
                while pos!=start{
                    path.push(pos);
                    pos=*parent.get(&pos).ok_or(GameError::NoPath)?;
                }
                path.reverse();
                return Ok(path)
            }
            visited+=1;
            if visited>30_000{return Err(GameError::NoPath)}
            for &(dx,dy) in &NEIGHBOURS{
                let next=((cell.0+dx).rem_euclid(n),(cell.1+dy).rem_euclid(n));
                if self.blocked(next.0,next.1,ignore){continue}
                // Prevent diagonal steps through the corners of occupied cells.
                if dx!=0 && dy!=0 && (self.blocked(cell.0+dx,cell.1,ignore)
                    ||self.blocked(cell.0,cell.1+dy,ignore)){continue}
                let step=if dx==0||dy==0{10}else{14};
                let next_cost=cost+step;
                if next_cost<*best.get(&next).unwrap_or(&i32::MAX){
                    best.insert(next,next_cost);
                    parent.insert(next,cell);
                    frontier.push((Reverse(next_cost+self.distance_heuristic(next.0,next.1,goal)),
                        Reverse(next_cost),next));
                }
            }
        }
        Err(GameError::NoPath)
    }
    fn nearest_adjacent(
        &self,unit:u32,target:(i32,i32)
    )->Result<Vec<(i32,i32)>,GameError>{
        let source=self.entity(unit).ok_or(GameError::MissingObject)?;
        let mut paths:Vec<_>=NEIGHBOURS.iter().filter_map(|&(dx,dy)|{
            let n=self.terrain.size as i32;
            let goal=((target.0+dx).rem_euclid(n),(target.1+dy).rem_euclid(n));
            self.find_path((source.x,source.y),goal,Some(unit)).ok()
        }).collect();
        paths.sort_by_key(|p|p.len());
        paths.into_iter().next().ok_or(GameError::NoPath)
    }
    pub fn order_move(&mut self,unit:u32,target:(i32,i32))->Result<(),GameError>{
        if !self.unit_owned(unit){return Err(GameError::NotOwned)}
        let source=self.entity(unit).ok_or(GameError::MissingObject)?;
        let path=self.find_path((source.x,source.y),target,Some(unit))?;
        self.entity_mut(unit).unwrap().task=Task::Walking{steps:path,after:After::Idle};
        Ok(())
    }
    pub fn order_gather(&mut self,unit:u32,tree:u32)->Result<(),GameError>{
        if !self.unit_owned(unit){return Err(GameError::NotOwned)}
        let target=self.entity(tree).ok_or(GameError::MissingObject)?;
        if target.kind!=2{return Err(GameError::Invalid("target must be nature"))}
        if self.harvest_yield(tree).is_none(){
            return Err(GameError::Invalid("this nature object is not a gatherable resource"))
        }
        let at=(target.x,target.y);
        let path=self.nearest_adjacent(unit,at)?;
        self.entity_mut(unit).unwrap().task=Task::Walking{
            steps:path,after:After::Gather(tree)
        };
        Ok(())
    }
    pub fn build(&mut self,unit:u32,rule_id:u32,at:(i32,i32))->Result<u32,GameError>{
        if !self.unit_owned(unit){return Err(GameError::NotOwned)}
        let rule=self.rules.get(&rule_id).ok_or(GameError::Invalid("unknown building rule"))?;
        if rule.kind!=1||rule.group!=self.human_race{
            return Err(GameError::Invalid("not a building of the selected race"))
        }
        let size_x=rule.size_x.clamp(1,32);
        let size_y=rule.size_y.clamp(1,32);
        for dx in 0..size_x{
            for dy in 0..size_y{
                let x=at.0-dx;
                let y=at.1-dy;
                if self.terrain.water(x,y){return Err(GameError::NotLand)}
                if self.objects.iter().any(|o|self.in_footprint(o,x,y)){
                    return Err(GameError::Occupied)
                }
            }
        }
        for (i,cost) in rule.cost.iter().enumerate(){
            if *cost>self.goods[i] {return Err(GameError::NotEnoughGoods)}
        }
        let path=self.nearest_adjacent(unit,at)?;
        for (i,cost) in rule.cost.iter().enumerate(){
            self.goods[i]-=*cost
        }
        let id=self.next_id;
        self.next_id+=1;
        self.objects.push(Entity{
            id,kind:1,group:rule.group,object_type:rule.object_type,
            owner:self.human_race,x:at.0,y:at.1,hp:rule.hp.max(1),
            complete:false,construction:0,task:Task::Idle,carrying:0
        });
        self.entity_mut(unit).unwrap().task=Task::Walking{
            steps:path,after:After::Construct(id)
        };
        Ok(id)
    }
    fn nearest_storage(&self,from:(i32,i32))->Option<u32>{
        self.objects.iter().filter(|o|{
            o.kind==1 && o.group==self.human_race
                &&(o.object_type==40||o.object_type==41)
        }).min_by_key(|o|self.distance_heuristic(from.0,from.1,(o.x,o.y)))
            .map(|o|o.id)
    }
    fn harvest_yield(&self,id:u32)->Option<(usize,i32)>{
        let nature=self.entity(id)?;
        if nature.kind!=2{return None}
        // ODB behaviour classes: 8 trees, 9 bushes, 13 stone.
        // Synthetic test fixtures without an ODB rule represent wood.
        let Some(rule)=self.rule(nature) else{return Some((GOOD_WOOD,1))};
        let good=match rule.class{
            8=>GOOD_WOOD,9=>GOOD_FOOD,13=>GOOD_STONE,
            _=>return None,
        };
        Some((good,rule.amount.clamp(1,100)))
    }
    fn start_delivery(&mut self,unit:u32,good:usize,amount:i32){
        let from=self.entity(unit).map(|o|(o.x,o.y));
        let Some(from)=from else{return};
        let Some(storage)=self.nearest_storage(from) else{
            self.goods[good]=self.goods[good].saturating_add(amount);return;
        };
        let Some(to)=self.entity(storage).map(|o|(o.x,o.y)) else{return};
        if let Ok(path)=self.nearest_adjacent(unit,to) {
            self.entity_mut(unit).unwrap().task=Task::Walking{
                steps:path,after:After::Deliver(storage,good,amount)
            };
        } else {
            self.goods[good]=self.goods[good].saturating_add(amount);
        }
    }
    /// Deterministic fixed-time simulation. Path speed is temporarily
    /// 6 ticks per tile and resource output is provisional, not vanilla exact.
    pub fn step(&mut self){
        if self.paused{return}
        self.tick+=1;
        let ids:Vec<u32>=self.objects.iter().map(|o|o.id).collect();
        let mut removed:HashSet<u32>=HashSet::new();
        for id in ids{
            let Some(i)=self.objects.iter().position(|o|o.id==id) else{continue};
            if self.objects[i].kind!=0{continue}
            let task=self.objects[i].task.clone();
            match task{
                Task::Idle=>{},
                Task::Walking{mut steps,after}=>{
                    if self.tick%6!=0{continue}
                    if !steps.is_empty(){
                        let (x,y)=steps.remove(0);
                        self.objects[i].x=x;
                        self.objects[i].y=y;
                    }
                    if steps.is_empty(){
                        self.objects[i].task=match after{
                            After::Idle=>Task::Idle,
                            After::Gather(tree)=>Task::Gathering{tree,time:0},
                            After::Deliver(storage,good,amount)=>Task::Delivering{storage,good,amount},
                            After::Construct(site)=>Task::Constructing{site},
                        };
                    }else{
                        self.objects[i].task=Task::Walking{steps,after}
                    }
                }
                Task::Gathering{tree,time}=>{
                    if self.entity(tree).is_none(){
                        self.objects[i].task=Task::Idle;continue
                    }
                    if time<59{
                        self.objects[i].task=Task::Gathering{tree,time:time+1}
                    } else {
                        let (good,amount)=self.harvest_yield(tree)
                            .unwrap_or((GOOD_WOOD,1));
                        removed.insert(tree);
                        self.objects[i].carrying=amount;
                        self.objects[i].task=Task::Idle;
                        self.start_delivery(id,good,amount);
                    }
                }
                Task::Delivering{storage,good,amount}=>{
                    if self.entity(storage).is_some(){
                        self.goods[good]=self.goods[good].saturating_add(amount);
                    }
                    self.objects[i].carrying=0;
                    self.objects[i].task=Task::Idle;
                }
                Task::Constructing{site}=>{
                    if let Some(building)=self.objects.iter_mut().find(|o|o.id==site){
                        building.construction=(building.construction+1).min(100);
                        if building.construction==100{
                            building.complete=true;
                            self.objects[i].task=Task::Idle;
                        }
                    }else{self.objects[i].task=Task::Idle}
                }
            }
        }
        if !removed.is_empty(){
            self.objects.retain(|o|!removed.contains(&o.id));
        }
        // Integration placeholder, intentionally NOT claimed as original economy.
        // A full faithful tax/needs/round scheduler is a later subsystem.
    }
    pub fn selected_unit_ids(&self)->Vec<u32>{
        self.objects.iter().filter(|o|o.kind==0 && o.group==self.human_race)
            .map(|o|o.id).collect()
    }

    /// Minimal local binary snapshots of entity positions, resources and ticks.
    /// Orders are reset on load; no original savegame format is claimed.
    pub fn save_snapshot(&self)->Vec<u8>{
        let mut b=b"TLWSNAP1".to_vec();
        b.extend_from_slice(&self.tick.to_le_bytes());
        b.extend_from_slice(&self.gold.to_le_bytes());
        for good in self.goods {b.extend_from_slice(&good.to_le_bytes())}
        b.extend_from_slice(&self.next_id.to_le_bytes());
        b.extend_from_slice(&(self.objects.len() as u32).to_le_bytes());
        for o in &self.objects {
            b.extend_from_slice(&o.id.to_le_bytes());
            for v in [o.kind,o.group,o.object_type,o.owner,o.x,o.y,o.hp] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b.push(u8::from(o.complete));
            b.extend_from_slice(&o.construction.to_le_bytes());
        }
        b
    }
    pub fn load_snapshot(&mut self,b:&[u8])->Result<(),GameError>{
        if b.len()<8+8+4+9*4+4+4||&b[..8]!=b"TLWSNAP1"{
            return Err(GameError::Invalid("invalid snapshot header"))
        }
        let mut pos=8usize;
        let mut take=|n:usize|->Result<&[u8],GameError>{
            let end=pos.checked_add(n).ok_or(GameError::Invalid("snapshot overflow"))?;
            let value=b.get(pos..end).ok_or(GameError::Invalid("truncated snapshot"))?;
            pos=end;Ok(value)
        };
        let tick=u64::from_le_bytes(take(8)?.try_into().unwrap());
        let gold=i32::from_le_bytes(take(4)?.try_into().unwrap());
        let mut goods=[0i32;9];
        for g in &mut goods{*g=i32::from_le_bytes(take(4)?.try_into().unwrap())}
        let next_id=u32::from_le_bytes(take(4)?.try_into().unwrap());
        let count=u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
        if count>MAX_ENTITIES{return Err(GameError::Invalid("snapshot too many entities"))}
        let mut objects=Vec::with_capacity(count);
        let mut ids=HashSet::new();
        for _ in 0..count{
            let id=u32::from_le_bytes(take(4)?.try_into().unwrap());
            if id==0||!ids.insert(id){return Err(GameError::Invalid("invalid snapshot entity id"))}
            let mut values=[0i32;7];
            for v in &mut values{*v=i32::from_le_bytes(take(4)?.try_into().unwrap())}
            let complete=take(1)?[0]!=0;
            let construction=u16::from_le_bytes(take(2)?.try_into().unwrap());
            if construction>100{return Err(GameError::Invalid("invalid construction"))}
            if values[4]<0||values[5]<0||values[4]>=self.terrain.size as i32
                || values[5]>=self.terrain.size as i32{
                return Err(GameError::Invalid("entity out of bounds"))
            }
            objects.push(Entity{
                id,kind:values[0],group:values[1],object_type:values[2],
                owner:values[3],x:values[4],y:values[5],hp:values[6],
                complete,construction,task:Task::Idle,carrying:0
            });
        }
        if pos!=b.len()||next_id<=ids.iter().copied().max().unwrap_or(0){
            return Err(GameError::Invalid("snapshot trailing data or invalid next id"))
        }
        self.tick=tick;self.gold=gold;self.goods=goods;self.next_id=next_id;
        self.objects=objects;
        Ok(())
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn setup()->Simulation{
        let n=128;
        let mut t=Terrain{
            size_bits:7,size:n,height:vec![0;n*n],class:vec![0;n*n],
            tile:vec![0;n*n],light:vec![162;n*n],rng_state:0,water_class:11
        };
        let water_cell=t.index(3,3);
        t.class[water_cell]=11;
        let rules=Rules::new();
        let map=vec![
            MapObject{kind:0,group:2,object_type:19,owner:0,x:10,y:10,z:0},
            MapObject{kind:2,group:6,object_type:9,owner:-1,x:12,y:10,z:0}
        ];
        Simulation::from_map(t,rules,map,2,200,[0;9]).unwrap()
    }
    #[test]
    fn path_finds_walkable_destination_and_avoids_water(){
        let sim=setup();
        let path=sim.find_path((2,3),(4,3),Some(1)).unwrap();
        assert!(!path.contains(&(3,3)));
        assert_eq!(path.last().copied(),Some((4,3)));
    }
    #[test]
    fn unit_can_walk_and_gather_and_save(){
        let mut sim=setup();
        sim.order_move(1,(11,10)).unwrap();
        for _ in 0..24 {sim.step()}
        assert_eq!((sim.entity(1).unwrap().x,sim.entity(1).unwrap().y),(11,10));
        sim.order_gather(1,2).unwrap();
        for _ in 0..70{sim.step()}
        assert!(sim.entity(2).is_none());
        assert_eq!(sim.goods[1],1);
        let bytes=sim.save_snapshot();
        let mut loaded=setup();
        loaded.load_snapshot(&bytes).unwrap();
        assert_eq!(loaded.goods[1],1);
        assert_eq!(loaded.tick,sim.tick);
        assert_eq!(loaded.objects.len(),sim.objects.len());
    }
    #[test]
    fn snapshot_rejects_truncation(){
        let mut sim=setup();
        assert!(sim.load_snapshot(b"oops").is_err());
        let mut b=sim.save_snapshot();
        b.truncate(b.len()-5);
        assert!(sim.load_snapshot(&b).is_err());
    }
}
