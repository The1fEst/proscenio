use serde_json::Value;
use std::collections::HashMap;

use crate::core::process;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Activity {
    pub microphone: bool,
    pub screen: bool,
}

pub fn read(handler: impl FnOnce(Activity) + 'static) {
    process::read(&["pw-dump"], move |output| {
        let dump = serde_json::from_str(&output).unwrap_or(Value::Null);
        handler(activity(&dump));
    });
}

fn activity(dump: &Value) -> Activity {
    let objects = dump.as_array().map(Vec::as_slice).unwrap_or_default();
    let classes: HashMap<u64, &str> = objects
        .iter()
        .filter(|object| object["type"] == "PipeWire:Interface:Node")
        .filter_map(|node| {
            let class = node.pointer("/info/props/media.class")?.as_str()?;
            Some((node["id"].as_u64()?, class))
        })
        .collect();
    let class = |id: &Value| id.as_u64().and_then(|id| classes.get(&id).copied());
    let mut found = Activity::default();
    for link in objects
        .iter()
        .filter(|object| object["type"] == "PipeWire:Interface:Link")
    {
        let source = class(&link["info"]["output-node-id"]);
        let target = class(&link["info"]["input-node-id"]);
        if source == Some("Video/Source") {
            found.screen = true;
        }
        if source == Some("Audio/Source") && target == Some("Stream/Input/Audio") {
            found.microphone = true;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: u64, class: &str) -> Value {
        json!({"id": id, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": class}}})
    }

    fn link(from: u64, to: u64) -> Value {
        json!({"type": "PipeWire:Interface:Link", "info": {"output-node-id": from, "input-node-id": to}})
    }

    #[test]
    fn links_out_of_sources_mean_the_device_is_in_use() {
        let nodes = [
            node(1, "Audio/Source"),
            node(2, "Stream/Input/Audio"),
            node(3, "Video/Source"),
            node(4, "Stream/Input/Video"),
            node(5, "Audio/Sink"),
        ];
        let cases = [
            (vec![], Activity::default()),
            (
                vec![link(1, 2)],
                Activity {
                    microphone: true,
                    screen: false,
                },
            ),
            (vec![link(1, 5)], Activity::default()),
            (
                vec![link(3, 4)],
                Activity {
                    microphone: false,
                    screen: true,
                },
            ),
        ];
        for (links, expected) in cases {
            let dump: Vec<Value> = nodes.iter().cloned().chain(links).collect();
            assert_eq!(activity(&Value::from(dump)), expected);
        }
    }
}
