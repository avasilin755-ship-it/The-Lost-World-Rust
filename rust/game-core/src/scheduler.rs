//! Original Alien Nations global timing wheel, migrated from the scheduler
//! behaviour documented in docs/game/economy.md §1.3 and
//! docs/game/units-and-jobs.md §1.1.
//!
//! Runtime-agnostic and deterministic. Most full original job types have
//! not yet been migrated; clients can register their own typed JobKind.
//! A 600-slot wheel runs *newest scheduled job first* (LIFO).
use std::fmt;

pub const SLOT_COUNT:usize=600;
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]
pub enum JobKind{
    TownFood(u32),
    NationFinance(u8),
    TownStatus(u32),
    GoalCheck(u8),
    Custom(u32),
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct Job{
    pub id:u64,
    pub kind:JobKind,
    pub hold:u16,
    pub cancelled:bool,
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum WheelError{
    InvalidDelay,TooManyJobsInTick
}
impl fmt::Display for WheelError{
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{
        write!(f,"{self:?}")
    }
}
impl std::error::Error for WheelError{}
#[derive(Debug,Clone)]
pub struct TimingWheel{
    slots:Vec<Vec<Job>>,
    pub current:usize,
    pub tick:u64,
    next_id:u64,
}
impl Default for TimingWheel {
    fn default()->Self{Self::new()}
}
impl TimingWheel{
    pub fn new()->Self{
        Self{slots:vec![Vec::new();SLOT_COUNT],current:0,tick:0,next_id:1}
    }
    pub fn add(&mut self,kind:JobKind,delay:u16)->Result<u64,WheelError>{
        if usize::from(delay)>=SLOT_COUNT{
            return Err(WheelError::InvalidDelay)
        }
        let id=self.next_id;
        self.next_id+=1;
        self.schedule(Job{id,kind,hold:0,cancelled:false},delay);
        Ok(id)
    }
    fn schedule(&mut self,job:Job,delay:u16){
        let slot=(self.current+usize::from(delay))%SLOT_COUNT;
        self.slots[slot].push(job);
    }
    pub fn hold(&mut self,id:u64,ticks:u16)->Result<bool,WheelError>{
        if usize::from(ticks)>=SLOT_COUNT{
            return Err(WheelError::InvalidDelay)
        }
        for slot in &mut self.slots {
            if let Some(job)=slot.iter_mut().find(|job|job.id==id){
                job.hold=ticks;
                return Ok(true)
            }
        }
        Ok(false)
    }
    pub fn cancel(&mut self,id:u64)->bool{
        for slot in &mut self.slots{
            if let Some(job)=slot.iter_mut().find(|job|job.id==id){
                job.cancelled=true;
                return true
            }
        }
        false
    }
    pub fn jobs_pending(&self)->usize{
        self.slots.iter().map(Vec::len).sum()
    }
    /// The callback returns a reschedule delay. 0 means job finished.
    /// A callback may add or cancel jobs using other command paths
    /// *after* this step, since the callback intentionally receives only
    /// its job, not a reference to the mutable scheduling wheel.
    ///
    /// Original 0-delay insertions can re-run in the current tick; a
    /// generous per-step cap prevents malformed replay files hanging.
    pub fn step<F>(&mut self,mut run:F)->Result<usize,WheelError>
    where F:FnMut(&mut Job)->u16 {
        let mut count=0;
        while let Some(mut job)=self.slots[self.current].pop(){
            count+=1;
            if count>100_000 {
                self.slots[self.current].push(job);
                return Err(WheelError::TooManyJobsInTick)
            }
            if job.cancelled&&job.hold==0{continue}
            if job.hold>0{
                let hold=job.hold;
                // A cancelled, held job is still deferred in the original;
                // it is deleted only when the hold is released.
                self.schedule(job,hold);
                continue
            }
            let delay=run(&mut job);
            if delay>0{
                let actual=delay.max(job.hold);
                if usize::from(actual)>=SLOT_COUNT{
                    return Err(WheelError::InvalidDelay)
                }
                self.schedule(job,actual);
            }else if job.hold>0{
                let hold=job.hold;
                job.cancelled=true;
                self.schedule(job,hold);
            }
        }
        self.tick+=1;
        self.current=(self.current+1)%SLOT_COUNT;
        Ok(count)
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn same_slot_uses_original_lifo_order(){
        let mut wheel=TimingWheel::new();
        let a=wheel.add(JobKind::Custom(1),0).unwrap();
        let b=wheel.add(JobKind::Custom(2),0).unwrap();
        let mut seen=Vec::new();
        wheel.step(|job|{seen.push(job.id);0}).unwrap();
        assert_eq!(seen,vec![b,a]);
        assert_eq!(wheel.tick,1);
        assert_eq!(wheel.jobs_pending(),0);
    }
    #[test]
    fn periodic_work_runs_after_299_ticks(){
        let mut wheel=TimingWheel::new();
        let mut seen=Vec::new();
        wheel.add(JobKind::TownFood(7),299).unwrap();
        for _ in 0..600{
            wheel.step(|job|{
                seen.push(job.kind);
                299
            }).unwrap();
        }
        // Records can be generated only on the slots 299 and 598 in the
        // first 600 ticks, never every render frame.
        assert_eq!(seen.len(),2);
    }
    #[test]
    fn cancelled_or_held_jobs_do_not_execute_before_release(){
        let mut wheel=TimingWheel::new();
        let cancel=wheel.add(JobKind::Custom(1),1).unwrap();
        let delayed=wheel.add(JobKind::Custom(2),1).unwrap();
        assert!(wheel.cancel(cancel));
        assert!(wheel.hold(delayed,3).unwrap());
        let mut hits=Vec::new();
        for _ in 0..4{
            wheel.step(|j|{hits.push(j.kind);0}).unwrap();
        }
        assert!(hits.is_empty());
        assert!(wheel.hold(delayed,0).unwrap());
        for _ in 0..3{
            wheel.step(|j|{hits.push(j.kind);0}).unwrap();
        }
        assert_eq!(hits,vec![JobKind::Custom(2)]);
    }
    #[test]
    fn cancellation_waits_for_hold_to_be_released(){
        let mut wheel=TimingWheel::new();
        let id=wheel.add(JobKind::Custom(9),1).unwrap();
        wheel.hold(id,2).unwrap();
        wheel.cancel(id);
        for _ in 0..4{
            wheel.step(|_|panic!("cancelled held job must not execute")).unwrap();
        }
        assert_eq!(wheel.jobs_pending(),1);
        wheel.hold(id,0).unwrap();
        for _ in 0..5{
            wheel.step(|_|panic!("cancelled job must not execute")).unwrap();
        }
        assert_eq!(wheel.jobs_pending(),0);
    }
    #[test]
    fn callback_can_put_a_job_on_hold_before_deletion(){
        let mut wheel=TimingWheel::new();
        let id=wheel.add(JobKind::Custom(5),0).unwrap();
        wheel.step(|job|{
            job.hold=2;
            0
        }).unwrap();
        assert_eq!(wheel.jobs_pending(),1);
        wheel.hold(id,0).unwrap();
        for _ in 0..4{
            wheel.step(|_|panic!("cancelled job must not run again")).unwrap();
        }
        assert_eq!(wheel.jobs_pending(),0);
    }
    #[test]
    fn rejects_out_of_range_delays(){
        let mut wheel=TimingWheel::new();
        assert_eq!(wheel.add(JobKind::Custom(1),600),Err(WheelError::InvalidDelay));
    }
}
