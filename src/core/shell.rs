use std::cell::Cell;

thread_local! {
    static NAME: Cell<&'static str> = const { Cell::new("proscenio") };
}

pub fn name() -> &'static str {
    NAME.get()
}

#[cfg(feature = "compat")]
pub fn set_name(name: &'static str) {
    NAME.set(name);
}
