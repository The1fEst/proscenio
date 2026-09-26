use std::cell::RefCell;
use std::rc::Rc;

type Handler = Rc<dyn Fn()>;

struct Action {
    name: &'static str,
    description: &'static str,
    press: Handler,
    release: Option<Handler>,
}

thread_local! {
    static ACTIONS: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
}

pub fn add(name: &'static str, description: &'static str, press: impl Fn() + 'static) {
    ACTIONS.with(|actions| {
        let mut actions = actions.borrow_mut();
        if actions.iter().any(|action| action.name == name) {
            return;
        }
        actions.push(Action {
            name,
            description,
            press: Rc::new(press),
            release: None,
        });
    });
}

pub fn on_release(name: &'static str, release: impl Fn() + 'static) {
    ACTIONS.with(|actions| {
        if let Some(action) = actions
            .borrow_mut()
            .iter_mut()
            .find(|action| action.name == name)
        {
            action.release = Some(Rc::new(release));
        }
    });
}

pub fn run(name: &str) {
    let press = ACTIONS.with(|actions| {
        actions
            .borrow()
            .iter()
            .find(|action| action.name == name)
            .map(|action| action.press.clone())
    });
    if let Some(press) = press {
        press();
    }
}

pub fn release(name: &str) {
    let release = ACTIONS.with(|actions| {
        actions
            .borrow()
            .iter()
            .find(|action| action.name == name)
            .and_then(|action| action.release.clone())
    });
    if let Some(release) = release {
        release();
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
        let pressed = Rc::new(Cell::new(0));
        let released = Rc::new(Cell::new(0));
        add("toggle", "Toggles", {
            let pressed = pressed.clone();
            move || pressed.set(pressed.get() + 1)
        });
        add("toggle", "Again", || {
            panic!("a second registration must not replace the first")
        });
        on_release("toggle", {
            let released = released.clone();
            move || released.set(released.get() + 1)
        });
        run("toggle");
        release("toggle");
        run("missing");
        assert_eq!((pressed.get(), released.get()), (1, 1));
        assert_eq!(list(), vec![("toggle", "Toggles")]);
    }
}
