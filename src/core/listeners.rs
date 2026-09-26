use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

type List<A> = RefCell<Vec<(u64, Rc<dyn Fn(&A)>)>>;

pub struct Listeners<A: 'static = ()> {
    list: Rc<List<A>>,
    next: Cell<u64>,
}

impl<A: 'static> Default for Listeners<A> {
    fn default() -> Self {
        Listeners {
            list: Rc::default(),
            next: Cell::new(0),
        }
    }
}

pub trait Unsubscribe {
    fn remove(&self, id: u64);
}

impl<A: 'static> Unsubscribe for List<A> {
    fn remove(&self, id: u64) {
        self.borrow_mut().retain(|(held, _)| *held != id);
    }
}

#[must_use]
pub struct Subscription {
    list: Weak<dyn Unsubscribe>,
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(list) = self.list.upgrade() {
            list.remove(self.id);
        }
    }
}

impl Subscription {
    pub fn new(list: Weak<dyn Unsubscribe>, id: u64) -> Self {
        Subscription { list, id }
    }

    pub fn forever(self) {
        std::mem::forget(self);
    }
}

impl<A: 'static> Listeners<A> {
    pub fn add_with(&self, listener: impl Fn(&A) + 'static) -> Subscription {
        let id = self.next.get();
        self.next.set(id + 1);
        self.list.borrow_mut().push((id, Rc::new(listener)));
        let list: Weak<List<A>> = Rc::downgrade(&self.list);
        Subscription { list, id }
    }

    pub fn notify_with(&self, value: &A) {
        let listeners: Vec<Rc<dyn Fn(&A)>> = self
            .list
            .borrow()
            .iter()
            .map(|(_, listener)| listener.clone())
            .collect();
        for listener in listeners {
            listener(value);
        }
    }
}

impl Listeners {
    pub fn add(&self, listener: impl Fn() + 'static) -> Subscription {
        self.add_with(move |_| listener())
    }

    pub fn notify(&self) {
        self.notify_with(&());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_subscription_stops_hearing_even_from_inside_a_notification() {
        let listeners = Listeners::default();
        let heard = Rc::new(Cell::new(0));
        let kept = listeners.add({
            let heard = heard.clone();
            move || heard.set(heard.get() + 1)
        });
        let dropped: Rc<RefCell<Option<Subscription>>> = Rc::new(RefCell::new(None));
        dropped.replace(Some(listeners.add({
            let dropped = dropped.clone();
            move || drop(dropped.take())
        })));
        listeners.notify();
        listeners.notify();
        assert_eq!(heard.get(), 2);
        assert!(dropped.borrow().is_none());
        drop(kept);
        listeners.notify();
        assert_eq!(heard.get(), 2);
    }

    #[test]
    fn listeners_with_an_argument_hear_it_until_their_subscription_goes() {
        let listeners: Listeners<(String, String)> = Listeners::default();
        let heard = Rc::new(RefCell::new(Vec::new()));
        let subscription = listeners.add_with({
            let heard = heard.clone();
            move |(event, data): &(String, String)| {
                heard.borrow_mut().push(format!("{event}>>{data}"))
            }
        });
        listeners.notify_with(&("workspace".to_owned(), "2".to_owned()));
        drop(subscription);
        listeners.notify_with(&("workspace".to_owned(), "3".to_owned()));
        assert_eq!(*heard.borrow(), vec!["workspace>>2".to_owned()]);
    }
}
