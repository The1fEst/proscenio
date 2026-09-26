use std::any::Any;
use std::cell::RefCell;

use crate::core::listeners::Subscription;

#[derive(Default)]
pub struct Scope {
    subscriptions: RefCell<Vec<Subscription>>,
    held: RefCell<Vec<Box<dyn Any>>>,
    deferred: RefCell<Vec<Box<dyn FnOnce()>>>,
}

impl Scope {
    pub fn keep(&self, subscription: Subscription) {
        self.subscriptions.borrow_mut().push(subscription);
    }

    pub fn hold(&self, value: impl Any) {
        self.held.borrow_mut().push(Box::new(value));
    }

    pub fn defer(&self, action: impl FnOnce() + 'static) {
        self.deferred.borrow_mut().push(Box::new(action));
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        self.subscriptions.take();
        for action in self.deferred.take() {
            action();
        }
        self.held.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::listeners::Listeners;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn ending_a_scope_unsubscribes_runs_what_was_deferred_and_lets_go_of_what_it_held() {
        let listeners = Listeners::default();
        let heard = Rc::new(Cell::new(0));
        let deferred = Rc::new(Cell::new(false));
        let held = Rc::new(());
        let scope = Scope::default();
        scope.keep(listeners.add({
            let heard = heard.clone();
            move || heard.set(heard.get() + 1)
        }));
        scope.defer({
            let deferred = deferred.clone();
            move || deferred.set(true)
        });
        scope.hold(held.clone());
        listeners.notify();
        drop(scope);
        listeners.notify();
        assert_eq!(
            (heard.get(), deferred.get(), Rc::strong_count(&held)),
            (1, true, 1)
        );
    }
}
