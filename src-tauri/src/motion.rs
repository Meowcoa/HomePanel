//! Short, finite content transitions. Native window dimensions and glass stay still.
#[derive(Default)]
pub struct Transition { key:Option<u64>, started:f64 }

#[derive(Clone,Copy,Debug)]
pub struct Sample { pub opacity:f32, pub offset:f32, pub active:bool }

impl Transition {
    pub fn restart(&mut self) {self.key=None;}
    pub fn sample(&mut self,key:u64,now:f64,reduced:bool)->Sample {
        if self.key!=Some(key) {self.key=Some(key);self.started=now;}
        let t=if reduced {1.0}else{((now-self.started)/0.20).clamp(0.0,1.0) as f32};
        let eased=1.0-(1.0-t).powi(3);
        Sample{opacity:eased,offset:8.0*(1.0-eased),active:t<1.0}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transitions_finish_without_rescaling_and_restart_for_navigation() {
        let mut motion=Transition::default();
        let start=motion.sample(1,10.0,false);assert_eq!(start.opacity,0.0);assert_eq!(start.offset,8.0);
        let half=motion.sample(1,10.1,false);assert!(half.opacity>0.5 && half.opacity<1.0);assert!(half.offset>0.0 && half.offset<4.0);
        let end=motion.sample(1,10.21,false);assert_eq!(end.opacity,1.0);assert_eq!(end.offset,0.0);assert!(!end.active);
        assert!(motion.sample(2,11.0,false).active);
        motion.restart();assert!(motion.sample(2,12.0,false).active);
    }
    #[test]
    fn reduced_motion_is_immediate_and_never_requests_animation_frames() {
        let mut motion=Transition::default();
        for key in [1,2,3] {let frame=motion.sample(key,10.0,true);assert_eq!(frame.opacity,1.0);assert_eq!(frame.offset,0.0);assert!(!frame.active);}
    }
}
