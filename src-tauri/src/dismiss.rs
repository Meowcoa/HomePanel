use std::time::{Duration, Instant};

/// Arm after this window group gains focus; brief native menu transitions are ignored.
#[derive(Default)]
pub struct DismissGuard {
    armed: bool,
    lost_at: Option<Instant>,
}

impl DismissGuard {
    pub const DELAY: Duration = Duration::from_millis(160);
    pub fn reset(&mut self) { *self = Self::default(); }
    pub fn waiting(&self) -> bool { self.lost_at.is_some() }
    pub fn observe(&mut self, visible: bool, own_foreground: Option<bool>, now: Instant) -> bool {
        if !visible { self.reset(); return false; }
        match own_foreground {
            Some(true) => { self.armed = true; self.lost_at = None; }
            Some(false) if self.armed => {
                let lost = self.lost_at.get_or_insert(now);
                return now.saturating_duration_since(*lost) >= Self::DELAY;
            }
            // No foreground window can occur during a menu handoff or desktop transition.
            _ => { self.lost_at = None; }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outside_focus_dismisses_after_grace_and_owned_focus_keeps_panel_open() {
        let now=Instant::now();let mut guard=DismissGuard::default();
        assert!(!guard.observe(true,Some(false),now)); // initial focus request still pending
        assert!(!guard.observe(true,Some(true),now));
        assert!(!guard.observe(true,Some(false),now));
        assert!(guard.waiting());
        assert!(!guard.observe(true,Some(false),now+Duration::from_millis(100)));
        assert!(guard.observe(true,Some(false),now+Duration::from_millis(160)));
        assert!(!guard.observe(true,Some(true),now+Duration::from_millis(170)));
        assert!(!guard.waiting());
    }
    #[test]
    fn menus_unknown_focus_and_reopening_do_not_reuse_old_blur_state() {
        let now=Instant::now();let mut guard=DismissGuard::default();
        guard.observe(true,Some(true),now);guard.observe(true,Some(false),now);
        assert!(!guard.observe(true,None,now+Duration::from_secs(1)));
        assert!(!guard.observe(true,Some(true),now+Duration::from_secs(2)));
        guard.observe(true,Some(false),now+Duration::from_secs(3));
        assert!(!guard.observe(false,Some(false),now+Duration::from_secs(4)));
        assert!(!guard.observe(true,Some(false),now+Duration::from_secs(5)));
    }
}
