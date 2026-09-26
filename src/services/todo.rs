use serde_json::{Value, json};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};

#[derive(Clone)]
pub struct Task {
    pub content: String,
    pub done: bool,
}

#[derive(Clone)]
pub struct Todo {
    pub list: Rc<RefCell<Vec<Task>>>,
    listeners: Rc<Listeners>,
}

impl Todo {
    pub fn new() -> Self {
        Todo {
            list: Rc::new(RefCell::new(read())),
            listeners: Rc::default(),
        }
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn add(&self, content: &str) {
        self.list.borrow_mut().push(Task {
            content: content.to_owned(),
            done: false,
        });
        self.save();
    }

    pub fn mark(&self, index: usize, done: bool) {
        if let Some(task) = self.list.borrow_mut().get_mut(index) {
            task.done = done;
        }
        self.save();
    }

    pub fn remove(&self, index: usize) {
        let mut list = self.list.borrow_mut();
        if index >= list.len() {
            return;
        }
        list.remove(index);
        drop(list);
        self.save();
    }

    fn save(&self) {
        write(&self.list.borrow());
        self.listeners.notify();
    }
}

pub fn path() -> PathBuf {
    crate::core::paths::state().join("todo.json")
}

fn read() -> Vec<Task> {
    let Ok(text) = std::fs::read_to_string(path()) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    parsed
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(Task {
                        content: item.get("content").and_then(Value::as_str)?.to_owned(),
                        done: item.get("done").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn write(list: &[Task]) {
    let path = path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let items: Vec<Value> = list
        .iter()
        .map(|task| json!({ "content": task.content, "done": task.done }))
        .collect();
    let _ = std::fs::write(path, Value::Array(items).to_string());
}
