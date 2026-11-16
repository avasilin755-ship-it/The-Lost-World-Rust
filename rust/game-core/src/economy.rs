//! Confirmed town economy primitives from docs/game/economy.md §§1.5, 5.1–5.2, 7.
//!
//! This is the source-accurate float32 food/luxury consumption formula and
//! shipped town-hall / warehouse capacity. Full per-town market, service,
//! research, taxes and AI remain separate ports.

pub const ECONOMY_ROUND_TICKS:u64=299;
pub const FOOD_GOOD:usize=4;
pub const LUXURY_FIRST:usize=6;
pub const STORE_CAPACITY_PER_BUILDING:i32=25;

#[derive(Debug,Clone,Copy,Default,PartialEq)]
pub struct Consumption {
    /// Original job +0x20 float32 fractional amount carried to the next round.
    pub food_fraction:f32,
    /// Original job +0x24..+0x2c float32 fractions of race luxuries.
    pub luxury_fraction:[f32;3],
}
#[derive(Debug,Clone,Copy,Default,PartialEq,Eq)]
pub struct ConsumptionReport{
    pub food_used:i32,
    pub luxury_used:[i32;3],
}
impl Consumption {
    /// A round is invoked after 299 scheduler ticks, not every frame.
    /// Uses the original game's single-precision fractional accumulation.
    pub fn round(
        &mut self,population:usize,town_race:usize,
        races_present:[bool;3],stock:&mut[i32;9],
    )->ConsumptionReport {
        let mut report=ConsumptionReport::default();
        if population==0{return report}
        // The original accumulates fractional food needs, then uses the
        // updated A (before subtraction) to calculate luxury demand.
        self.food_fraction+=population as f32*0.2_f32;
        let total=self.food_fraction;
        let wanted_food=total.trunc() as i32;
        let mut foreign_multiplier=1.5_f32;
        for race in 0..3 {
            if !races_present[race]{continue}
            let threshold=if race==town_race {30}
            else {
                let cutoff=(30.0_f32*foreign_multiplier).trunc() as usize;
                foreign_multiplier+=1.0;
                cutoff
            };
            if population<threshold{continue}
            self.luxury_fraction[race]+=total*0.0625_f32;
            let wanted=self.luxury_fraction[race].trunc() as i32;
            if wanted>0{
                self.luxury_fraction[race]-=wanted as f32;
                let good=LUXURY_FIRST+race;
                let have=stock[good].max(0);
                report.luxury_used[race]=wanted.min(have);
                stock[good]-=report.luxury_used[race];
            }
        }
        if wanted_food>0{
            self.food_fraction-=wanted_food as f32;
            let have=stock[FOOD_GOOD].max(0);
            report.food_used=wanted_food.min(have);
            stock[FOOD_GOOD]-=report.food_used;
        }
        report
    }
}

/// Each completed town hall and completed warehouse holds 25 units of
/// EACH good. Construction sites have zero storage capacity.
pub fn storage_capacity(
    objects:impl IntoIterator<Item=(i32,i32,bool)>,race:i32
)->i32{
    objects.into_iter().filter(|&(group,ty,complete)|
        group==race && complete && (ty==40||ty==41)
    ).fold(0i32,|acc,_|acc.saturating_add(STORE_CAPACITY_PER_BUILDING))
}
pub fn storage_room(capacity:i32,stored:i32)->i32{
    capacity.saturating_sub(stored).max(0)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn food_fraction_matches_original_rounding(){
        let mut c=Consumption::default();
        let mut goods=[0;9];goods[FOOD_GOOD]=10;
        let one=c.round(7,2,[true,true,true],&mut goods);
        let two=c.round(7,2,[true,true,true],&mut goods);
        let three=c.round(7,2,[true,true,true],&mut goods);
        assert_eq!((one.food_used,two.food_used,three.food_used),(1,1,2));
        assert_eq!(goods[FOOD_GOOD],6);
        assert!(c.food_fraction>0.19&&c.food_fraction<0.21);
    }
    #[test]
    fn no_food_cannot_be_consumed_below_zero(){
        let mut c=Consumption::default();
        let mut goods=[0;9];
        for _ in 0..5{c.round(10,1,[true,true,true],&mut goods);}
        assert_eq!(goods[FOOD_GOOD],0);
        assert!(c.food_fraction>=0.0&&c.food_fraction<1.0);
    }
    #[test]
    fn foreign_luxury_thresholds_follow_participating_race_order(){
        let mut c=Consumption::default();
        let mut goods=[0;9];goods[6]=20;goods[7]=20;goods[8]=20;
        // Sajiki town: own threshold30, Amazon45, Pimmon75.
        // At 60 population: Amazon and Sajiki luxuries consumed; Pimmon not.
        c.round(60,2,[true,true,true],&mut goods);
        let r=c.round(60,2,[true,true,true],&mut goods);
        assert_eq!(r.luxury_used[1],0);
        assert!(r.luxury_used[0]>0);
        assert!(r.luxury_used[2]>0);
    }
    #[test]
    fn building_storage_capacity_is_per_good_and_excludes_sites(){
        let objects=[
            (2,40,true),(2,41,true),(2,41,false),(1,40,true),
            (2,37,true),(2,41,true),
        ];
        let capacity=storage_capacity(objects,2);
        assert_eq!(capacity,75);
        assert_eq!(storage_room(capacity,20),55);
        assert_eq!(storage_room(capacity,80),0);
    }
}
