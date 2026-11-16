//! Deterministic playable simulation scaffold over original maps / ODB rules.
//!
//! This is an integrated vertical slice, not yet an exact recreation of the
//! 600-slot scheduler, original economy, jobs, AI, combat, or multiplayer.
//! Terrain, building footprints, costs and object classes use original data.
//! Commands and game state are independent of Macroquad.

use crate::mission::MapObject;
use crate::rules::{ObjectRule, Rules, rule_id};
use crate::terrain::Terrain;
use crate::economy::{Consumption,ECONOMY_ROUND_TICKS};
use std::{cmp::Reverse, collections::{BinaryHeap, BTreeMap, HashMap, HashSet}};

pub const GOOD_WOOD:usize=1;
pub const GOOD_STONE:usize=2;
pub const GOOD_FOOD:usize=4;
pub const MAX_ENTITIES:usize=50_000;
const NEIGHBOURS:[(i32,i32);8]=[
    (-1,-1),(-1,0),(-1,1),(0,-1),(0,1),(1,-1),(1,0),(1,1)
];
/// Native OD directions are clockwise starting from the back view.
/// These indexes match MG0..MG7 in the original game archives.
/// Native animals alternate roaming and their original ID3 idle animation.
/// The period is deterministic per entity, independent of frame rate.
/// Idle workers occasionally turn and play a native ID3/ID5 fidget.
/// A per-entity offset prevents synchronized animations across the map.
pub fn unit_idle_phase(id:u32,tick:u64)->u64{
    let period=50+(id as u64*113)%250;
    (tick+id as u64*37)%period
}
pub fn animal_cycle_phase(id:u32,tick:u64)->u64{
    let period=140+(id as u64*29)%55;
    (tick+id as u64*13)%period
}
pub fn facing_for_delta(dx:i32,dy:i32)->Option<u8>{
    match (dx.signum(),dy.signum()){
        (-1,-1)=>Some(0),(-1,0)=>Some(1),(-1,1)=>Some(2),
        (0,1)=>Some(3),(1,1)=>Some(4),(1,0)=>Some(5),
        (1,-1)=>Some(6),(0,-1)=>Some(7),_=>None,
    }
}
fn wrapped_delta(from:(i32,i32),to:(i32,i32),size:i32)->(i32,i32){
    let adjust=|mut delta:i32|{
        if delta>size/2{delta-=size}
        else if delta < -size/2{delta+=size}
        delta
    };
    (adjust(to.0-from.0),adjust(to.1-from.1))
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum Task {
    Idle,
    Walking{steps:Vec<(i32,i32)>,after:After},
    Gathering{tree:u32,time:u16},
    Delivering{storage:u32,good:usize,amount:i32},
    Constructing{site:u32},
    /// Carrying resources until a completed warehouse becomes reachable.
    Holding{good:usize,amount:i32},
    /// Porter has arrived at a completed store, but has not picked up the
    /// requested unit of raw material for an unfinished building yet.
    Fetching{site:u32,storage:u32,good:usize},
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum After {
    Idle, Gather(u32), Deliver(u32,usize,i32), Construct(u32),
    Hold(usize,i32),
    /// Walk to a real store first. The material leaves the town stock only
    /// when the porter actually picks it up, not at site placement.
    Fetch(u32,u32,usize),
}
#[derive(Debug,Clone)]
pub struct Entity {
    pub id:u32,pub kind:i32,pub group:i32,pub object_type:i32,pub owner:i32,
    pub x:i32,pub y:i32,pub hp:i32,pub complete:bool,pub construction:u16,
    pub task:Task,pub carrying:i32,pub facing:u8,
}
/// Material stock and progress belong to each construction order, not
/// to the globally shared town warehouse. Porters carry one unit per trip.
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct SiteLogistics {
    pub remaining:[i32;9],
    pub stock:[i32;9],
    pub work_done:u16,
    pub work_total:u16,
    pub ready_for_work:bool,
}
impl SiteLogistics {
    fn new(cost:[i32;9])->Self {
        let remaining=cost.map(|n|n.max(0));
        let total=remaining.iter().sum::<i32>().clamp(0,10000) as u16;
        Self {
            remaining,stock:[0;9],work_done:0,work_total:total.max(10),
            ready_for_work:false,
        }
    }
    fn work_step(&mut self)->bool {
        if !self.ready_for_work {
            if let Some(i)=self.remaining.iter().position(|&n|n>0){
                if self.stock[i]==0{return false}
                self.stock[i]-=1;
                self.remaining[i]-=1;
                self.ready_for_work=true;
                return false
            }
        }
        self.ready_for_work=false;
        self.work_done=(self.work_done+1).min(self.work_total);
        true
    }
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
    /// Original per-town food/luxury fractional consumption state. There
    /// is one simulated player town in the current integrated client.
    pub consumption:Consumption,
    pub races_present:[bool;3],
    /// Original construction-site request queues and material stock.
    pub sites:BTreeMap<u32,SiteLogistics>,
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum GameError{
    Invalid(&'static str),NoPath,MissingObject,NotOwned,NotEnoughGoods,Occupied,NotLand,
    Busy,TownHallExists,TooManyEntities,WrongWorker
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
                task:Task::Idle,carrying:0,facing:4
            });
        }
        let next_id=states.len() as u32+1;
        let mut present=[false;3];
        if (0..3).contains(&human_race){present[human_race as usize]=true}
        let mut world=Self{
            terrain,rules,objects:states,next_id,human_race,
            gold,goods,paused:false,tick:0,
            consumption:Consumption::default(),races_present:present,
            sites:BTreeMap::new(),
        };
        // Base mission terrain was generated from a seed and therefore has
        // not yet had any building pads constructed. Original map loading
        // levels and paves them before displaying the first scene.
        if world.terrain.water_class==11{
            let pads=world.objects.iter().filter(|o|o.kind==1).filter_map(|o|{
                world.rules.get(&rule_id(o.group,o.object_type)).map(|r|
                    (o.x,o.y,o.object_type,r.size_x,r.size_y)
                )
            }).collect::<Vec<_>>();
            for (x,y,ty,sx,sy) in pads{
                if ty!=62&&ty!=63{
                    world.terrain.foundation(x,y,sx,sy)
                }
            }
        }
        Ok(world)
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
    /// One occupancy raster per route avoids an O(objects) query for
    /// each A* neighbour. This matters on maps with thousands of trees.
    fn blocked_grid(&self,ignore:Option<u32>)->Vec<bool>{
        let n=self.terrain.size as i32;
        let mut grid=Vec::with_capacity(self.terrain.size*self.terrain.size);
        for y in 0..n{
            for x in 0..n {grid.push(self.terrain.water(x,y))}
        }
        for obj in &self.objects{
            if obj.kind==0||Some(obj.id)==ignore{continue}
            let (sx,sy)=if obj.kind==1{
                self.rules.get(&rule_id(obj.group,obj.object_type))
                    .map_or((1,1),|r|(r.size_x.clamp(1,32),r.size_y.clamp(1,32)))
            }else{(1,1)};
            for dx in 0..sx{
                for dy in 0..sy{
                    let i=self.terrain.index(obj.x-dx,obj.y-dy);
                    grid[i]=true;
                }
            }
        }
        grid
    }
    pub fn find_path(
        &self,start:(i32,i32),goal:(i32,i32),ignore:Option<u32>,
    )->Result<Vec<(i32,i32)>,GameError>{
        let n=self.terrain.size as i32;
        let start=(start.0.rem_euclid(n),start.1.rem_euclid(n));
        let goal=(goal.0.rem_euclid(n),goal.1.rem_euclid(n));
        if start==goal{return Ok(Vec::new())}
        let occupancy=self.blocked_grid(ignore);
        let is_blocked=|x:i32,y:i32| occupancy[self.terrain.index(x,y)];
        if is_blocked(goal.0,goal.1){return Err(GameError::Occupied)}
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
                if is_blocked(next.0,next.1){continue}
                // Prevent diagonal steps through the corners of occupied cells.
                if dx!=0 && dy!=0 && (is_blocked(cell.0+dx,cell.1)
                    ||is_blocked(cell.0,cell.1+dy)){continue}
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
    /// Explicit escape hatch for a worker holding cargo without storage.
    /// Never discard items automatically: the player must request it.
    pub fn drop_cargo(&mut self,unit:u32)->Result<(),GameError>{
        if !self.unit_owned(unit){return Err(GameError::NotOwned)}
        let worker=self.entity_mut(unit).ok_or(GameError::MissingObject)?;
        if !matches!(worker.task,Task::Holding{..}){
            return Err(GameError::Invalid("worker is not waiting with cargo"))
        }
        worker.carrying=0;
        worker.task=Task::Idle;
        Ok(())
    }
    pub fn order_move(&mut self,unit:u32,target:(i32,i32))->Result<(),GameError>{
        if !self.unit_owned(unit){return Err(GameError::NotOwned)}
        let source=self.entity(unit).ok_or(GameError::MissingObject)?;
        let path=self.find_path((source.x,source.y),target,Some(unit))?;
        // A manual move must not silently erase a load of undelivered goods.
        let after=match &source.task{
            Task::Holding{good,amount}|Task::Delivering{good,amount,..}
                =>After::Hold(*good,*amount),
            Task::Walking{after:After::Deliver(_,good,amount),..}
            |Task::Walking{after:After::Hold(good,amount),..}
                =>After::Hold(*good,*amount),
            _=>After::Idle,
        };
        self.entity_mut(unit).unwrap().task=Task::Walking{steps:path,after};
        Ok(())
    }
    pub fn order_gather(&mut self,unit:u32,tree:u32)->Result<(),GameError>{
        if !self.unit_owned(unit){return Err(GameError::NotOwned)}
        let target=self.entity(tree).ok_or(GameError::MissingObject)?;
        if target.kind!=2{return Err(GameError::Invalid("target must be nature"))}
        if self.harvest_yield(tree).is_none(){
            return Err(GameError::Invalid("this nature object is not a gatherable resource"))
        }
        // Original tasks are role-specific. The previous prototype allowed
        // a builder (type 11) to harvest trees and crops, which the video
        // revealed as a gameplay bug.
        let nature_class=self.rule(target).map_or(8,|r|r.class);
        let worker_type=self.entity(unit).ok_or(GameError::MissingObject)?.object_type;
        let allowed=match nature_class{
            8=>worker_type==19,         // woodcutter -> tree
            9|10..=12=>worker_type==9, // porter -> bushes and crops
            13=>worker_type==27,       // stonemason -> stone
            _=>false,
        };
        if !allowed{return Err(GameError::WrongWorker)}
        if self.entity(unit).is_some_and(|o|o.carrying>0){
            return Err(GameError::Busy)
        }
        let at=(target.x,target.y);
        let path=self.nearest_adjacent(unit,at)?;
        self.entity_mut(unit).unwrap().task=Task::Walking{
            steps:path,after:After::Gather(tree)
        };
        Ok(())
    }
    /// Validate the whole footprint and real ODB inventory before allowing a site.
    /// A provisional one-town-hall rule prevents infinite free town-hall spam
    /// until settlement founding is implemented.
    pub fn can_build(&self,unit:u32,rule_id:u32,at:(i32,i32))->Result<(),GameError>{
        if !self.unit_owned(unit){return Err(GameError::NotOwned)}
        if self.entity(unit).is_some_and(|o|o.object_type!=11){
            return Err(GameError::WrongWorker)
        }
        if !matches!(self.entity(unit).unwrap().task,Task::Idle){
            return Err(GameError::Busy)
        }
        let rule=self.rules.get(&rule_id).ok_or(GameError::Invalid("unknown building rule"))?;
        if rule.kind!=1||rule.group!=self.human_race{
            return Err(GameError::Invalid("not a building of the selected race"))
        }
        if self.objects.len()>=MAX_ENTITIES{return Err(GameError::TooManyEntities)}
        if rule.object_type==40 && self.objects.iter().any(|o|
            o.kind==1 && o.group==self.human_race && o.object_type==40){
            return Err(GameError::TownHallExists)
        }
        let n=self.terrain.size as i32;
        for dx in 0..rule.size_x.clamp(1,32){
            for dy in 0..rule.size_y.clamp(1,32){
                let x=at.0-dx;
                let y=at.1-dy;
                if x<0||y<0||x>=n||y>=n{return Err(GameError::NotLand)}
                if self.terrain.water(x,y){return Err(GameError::NotLand)}
                if self.objects.iter().any(|o|self.in_footprint(o,x,y)){
                    return Err(GameError::Occupied)
                }
            }
        }
        // Original placement accepts sites even if stocks are empty:
        // per-good requests remain open until a porter can supply them.
        Ok(())
    }
    /// Find a reachable work cell outside an entire building footprint,
    /// not merely adjacent to its often-blocked bottom-right anchor.
    fn adjacent_to_footprint(
        &self,unit:u32,at:(i32,i32),sx:i32,sy:i32
    )->Result<Vec<(i32,i32)>,GameError>{
        let source=self.entity(unit).ok_or(GameError::MissingObject)?;
        let n=self.terrain.size as i32;
        let mut candidates=HashSet::new();
        for dx in 0..sx {
            for dy in 0..sy {
                if dx!=0 && dy!=0 && dx!=sx-1 && dy!=sy-1{continue}
                for &(ox,oy) in &NEIGHBOURS {
                    let px=at.0-dx+ox;
                    let py=at.1-dy+oy;
                    if px<0||py<0||px>=n||py>=n{continue}
                    if at.0-px>=0 && at.0-px<sx
                        && at.1-py>=0 && at.1-py<sy{continue}
                    if !self.blocked(px,py,Some(unit)){
                        candidates.insert((px,py));
                    }
                }
            }
        }
        let mut candidates:Vec<_>=candidates.into_iter().collect();
        candidates.sort_by_key(|&goal|self.distance_heuristic(source.x,source.y,goal));
        for goal in candidates{
            if let Ok(path)=self.find_path((source.x,source.y),goal,Some(unit)){
                return Ok(path)
            }
        }
        Err(GameError::NoPath)
    }
    pub fn build(&mut self,unit:u32,rule_id:u32,at:(i32,i32))->Result<u32,GameError>{
        self.can_build(unit,rule_id,at)?;
        let rule=self.rules.get(&rule_id).unwrap().clone();
        // The site must exist during A* so the route cannot cut through it.
        let id=self.next_id;
        self.objects.push(Entity{
            id,kind:1,group:rule.group,object_type:rule.object_type,
            owner:self.human_race,x:at.0,y:at.1,hp:(rule.hp.max(1)+9)/10,
            complete:false,construction:0,task:Task::Idle,carrying:0,facing:4
        });
        let path=match self.adjacent_to_footprint(
            unit,at,rule.size_x.clamp(1,32),rule.size_y.clamp(1,32)
        ){
            Ok(path)=>path,
            Err(error)=>{self.objects.pop();return Err(error)}
        };
        self.sites.insert(id,SiteLogistics::new(rule.cost));
        self.next_id+=1;
        if rule.object_type!=62&&rule.object_type!=63{
            self.terrain.foundation(at.0,at.1,rule.size_x,rule.size_y)
        }
        self.entity_mut(unit).unwrap().task=Task::Walking{
            steps:path,after:After::Construct(id)
        };
        Ok(id)
    }
    /// Resume an unfinished building without charging its materials again.
    pub fn order_construct(&mut self,unit:u32,site:u32)->Result<(),GameError>{
        if !self.unit_owned(unit){return Err(GameError::NotOwned)}
        if self.entity(unit).is_some_and(|o|o.object_type!=11){
            return Err(GameError::WrongWorker)
        }
        if !matches!(self.entity(unit).unwrap().task,Task::Idle){
            return Err(GameError::Busy)
        }
        let target=self.entity(site).ok_or(GameError::MissingObject)?;
        if target.kind!=1 || target.group!=self.human_race || target.complete{
            return Err(GameError::Invalid("not an unfinished owned building"))
        }
        let rule=self.rule(target).ok_or(GameError::Invalid("missing building rule"))?;
        let at=(target.x,target.y);
        let size=(rule.size_x.clamp(1,32),rule.size_y.clamp(1,32));
        let path=self.adjacent_to_footprint(unit,at,size.0,size.1)?;
        self.entity_mut(unit).unwrap().task=Task::Walking{
            steps:path,after:After::Construct(site)
        };
        Ok(())
    }
    fn nearest_storage(&self,from:(i32,i32))->Option<u32>{
        self.objects.iter().filter(|o|{
            o.kind==1 && o.group==self.human_race && o.complete
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
            self.entity_mut(unit).unwrap().task=Task::Holding{good,amount};
            return;
        };
        let Some(to)=self.entity(storage).map(|o|(o.x,o.y)) else{return};
        if let Ok(path)=self.nearest_adjacent(unit,to) {
            self.entity_mut(unit).unwrap().task=Task::Walking{
                steps:path,after:After::Deliver(storage,good,amount)
            };
        } else {
            self.entity_mut(unit).unwrap().task=Task::Holding{good,amount};
        }
    }
    /// Count already dispatched construction material, including travel
    /// towards the store and the return trip. No duplicate requests.
    fn site_in_flight(&self,site:u32)->[i32;9]{
        let mut count=[0;9];
        for worker in &self.objects {
            if worker.kind!=0{continue}
            match &worker.task {
                Task::Walking{after:After::Fetch(s,_,good),..}
                    if *s==site&&*good<9=>count[*good]+=1,
                Task::Fetching{site:s,good,..}
                    if *s==site&&*good<9=>count[*good]+=1,
                Task::Walking{after:After::Deliver(s,good,amount),..}
                    if *s==site&&*good<9=>count[*good]+=*amount,
                Task::Delivering{storage:s,good,amount}
                    if *s==site&&*good<9=>count[*good]+=*amount,
                _=>{}
            }
        }
        count
    }
    /// Port one unit per transporter trip, as in the original game. We do
    /// not withdraw goods until a porter arrives at the source building.
    /// Pathing is provisional (no original store door queue yet).
    fn dispatch_site_materials(&mut self){
        let site_ids:Vec<u32>=self.sites.keys().copied().collect();
        for site in site_ids{
            let Some(target)=self.entity(site) else{continue};
            if target.complete{continue}
            let required=self.sites[&site].remaining;
            let local=self.sites[&site].stock;
            let mut transit=self.site_in_flight(site);
            for good in 0..9 {
                while required[good]>local[good]+transit[good]
                    && self.goods[good]>0
                {
                    let workers:Vec<u32>=self.objects.iter()
                        .filter(|o|o.kind==0&&o.group==self.human_race
                            &&o.object_type==9
                            &&o.carrying==0&&matches!(o.task,Task::Idle))
                        .map(|o|o.id).collect();
                    let mut dispatched=false;
                    for porter in workers{
                        let Some(source)=self.entity(porter).map(|o|(o.x,o.y))
                            else{continue};
                        let Some(store)=self.nearest_storage(source) else{break};
                        let Some(storage)=self.entity(store) else{continue};
                        let at=(storage.x,storage.y);
                        let dims=self.rule(storage).map_or((1,1),|r|
                            (r.size_x.clamp(1,32),r.size_y.clamp(1,32))
                        );
                        if let Ok(path)=self.adjacent_to_footprint(
                            porter,at,dims.0,dims.1
                        ){
                            self.entity_mut(porter).unwrap().task=Task::Walking{
                                steps:path,after:After::Fetch(site,store,good)
                            };
                            transit[good]+=1;
                            dispatched=true;
                            break
                        }
                    }
                    if !dispatched{break}
                }
            }
        }
    }
    /// Arrive at a completed store; pick up exactly one town good, then
    /// head to the perimeter of the unfinished construction footprint.
    fn fetch_for_site(
        &mut self,worker:u32,site:u32,storage:u32,good:usize
    ){
        let valid=self.sites.contains_key(&site)
            && self.entity(site).is_some_and(|o|!o.complete)
            && self.entity(storage).is_some_and(|o|o.complete)
            && good<9&&self.goods[good]>0;
        if !valid{
            if let Some(o)=self.entity_mut(worker){o.task=Task::Idle}
            return
        }
        let Some(target)=self.entity(site) else{return};
        let at=(target.x,target.y);
        let dims=self.rule(target).map_or((1,1),|r|
            (r.size_x.clamp(1,32),r.size_y.clamp(1,32))
        );
        let route=self.adjacent_to_footprint(worker,at,dims.0,dims.1);
        if let Ok(steps)=route{
            self.goods[good]-=1;
            if let Some(o)=self.entity_mut(worker){
                o.carrying=1;
                o.task=Task::Walking{
                    steps,after:After::Deliver(site,good,1)
                };
            }
        }else if let Some(o)=self.entity_mut(worker){
            o.task=Task::Idle;
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
            if self.objects[i].kind==2{
                // Wild animals (ODB class 7) have MG0..7 walking and ID3
                // idle animations. A tiny deterministic wander gives the
                // renderer real facing/motion events instead of looping all
                // nature sprites or replaying harvest animations.
                let animal=self.rules.get(&rule_id(
                    self.objects[i].group,self.objects[i].object_type
                )).is_some_and(|rule|rule.class==7);
                if animal {
                    let phase=animal_cycle_phase(id,self.tick);
                    if phase==0{
                        let (x,y)=(self.objects[i].x,self.objects[i].y);
                        let dir=((self.tick/10+id as u64*37)%8) as usize;
                        let n=self.terrain.size as i32;
                        for j in 0..NEIGHBOURS.len(){
                            let (dx,dy)=NEIGHBOURS[(dir+j)%8];
                            let to=(x+dx,y+dy);
                            if to.0<0||to.1<0||to.0>=n||to.1>=n{continue}
                            if self.blocked(to.0,to.1,Some(id)){continue}
                            self.objects[i].x=to.0;
                            self.objects[i].y=to.1;
                            if let Some(facing)=facing_for_delta(dx,dy){
                                self.objects[i].facing=facing;
                            }
                            break
                        }
                    }else if phase==20{
                        self.objects[i].facing=3; // native animal idle ID3
                    }
                }
                continue
            }
            if self.objects[i].kind!=0{continue}
            let task=self.objects[i].task.clone();
            match task{
                Task::Idle=>{
                    if self.objects[i].carrying==0 && unit_idle_phase(id,self.tick)==0{
                        self.objects[i].facing=if id%2==0{3}else{5};
                    }
                },
                Task::Walking{mut steps,after}=>{
                    if self.tick%6!=0{continue}
                    if !steps.is_empty(){
                        let (x,y)=steps.remove(0);
                        let old=(self.objects[i].x,self.objects[i].y);
                        let delta=wrapped_delta(old,(x,y),self.terrain.size as i32);
                        if let Some(facing)=facing_for_delta(delta.0,delta.1){
                            self.objects[i].facing=facing;
                        }
                        self.objects[i].x=x;
                        self.objects[i].y=y;
                    }
                    if steps.is_empty(){
                        let target_id=match after{
                            After::Gather(id)|After::Construct(id)=>Some(id),
                            _=>None,
                        };
                        if let Some(target)=target_id.and_then(|id|self.entity(id)){
                            let source=(self.objects[i].x,self.objects[i].y);
                            let delta=wrapped_delta(
                                source,(target.x,target.y),self.terrain.size as i32);
                            if let Some(facing)=facing_for_delta(delta.0,delta.1){
                                self.objects[i].facing=facing;
                            }
                        }
                        self.objects[i].task=match after{
                            After::Idle=>Task::Idle,
                            After::Gather(tree)=>Task::Gathering{tree,time:0},
                            After::Fetch(site,storage,good)=>Task::Fetching{
                                site,storage,good
                            },
                            After::Deliver(storage,good,amount)=>Task::Delivering{storage,good,amount},
                            After::Construct(site)=>Task::Constructing{site},
                            After::Hold(good,amount)=>Task::Holding{good,amount},
                        };
                    }else{
                        self.objects[i].task=Task::Walking{steps,after}
                    }
                }
                Task::Gathering{tree,time}=>{
                    if self.entity(tree).is_none()||removed.contains(&tree){
                        self.objects[i].task=Task::Idle;continue
                    }
                    if time<59{
                        self.objects[i].task=Task::Gathering{tree,time:time+1}
                    } else {
                        let Some((good,amount))=self.harvest_yield(tree) else{
                            self.objects[i].task=Task::Idle;continue
                        };
                        removed.insert(tree);
                        self.objects[i].carrying=amount;
                        self.objects[i].task=Task::Idle;
                        self.start_delivery(id,good,amount);
                    }
                }
                Task::Fetching{site,storage,good}=>{
                    self.fetch_for_site(id,site,storage,good);
                }
                Task::Delivering{storage,good,amount}=>{
                    if let Some(site)=self.sites.get_mut(&storage){
                        if good<9{
                            site.stock[good]=site.stock[good].saturating_add(amount);
                        }
                        self.objects[i].carrying=0;
                        self.objects[i].task=Task::Idle;
                    }else if self.entity(storage).is_some_and(|o|
                        o.complete&&o.kind==1&&(o.object_type==40||o.object_type==41)
                    ){
                        self.goods[good]=self.goods[good].saturating_add(amount);
                        self.objects[i].carrying=0;
                        self.objects[i].task=Task::Idle;
                    }else{
                        self.objects[i].task=Task::Holding{good,amount};
                    }
                }
                Task::Holding{good,amount}=>{
                    if self.tick%20==0{
                        self.start_delivery(id,good,amount);
                    }
                }
                Task::Constructing{site}=>{
                    if self.tick%15!=0{continue}
                    let Some(target)=self.entity(site) else{
                        self.objects[i].task=Task::Idle;continue
                    };
                    let full_hp=self.rule(target).map_or(1,|r|r.hp.max(1));
                    // Finish borrowing the per-site ledger before
                    // mutating the physical building entity.
                    let update=if let Some(supply)=self.sites.get_mut(&site){
                        if supply.work_step(){
                            Some((supply.work_done,supply.work_total))
                        }else{None}
                    }else{
                        self.objects[i].task=Task::Idle;continue
                    };
                    if let Some((done,total))=update{
                        let progress=u32::from(done)*100/u32::from(total);
                        if let Some(building)=self.entity_mut(site){
                            building.construction=progress as u16;
                            building.hp=((progress as i32*full_hp)/100)
                                .clamp(1,full_hp);
                            if done==total{
                                building.complete=true;
                                self.objects[i].task=Task::Idle;
                            }
                        }
                    }
                }
            }
        }
        if !removed.is_empty(){
            self.objects.retain(|o|!removed.contains(&o.id));
        }
        let live_sites:HashSet<u32>=self.objects.iter()
            .filter(|o|o.kind==1&&!o.complete).map(|o|o.id).collect();
        self.sites.retain(|site,_|live_sites.contains(site));
        if self.tick%15==0{
            self.dispatch_site_materials();
        }
        // First live economy subsystem: food/luxury every 299 ticks.
        // Population is currently approximated by the active outdoor
        // units of the player's race; original indoor residents and
        // multiple towns are pending separate migrations.
        if self.tick%ECONOMY_ROUND_TICKS==0{
            let population=self.objects.iter().filter(|o|
                o.kind==0&&o.group==self.human_race
            ).count();
            self.consumption.round(
                population,self.human_race.clamp(0,2) as usize,
                self.races_present,&mut self.goods
            );
        }
    }
    pub fn selected_unit_ids(&self)->Vec<u32>{
        self.objects.iter().filter(|o|o.kind==0 && o.group==self.human_race)
            .map(|o|o.id).collect()
    }

    /// TLWSNAP6 adds porter/source transport tasks and per-site material
    /// stocks. Prior versions 1-5 remain loadable for backwards compatibility.
    pub fn save_snapshot(&self)->Vec<u8>{
        let mut b=b"TLWSNAP6".to_vec();
        b.extend_from_slice(&self.tick.to_le_bytes());
        b.push(u8::from(self.paused));
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
            b.extend_from_slice(&o.carrying.to_le_bytes());
            b.push(o.facing);
            match &o.task{
                Task::Idle=>b.push(0),
                Task::Walking{steps,after}=>{
                    b.push(1);
                    b.extend_from_slice(&(steps.len() as u32).to_le_bytes());
                    for &(x,y) in steps{
                        b.extend_from_slice(&x.to_le_bytes());
                        b.extend_from_slice(&y.to_le_bytes());
                    }
                    match after{
                        After::Idle=>b.push(0),
                        After::Gather(id)=>{
                            b.push(1);b.extend_from_slice(&id.to_le_bytes());
                        }
                        After::Deliver(id,good,amount)=>{
                            b.push(2);b.extend_from_slice(&id.to_le_bytes());
                            b.extend_from_slice(&(*good as u32).to_le_bytes());
                            b.extend_from_slice(&amount.to_le_bytes());
                        }
                        After::Construct(id)=>{
                            b.push(3);b.extend_from_slice(&id.to_le_bytes());
                        }
                        After::Hold(good,amount)=>{
                            b.push(4);
                            b.extend_from_slice(&(*good as u32).to_le_bytes());
                            b.extend_from_slice(&amount.to_le_bytes());
                        }
                        After::Fetch(site,source,good)=>{
                            b.push(5);
                            b.extend_from_slice(&site.to_le_bytes());
                            b.extend_from_slice(&source.to_le_bytes());
                            b.extend_from_slice(&(*good as u32).to_le_bytes());
                        }
                    }
                }
                Task::Gathering{tree,time}=>{
                    b.push(2);
                    b.extend_from_slice(&tree.to_le_bytes());
                    b.extend_from_slice(&time.to_le_bytes());
                }
                Task::Delivering{storage,good,amount}=>{
                    b.push(3);
                    b.extend_from_slice(&storage.to_le_bytes());
                    b.extend_from_slice(&(*good as u32).to_le_bytes());
                    b.extend_from_slice(&amount.to_le_bytes());
                }
                Task::Constructing{site}=>{
                    b.push(4);b.extend_from_slice(&site.to_le_bytes());
                }
                Task::Holding{good,amount}=>{
                    b.push(5);
                    b.extend_from_slice(&(*good as u32).to_le_bytes());
                    b.extend_from_slice(&amount.to_le_bytes());
                }
                Task::Fetching{site,storage,good}=>{
                    b.push(6);
                    b.extend_from_slice(&site.to_le_bytes());
                    b.extend_from_slice(&storage.to_le_bytes());
                    b.extend_from_slice(&(*good as u32).to_le_bytes());
                }
            }
        }
        b.extend_from_slice(&self.terrain.rng_state.to_le_bytes());
        for i in 0..self.terrain.height.len(){
            b.extend_from_slice(&self.terrain.height[i].to_le_bytes());
            b.push(self.terrain.class[i]);
            b.push(self.terrain.tile[i]);
            b.push(self.terrain.light[i]);
        }
        b.extend_from_slice(&self.consumption.food_fraction.to_le_bytes());
        for value in self.consumption.luxury_fraction{
            b.extend_from_slice(&value.to_le_bytes());
        }
        let mut race_mask=0u8;
        for (i,present) in self.races_present.iter().enumerate(){
            if *present{race_mask|=1<<i}
        }
        b.push(race_mask);
        b.extend_from_slice(&(self.sites.len() as u32).to_le_bytes());
        for (&id,site) in &self.sites{
            b.extend_from_slice(&id.to_le_bytes());
            for &v in &site.remaining{b.extend_from_slice(&v.to_le_bytes())}
            for &v in &site.stock{b.extend_from_slice(&v.to_le_bytes())}
            b.extend_from_slice(&site.work_done.to_le_bytes());
            b.extend_from_slice(&site.work_total.to_le_bytes());
            b.push(u8::from(site.ready_for_work));
        }
        b
    }
    pub fn load_snapshot(&mut self,b:&[u8])->Result<(),GameError>{
        if b.len()<8+8+4+9*4+4+4{
            return Err(GameError::Invalid("invalid snapshot header"))
        }
        let legacy=&b[..8]==b"TLWSNAP1";
        let version6=&b[..8]==b"TLWSNAP6";
        let version5=&b[..8]==b"TLWSNAP5"||version6;
        let version4=&b[..8]==b"TLWSNAP4"||version5;
        let version3=&b[..8]==b"TLWSNAP3"||version4;
        if !legacy && !version3 && &b[..8]!=b"TLWSNAP2"{
            return Err(GameError::Invalid("invalid snapshot header"))
        }
        let mut pos=8usize;
        let mut take=|n:usize|->Result<&[u8],GameError>{
            let end=pos.checked_add(n).ok_or(GameError::Invalid("snapshot overflow"))?;
            let value=b.get(pos..end).ok_or(GameError::Invalid("truncated snapshot"))?;
            pos=end;Ok(value)
        };
        let tick=u64::from_le_bytes(take(8)?.try_into().unwrap());
        let paused=if legacy {false}else{take(1)?[0]!=0};
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
                ||values[5]>=self.terrain.size as i32{
                return Err(GameError::Invalid("entity out of bounds"))
            }
            let carrying=if legacy{0}else{
                i32::from_le_bytes(take(4)?.try_into().unwrap())
            };
            if carrying<0{return Err(GameError::Invalid("negative carried goods"))}
            let facing=if version3 {take(1)?[0]} else {4};
            if facing>=8{return Err(GameError::Invalid("invalid facing direction"))}
            let task=if legacy{Task::Idle}else{
                let tag=take(1)?[0];
                match tag{
                    0=>Task::Idle,
                    1=>{
                        let n=u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
                        if n>30_000{return Err(GameError::Invalid("snapshot path too long"))}
                        let mut steps=Vec::with_capacity(n);
                        for _ in 0..n{
                            let x=i32::from_le_bytes(take(4)?.try_into().unwrap());
                            let y=i32::from_le_bytes(take(4)?.try_into().unwrap());
                            if x<0||y<0||x>=self.terrain.size as i32
                                ||y>=self.terrain.size as i32{
                                return Err(GameError::Invalid("path step out of bounds"))
                            }
                            steps.push((x,y));
                        }
                        let after=match take(1)?[0]{
                            0=>After::Idle,
                            1=>After::Gather(u32::from_le_bytes(take(4)?.try_into().unwrap())),
                            2=>{
                                let id=u32::from_le_bytes(take(4)?.try_into().unwrap());
                                let good=u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
                                let amount=i32::from_le_bytes(take(4)?.try_into().unwrap());
                                if good>=9||amount<0{return Err(GameError::Invalid("invalid cargo"))}
                                After::Deliver(id,good,amount)
                            }
                            3=>After::Construct(u32::from_le_bytes(take(4)?.try_into().unwrap())),
                            4=>{
                                let good=u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
                                let amount=i32::from_le_bytes(take(4)?.try_into().unwrap());
                                if good>=9||amount<0{return Err(GameError::Invalid("invalid cargo"))}
                                After::Hold(good,amount)
                            }
                            5 if version6=>{
                                let site=u32::from_le_bytes(take(4)?.try_into().unwrap());
                                let source=u32::from_le_bytes(take(4)?.try_into().unwrap());
                                let good=u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
                                if good>=9{return Err(GameError::Invalid("invalid fetch good"))}
                                After::Fetch(site,source,good)
                            }
                            _=>return Err(GameError::Invalid("unknown after-task tag"))
                        };
                        Task::Walking{steps,after}
                    }
                    2=>{
                        let tree=u32::from_le_bytes(take(4)?.try_into().unwrap());
                        let time=u16::from_le_bytes(take(2)?.try_into().unwrap());
                        if time>=60{return Err(GameError::Invalid("invalid harvest progress"))}
                        Task::Gathering{tree,time}
                    }
                    3=>{
                        let storage=u32::from_le_bytes(take(4)?.try_into().unwrap());
                        let good=u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
                        let amount=i32::from_le_bytes(take(4)?.try_into().unwrap());
                        if good>=9||amount<0{return Err(GameError::Invalid("invalid cargo"))}
                        Task::Delivering{storage,good,amount}
                    }
                    4=>Task::Constructing{
                        site:u32::from_le_bytes(take(4)?.try_into().unwrap())
                    },
                    5=>{
                        let good=u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
                        let amount=i32::from_le_bytes(take(4)?.try_into().unwrap());
                        if good>=9||amount<0{return Err(GameError::Invalid("invalid cargo"))}
                        Task::Holding{good,amount}
                    }
                    6 if version6=>{
                        let site=u32::from_le_bytes(take(4)?.try_into().unwrap());
                        let storage=u32::from_le_bytes(take(4)?.try_into().unwrap());
                        let good=u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
                        if good>=9{return Err(GameError::Invalid("invalid fetch good"))}
                        Task::Fetching{site,storage,good}
                    }
                    _=>return Err(GameError::Invalid("unknown task tag"))
                }
            };
            objects.push(Entity{
                id,kind:values[0],group:values[1],object_type:values[2],
                owner:values[3],x:values[4],y:values[5],hp:values[6],
                complete,construction,task,carrying,facing
            });
        }
        let saved_terrain=if version4{
            let rng=u32::from_le_bytes(take(4)?.try_into().unwrap());
            let count=self.terrain.height.len();
            let mut height=Vec::with_capacity(count);
            let mut class=Vec::with_capacity(count);
            let mut tile=Vec::with_capacity(count);
            let mut light=Vec::with_capacity(count);
            for _ in 0..count{
                height.push(u16::from_le_bytes(take(2)?.try_into().unwrap()));
                let cls=take(1)?[0];
                if cls>self.terrain.water_class+1{
                    return Err(GameError::Invalid("invalid terrain class"))
                }
                class.push(cls);
                tile.push(take(1)?[0]);
                light.push(take(1)?[0]);
            }
            Some((rng,height,class,tile,light))
        }else{None};
        let saved_economy=if version5{
            let food=f32::from_le_bytes(take(4)?.try_into().unwrap());
            let mut lux=[0f32;3];
            for value in &mut lux{
                *value=f32::from_le_bytes(take(4)?.try_into().unwrap())
            }
            if ![food,lux[0],lux[1],lux[2]].iter().all(|v|
                v.is_finite()&&*v>=0.0&&*v<1.0
            ){
                return Err(GameError::Invalid("invalid economy fraction"))
            }
            let mask=take(1)?[0];
            if mask&!7!=0{return Err(GameError::Invalid("invalid race mask"))}
            let races=std::array::from_fn(|i|mask&(1<<i)!=0);
            Some((Consumption{
                food_fraction:food,luxury_fraction:lux
            },races))
        }else{None};
        let saved_sites=if version6{
            let count=u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
            if count>MAX_ENTITIES{return Err(GameError::Invalid("too many construction sites"))}
            let mut sites=BTreeMap::new();
            for _ in 0..count{
                let id=u32::from_le_bytes(take(4)?.try_into().unwrap());
                if !objects.iter().any(|o|o.id==id&&o.kind==1&&!o.complete)
                    ||sites.contains_key(&id){
                    return Err(GameError::Invalid("invalid construction site"))
                }
                let mut remaining=[0i32;9];
                let mut stock=[0i32;9];
                for v in &mut remaining{
                    *v=i32::from_le_bytes(take(4)?.try_into().unwrap());
                }
                for v in &mut stock{
                    *v=i32::from_le_bytes(take(4)?.try_into().unwrap());
                }
                if remaining.iter().chain(stock.iter()).any(|&v|
                    !(0..=100_000).contains(&v)
                ){return Err(GameError::Invalid("invalid construction stock"))}
                let work_done=u16::from_le_bytes(take(2)?.try_into().unwrap());
                let work_total=u16::from_le_bytes(take(2)?.try_into().unwrap());
                let ready_for_work=match take(1)?[0]{
                    0=>false,1=>true,_=>return Err(GameError::Invalid("invalid site phase"))
                };
                if work_total<10||work_total>10_000||work_done>work_total{
                    return Err(GameError::Invalid("invalid site work progress"))
                }
                sites.insert(id,SiteLogistics{
                    remaining,stock,work_done,work_total,ready_for_work
                });
            }
            Some(sites)
        }else{None};
        if pos!=b.len()||next_id<=ids.iter().copied().max().unwrap_or(0){
            return Err(GameError::Invalid("snapshot trailing data or invalid next id"))
        }
        self.tick=tick;self.gold=gold;self.goods=goods;self.next_id=next_id;
        self.objects=objects;self.paused=paused;
        self.sites=if let Some(sites)=saved_sites{sites}else{
            // Legacy TLWSNAP1-5 have no physical site-stock ledger.
            // Their placement had already deducted all material costs.
            // Reconstruct an in-site stock from the unfinished amount
            // so their previously saved builder tasks keep working.
            let mut sites=BTreeMap::new();
            for obj in &self.objects{
                if obj.kind!=1||obj.complete{continue}
                let cost=self.rules.get(&rule_id(obj.group,obj.object_type))
                    .map_or([0;9],|r|r.cost);
                let mut entry=SiteLogistics::new(cost);
                entry.work_done=((obj.construction as u32
                    *u32::from(entry.work_total)+50)/100)
                    .min(u32::from(entry.work_total)) as u16;
                let mut consumed=i32::from(entry.work_done);
                for good in 0..9{
                    let used=consumed.min(entry.remaining[good]);
                    entry.remaining[good]-=used;
                    consumed-=used;
                    entry.stock[good]=entry.remaining[good];
                }
                sites.insert(obj.id,entry);
            }
            sites
        };
        if let Some((consumption,races))=saved_economy{
            self.consumption=consumption;
            self.races_present=races;
        }else{
            self.consumption=Consumption::default();
        }
        if let Some((rng,height,class,tile,light))=saved_terrain{
            self.terrain.rng_state=rng;
            self.terrain.height=height;
            self.terrain.class=class;
            self.terrain.tile=tile;
            self.terrain.light=light;
        }
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
        // Resources remain with the worker until a real storage exists.
        assert_eq!(sim.goods[1],0);
        assert_eq!(sim.entity(1).unwrap().carrying,1);
        assert!(matches!(sim.entity(1).unwrap().task,Task::Holding{..}));
        let storage=sim.next_id;
        sim.next_id+=1;
        sim.objects.push(Entity{
            id:storage,kind:1,group:2,object_type:40,owner:2,
            x:9,y:10,hp:100,complete:true,construction:100,
            task:Task::Idle,carrying:0,facing:4
        });
        for _ in 0..180{sim.step()}
        assert_eq!(sim.goods[1],1);
        assert_eq!(sim.entity(1).unwrap().carrying,0);
        let bytes=sim.save_snapshot();
        let mut loaded=setup();
        loaded.load_snapshot(&bytes).unwrap();
        assert_eq!(loaded.goods[1],1);
        assert_eq!(loaded.tick,sim.tick);
        assert_eq!(loaded.objects.len(),sim.objects.len());
    }
    fn house_rule(ty:i32)->ObjectRule{
        let mut cost=[0;9];cost[GOOD_WOOD]=3;cost[GOOD_STONE]=2;
        ObjectRule{
            id:rule_id(2,ty),group:2,kind:1,object_type:ty,
            name:"House".into(),size_x:2,size_y:2,door_dx:0,door_dy:0,
            hp:200,radius:0,residents:0,flags:0,class:0,
            step_ticks:[0;4],money:0,operator:0,invention:0,made_at:0,
            cost,product:0,amount:0,weapon:0
        }
    }
    #[test]
    fn building_waits_for_materials_and_does_not_precharge_goods(){
        let mut sim=setup();
        sim.entity_mut(1).unwrap().object_type=11;
        sim.rules.insert(rule_id(2,37),house_rule(37));
        sim.goods[GOOD_WOOD]=3;sim.goods[GOOD_STONE]=2;
        let site=sim.build(1,rule_id(2,37),(25,25)).unwrap();
        assert_eq!(sim.goods[GOOD_WOOD],3);
        assert_eq!(sim.goods[GOOD_STONE],2);
        assert_eq!(sim.can_build(1,rule_id(2,37),(30,30)),Err(GameError::Busy));
        for _ in 0..400{sim.step()}
        assert_eq!(sim.entity(site).unwrap().construction,0);
        let snap=sim.save_snapshot();
        let mut restored=setup();
        restored.rules=sim.rules.clone();
        restored.load_snapshot(&snap).unwrap();
        assert_eq!(restored.entity(1).unwrap().task,sim.entity(1).unwrap().task);
        assert_eq!(restored.sites.get(&site),sim.sites.get(&site));
        // A simulated external porter has now delivered the materials.
        restored.sites.get_mut(&site).unwrap().stock[GOOD_WOOD]=3;
        restored.sites.get_mut(&site).unwrap().stock[GOOD_STONE]=2;
        for _ in 0..500{restored.step()}
        assert!(restored.entity(site).unwrap().complete);
        assert_eq!(restored.entity(site).unwrap().construction,100);
        assert_eq!(restored.can_build(1,rule_id(2,37),(25,25)),Err(GameError::Occupied));
        // Unlike the old prototype, the original allows placement with
        // insufficient goods; the site will wait until supply arrives.
        assert_eq!(restored.can_build(1,rule_id(2,37),(30,30)),Ok(()));
    }
    #[test]
    fn porter_collects_exactly_one_unit_from_storage_and_delivers_it(){
        let mut sim=setup();
        sim.entity_mut(1).unwrap().object_type=11;
        let mut house=house_rule(37);
        house.cost=[0;9];house.cost[GOOD_WOOD]=1;
        sim.rules.insert(rule_id(2,37),house);
        let store_id=sim.next_id;sim.next_id+=1;
        sim.objects.push(Entity{
            id:store_id,kind:1,group:2,object_type:40,owner:2,
            x:8,y:10,hp:100,complete:true,construction:100,
            task:Task::Idle,carrying:0,facing:4
        });
        let porter_id=sim.next_id;sim.next_id+=1;
        sim.objects.push(Entity{
            id:porter_id,kind:0,group:2,object_type:9,owner:2,
            x:7,y:10,hp:100,complete:true,construction:100,
            task:Task::Idle,carrying:0,facing:4
        });
        sim.goods[GOOD_WOOD]=1;
        let site=sim.build(1,rule_id(2,37),(17,17)).unwrap();
        assert_eq!(sim.goods[GOOD_WOOD],1);
        let mut observed_in_transit=false;
        let mut observed_delivered=false;
        for _ in 0..1200{
            sim.step();
            if sim.entity(porter_id).is_some_and(|o|o.carrying==1){
                observed_in_transit=true;
            }
            if sim.sites.get(&site).is_some_and(|s|s.stock[GOOD_WOOD]==1){
                observed_delivered=true;
            }
            if sim.entity(site).unwrap().complete{break}
        }
        assert!(observed_in_transit,"porter never picked up the wood");
        assert!(observed_delivered,"wood never reached construction site");
        assert!(sim.entity(site).unwrap().complete,"builder never finished");
        assert_eq!(sim.goods[GOOD_WOOD],0);
        assert_eq!(sim.entity(porter_id).unwrap().carrying,0);
        assert!(!sim.sites.contains_key(&site));
    }
    #[test]
    fn only_original_builder_profession_can_place_buildings(){
        let mut sim=setup();
        sim.rules.insert(rule_id(2,37),house_rule(37));
        sim.goods[GOOD_WOOD]=3;
        sim.goods[GOOD_STONE]=2;
        assert_eq!(
            sim.build(1,rule_id(2,37),(25,25)),
            Err(GameError::WrongWorker)
        );
        sim.entity_mut(1).unwrap().object_type=11;
        let site=sim.build(1,rule_id(2,37),(25,25)).unwrap();
        assert!(sim.entity(site).unwrap().hp<200);
    }
    #[test]
    fn duplicate_town_halls_are_blocked_until_settlements_are_supported(){
        let mut sim=setup();
        sim.entity_mut(1).unwrap().object_type=11;
        let mut hall=house_rule(40);
        hall.cost=[0;9];
        sim.rules.insert(rule_id(2,40),hall);
        sim.build(1,rule_id(2,40),(25,25)).unwrap();
        for _ in 0..400{sim.step()}
        assert_eq!(sim.can_build(1,rule_id(2,40),(28,28)),Err(GameError::TownHallExists));
    }
    #[test]
    fn player_can_release_worker_when_no_storage_exists(){
        let mut sim=setup();
        sim.order_gather(1,2).unwrap();
        for _ in 0..100{sim.step()}
        assert!(matches!(sim.entity(1).unwrap().task,Task::Holding{..}));
        sim.drop_cargo(1).unwrap();
        assert_eq!(sim.entity(1).unwrap().carrying,0);
        assert_eq!(sim.entity(1).unwrap().task,Task::Idle);
        assert_eq!(sim.goods[GOOD_WOOD],0);
    }
    #[test]
    fn idle_fidgets_do_not_interfere_with_active_worker_jobs(){
        let mut sim=setup();
        let start=sim.entity(1).unwrap().facing;
        assert!(unit_idle_phase(1,0)<300);
        for _ in 0..600{sim.step()}
        assert!(sim.entity(1).unwrap().facing<8);
        // A fixed movement order always takes control over idle fidgets.
        sim.order_move(1,(11,10)).unwrap();
        for _ in 0..6{sim.step()}
        assert_eq!(sim.entity(1).unwrap().facing,5);
        assert!(start<8);
    }
    #[test]
    fn wild_animals_wander_without_touching_wood_or_berries(){
        let mut sim=setup();
        let mut rule=house_rule(33);
        rule.kind=2;rule.group=6;rule.id=rule_id(6,33);
        rule.class=7;
        sim.rules.insert(rule.id,rule);
        sim.objects.push(Entity{
            id:3,kind:2,group:6,object_type:33,owner:-1,
            x:35,y:35,hp:50,complete:true,construction:100,
            task:Task::Idle,carrying:0,facing:4
        });
        let before=(sim.entity(3).unwrap().x,sim.entity(3).unwrap().y);
        for _ in 0..240{sim.step()}
        let animal=sim.entity(3).unwrap();
        assert!(!sim.terrain.water(animal.x,animal.y));
        assert_ne!((animal.x,animal.y),before);
        assert!(animal.facing<8);
        // Static wood resource was not harvested by an animal.
        assert!(sim.entity(2).is_some());
    }
    #[test]
    fn facing_uses_original_eight_directions_and_survives_snapshot(){
        assert_eq!(facing_for_delta(-1,-1),Some(0));
        assert_eq!(facing_for_delta(-1,0),Some(1));
        assert_eq!(facing_for_delta(-1,1),Some(2));
        assert_eq!(facing_for_delta(0,1),Some(3));
        assert_eq!(facing_for_delta(1,1),Some(4));
        assert_eq!(facing_for_delta(1,0),Some(5));
        assert_eq!(facing_for_delta(1,-1),Some(6));
        assert_eq!(facing_for_delta(0,-1),Some(7));
        assert_eq!(facing_for_delta(0,0),None);
        assert_eq!(wrapped_delta((0,0),(127,0),128),(-1,0));
        let mut sim=setup();
        sim.order_move(1,(11,10)).unwrap();
        for _ in 0..6{sim.step()}
        assert_eq!(sim.entity(1).unwrap().facing,5);
        let bytes=sim.save_snapshot();
        let mut loaded=setup();
        loaded.load_snapshot(&bytes).unwrap();
        assert_eq!(loaded.entity(1).unwrap().facing,5);
        sim.order_move(1,(11,11)).unwrap();
        for _ in 0..6{sim.step()}
        assert_eq!(sim.entity(1).unwrap().facing,3);
    }
    #[test]
    fn snapshot_preserves_active_gather_task(){
        let mut sim=setup();
        sim.order_gather(1,2).unwrap();
        for _ in 0..12{sim.step()}
        let state=sim.entity(1).unwrap().task.clone();
        let bytes=sim.save_snapshot();
        let mut loaded=setup();
        loaded.load_snapshot(&bytes).unwrap();
        assert_eq!(loaded.entity(1).unwrap().task,state);
    }
    #[test]
    fn builder_cannot_harvest_wood_but_woodcutter_can(){
        let mut sim=setup();
        sim.entity_mut(1).unwrap().object_type=11;
        assert_eq!(sim.order_gather(1,2),Err(GameError::WrongWorker));
        sim.entity_mut(1).unwrap().object_type=19;
        assert!(sim.order_gather(1,2).is_ok());
    }
    #[test]
    fn original_economy_runs_after_every_299_ticks(){
        let mut sim=setup();
        sim.goods[4]=10;
        assert_eq!(sim.goods[4],10);
        for _ in 0..(299*4){
            sim.step();
        }
        assert_eq!(sim.goods[4],10);
        for _ in 0..299{sim.step();}
        assert_eq!(sim.goods[4],9);
        assert!(sim.consumption.food_fraction<0.01);
    }
    #[test]
    fn tlwsnap6_persists_economy_and_tlwsnap4_remains_readable(){
        let mut sim=setup();
        sim.races_present=[true,false,true];
        sim.consumption.food_fraction=0.4;
        sim.consumption.luxury_fraction=[0.1,0.2,0.3];
        let snapshot=sim.save_snapshot();
        assert!(snapshot.starts_with(b"TLWSNAP6"));
        let mut loaded=setup();
        loaded.load_snapshot(&snapshot).unwrap();
        assert_eq!(loaded.races_present,[true,false,true]);
        assert_eq!(loaded.consumption,sim.consumption);
        let mut legacy=snapshot.clone();
        legacy[..8].copy_from_slice(b"TLWSNAP4");
        legacy.truncate(legacy.len()-21);
        let mut old_loaded=setup();
        old_loaded.load_snapshot(&legacy).unwrap();
        assert_eq!(old_loaded.consumption,Consumption::default());
    }
    #[test]
    fn snapshot_v6_restores_paved_foundations_and_rng_state(){
        let mut sim=setup();
        sim.terrain.foundation(25,25,3,2);
        let snapshot=sim.save_snapshot();
        assert!(snapshot.starts_with(b"TLWSNAP6"));
        let mut loaded=setup();
        loaded.load_snapshot(&snapshot).unwrap();
        assert_eq!(loaded.terrain.height,sim.terrain.height);
        assert_eq!(loaded.terrain.class,sim.terrain.class);
        assert_eq!(loaded.terrain.tile,sim.terrain.tile);
        assert_eq!(loaded.terrain.light,sim.terrain.light);
        assert_eq!(loaded.terrain.rng_state,sim.terrain.rng_state);
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
