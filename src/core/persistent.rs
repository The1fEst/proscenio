use serde_json::Value;
use std::path::PathBuf;

pub fn read(keys: &[&str]) -> Option<Value> {
    let text = std::fs::read_to_string(path()).ok()?;
    let mut node: Value = serde_json::from_str(&text).ok()?;
    for key in keys {
        node = node.get_mut(*key)?.take();
    }
    Some(node)
}

pub fn write(keys: &[&str], value: Value) {
    let path = path();
    let mut root = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Default::default()));
    let Some((last, sections)) = keys.split_last() else {
        return;
    };
    let mut node = &mut root;
    for section in sections {
        let Some(object) = node.as_object_mut() else {
            return;
        };
        node = object
            .entry(*section)
            .or_insert_with(|| Value::Object(Default::default()));
    }
    let Some(object) = node.as_object_mut() else {
        return;
    };
    object.insert((*last).to_owned(), value);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&root) {
        let _ = std::fs::write(path, text + "\n");
    }
}

fn path() -> PathBuf {
    crate::core::paths::state().join("states.json")
}
