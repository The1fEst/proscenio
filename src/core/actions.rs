use std::cell::RefCell;
use std::rc::Rc;

type Handler = Rc<dyn Fn()>;

struct Action {
    name: &'static str,
    description: &'static str,
    on_press: Handler,
    on_release: Option<Handler>,
}

thread_local! {
    static ACTIONS: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
}

pub fn add(name: &'static str, description: &'static str, handler: impl Fn() + 'static) {
    ACTIONS.with(|actions| {
        let mut actions = actions.borrow_mut();
        if actions.iter().any(|action| action.name == name) {
            return;
        }
        actions.push(Action {
            name,
            description,
            on_press: Rc::new(handler),
            on_release: None,
        });
    });
}

pub fn set_release_handler(name: &'static str, handler: impl Fn() + 'static) {
    ACTIONS.with(|actions| {
        if let Some(action) = actions
            .borrow_mut()
            .iter_mut()
            .find(|action| action.name == name)
        {
            action.on_release = Some(Rc::new(handler));
        }
    });
}

pub fn run(name: &str) {
    let handler = ACTIONS.with(|actions| {
        actions
            .borrow()
            .iter()
            .find(|action| action.name == name)
            .map(|action| action.on_press.clone())
    });
    if let Some(handler) = handler {
        handler();
    }
}

pub fn release(name: &str) {
    let handler = ACTIONS.with(|actions| {
        actions
            .borrow()
            .iter()
            .find(|action| action.name == name)
            .and_then(|action| action.on_release.clone())
    });
    if let Some(handler) = handler {
        handler();
    }
}

pub fn list() -> Vec<(&'static str, &'static str)> {
    ACTIONS.with(|actions| {
        actions
            .borrow()
            .iter()
            .map(|action| (action.name, action.description))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn an_action_runs_its_press_and_release_by_name_and_keeps_the_first_registration() {
        let press_count = Rc::new(Cell::new(0));
        let release_count = Rc::new(Cell::new(0));
        add("toggle", "Toggles", {
            let press_count = press_count.clone();
            move || press_count.set(press_count.get() + 1)
        });
        add("toggle", "Again", || {
            panic!("a second registration must not replace the first")
        });
        set_release_handler("toggle", {
            let release_count = release_count.clone();
            move || release_count.set(release_count.get() + 1)
        });
        run("toggle");
        release("toggle");
        run("missing");
        assert_eq!((press_count.get(), release_count.get()), (1, 1));
        assert_eq!(list(), vec![("toggle", "Toggles")]);
    }
}
