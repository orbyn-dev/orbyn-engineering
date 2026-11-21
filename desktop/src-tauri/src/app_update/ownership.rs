//! Shell-owned exclusion outlives a service process and the task awaiting an installer.
#[derive(Default)]
pub(super) struct Ownership {
    generation: u64,
    alive: bool,
    effect: Option<u64>,
    sequence: u64,
    deferred: bool,
    stopping: bool,
}
impl Ownership {
    pub(super) fn current(&self, generation: u64) -> bool {
        self.alive && self.generation == generation && !self.stopping
    }
    pub(super) fn busy(&self) -> bool {
        self.effect.is_some()
    }
    pub(super) fn owns(&self, generation: u64, effect: u64) -> bool {
        self.generation == generation && self.effect == Some(effect)
    }
    pub(super) fn request_restart(&mut self) -> bool {
        if self.stopping {
            return false;
        }
        if self.busy() {
            self.deferred = true;
            return false;
        }
        true
    }
    pub(super) fn attach(&mut self, generation: u64) -> bool {
        if self.busy() || self.stopping {
            return false;
        }
        self.generation = generation;
        self.alive = true;
        self.deferred = false;
        true
    }
    pub(super) fn begin(&mut self, generation: u64) -> Option<u64> {
        if !self.current(generation) || self.busy() {
            return None;
        }
        self.sequence = self.sequence.checked_add(1)?;
        self.effect = Some(self.sequence);
        self.effect
    }
    pub(super) fn exited(&mut self, generation: u64) {
        if self.generation == generation {
            self.alive = false;
        }
    }
    pub(super) fn finish(&mut self, generation: u64, effect: u64) -> bool {
        if self.generation != generation || self.effect != Some(effect) {
            return false;
        }
        self.effect = None;
        let restart = self.deferred && !self.stopping;
        self.deferred = false;
        restart
    }
    pub(super) fn shutdown(&mut self) {
        self.stopping = true;
        self.alive = false;
        self.deferred = false;
    }
}
/// The effect owns settlement, including queued-task cancellation and unwinding.
pub(super) struct Settlement<F: FnOnce(bool)> {
    settle: Option<F>,
    reported: bool,
}
impl<F: FnOnce(bool)> Settlement<F> {
    pub(super) fn new(settle: F) -> Self {
        Self {
            settle: Some(settle),
            reported: false,
        }
    }
    pub(super) fn reported(&mut self) {
        self.reported = true;
    }
}
impl<F: FnOnce(bool)> Drop for Settlement<F> {
    fn drop(&mut self) {
        if let Some(settle) = self.settle.take() {
            settle(self.reported);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, mpsc};
    #[test]
    fn a_blocking_installer_defers_replacement_core_until_it_really_settles() {
        let owner = Arc::new(Mutex::new(Ownership::default()));
        owner.lock().unwrap().attach(1);
        let effect = owner.lock().unwrap().begin(1).unwrap();
        let (finish, wait) = mpsc::channel();
        let cloned = owner.clone();
        let installer = std::thread::spawn(move || {
            wait.recv().unwrap();
            cloned.lock().unwrap().finish(1, effect)
        });
        owner.lock().unwrap().exited(1);
        let allowed = owner.lock().unwrap().request_restart();
        assert!(!owner.lock().unwrap().attach(2));
        assert!(owner.lock().unwrap().owns(1, effect));
        finish.send(()).unwrap();
        let deferred = installer.join().unwrap();
        assert!(
            !allowed,
            "a core crash must not admit generation 2 while installer generation 1 is blocked"
        );
        assert!(deferred);
        let mut state = owner.lock().unwrap();
        assert!(state.request_restart());
        state.attach(2);
        assert!(!state.current(1));
        assert!(state.begin(1).is_none());
        assert!(!state.finish(1, effect));
        assert!(state.current(2));
    }
    #[test]
    fn stopping_revokes_deferred_restart_and_late_completion_is_inert() {
        let mut state = Ownership::default();
        state.attach(1);
        let effect = state.begin(1).unwrap();
        state.exited(1);
        assert!(!state.request_restart());
        state.shutdown();
        assert!(!state.finish(1, effect));
        assert!(!state.request_restart());
    }
}

#[cfg(test)]
mod settlement_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn cancellation_and_blocking_panic_settle_the_exact_effect() {
        for panic in [false, true] {
            let owner = Arc::new(Mutex::new(Ownership::default()));
            owner.lock().unwrap().attach(1);
            let effect = owner.lock().unwrap().begin(1).unwrap();
            owner.lock().unwrap().exited(1);
            assert!(!owner.lock().unwrap().request_restart());
            let copy = owner.clone();
            let guard = Settlement::new(move |_| {
                assert!(copy.lock().unwrap().finish(1, effect));
            });
            if panic {
                assert!(
                    std::thread::spawn(move || {
                        let _guard = guard;
                        panic!("fake installer");
                    })
                    .join()
                    .is_err()
                );
            } else {
                let future = async move {
                    let _guard = guard;
                    std::future::pending::<()>().await;
                };
                drop(future);
            }
            assert!(!owner.lock().unwrap().busy());
            assert!(owner.lock().unwrap().request_restart());
        }
    }
}

#[cfg(test)]
mod completion_test {
    use super::*;
    #[test]
    fn explicit_outcome_is_settled_once() {
        let mut guard = Settlement::new(|reported| assert!(reported));
        guard.reported();
        drop(guard);
    }
}
