use std::any::Any;
use std::cell::RefCell;

use crate::core::listeners::Subscription;

#[derive(Default)]
pub struct Scope {
    subscriptions: RefCell<Vec<Subscription>>,
    owned_values: RefCell<Vec<Box<dyn Any>>>,
    deferred_actions: RefCell<Vec<Box<dyn FnOnce()>>>,
}

impl Scope {
    pub fn keep(&self, subscription: Subscription) {
        self.subscriptions.borrow_mut().push(subscription);
    }

    pub fn hold(&self, value: impl Any) {
        self.owned_values.borrow_mut().push(Box::new(value));
    }

    pub fn defer(&self, action: impl FnOnce() + 'static) {
        self.deferred_actions.borrow_mut().push(Box::new(action));
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        self.subscriptions.take();
        for action in self.deferred_actions.take() {
            action();
        }
        self.owned_values.take();
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
        let heard_count = Rc::new(Cell::new(0));
        let has_run_deferred = Rc::new(Cell::new(false));
        let owned_value = Rc::new(());
        let scope = Scope::default();
        scope.keep(listeners.add({
            let heard_count = heard_count.clone();
            move || heard_count.set(heard_count.get() + 1)
        }));
        scope.defer({
            let has_run_deferred = has_run_deferred.clone();
            move || has_run_deferred.set(true)
        });
        scope.hold(owned_value.clone());
        listeners.notify();
        drop(scope);
        listeners.notify();
        assert_eq!(
            (
                heard_count.get(),
                has_run_deferred.get(),
                Rc::strong_count(&owned_value)
            ),
            (1, true, 1)
        );
    }
}
