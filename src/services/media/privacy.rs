use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::core::process;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Activity {
    pub is_microphone_in_use: bool,
    pub is_screen_shared: bool,
}

pub fn read(on_activity: impl FnOnce(Activity) + 'static) {
    process::read(&["pw-dump"], move |output| {
        let dump = serde_json::from_str(&output).unwrap_or(Value::Null);
        on_activity(parse_activity(&dump));
    });
}

fn parse_activity(dump: &Value) -> Activity {
    let objects = dump.as_array().map(Vec::as_slice).unwrap_or_default();
    let classes: HashMap<u64, &str> = objects
        .iter()
        .filter(|object| object["type"] == "PipeWire:Interface:Node")
        .filter_map(|node| {
            let props = node.pointer("/info/props")?;
            let class = props["media.class"].as_str()?;
            if class == "Video/Source" && is_camera(props) {
                return None;
            }
            if props["stream.monitor"] == true {
                return None;
            }
            Some((node["id"].as_u64()?, class))
        })
        .collect();
    let monitor_ports: HashSet<u64> = objects
        .iter()
        .filter(|object| {
            object["type"] == "PipeWire:Interface:Port"
                && object["info"]["props"]["port.monitor"] == true
        })
        .filter_map(|port| port["id"].as_u64())
        .collect();
    let class = |id: &Value| id.as_u64().and_then(|id| classes.get(&id).copied());
    let is_monitor_port = |id: &Value| id.as_u64().is_some_and(|id| monitor_ports.contains(&id));
    let mut activity = Activity::default();
    for link in objects.iter().filter(|object| {
        object["type"] == "PipeWire:Interface:Link" && object["info"]["state"] == "active"
    }) {
        let source = class(&link["info"]["output-node-id"]);
        let target = class(&link["info"]["input-node-id"]);
        if source == Some("Video/Source") {
            activity.is_screen_shared = true;
        }
        let is_capture = match source {
            Some("Audio/Source" | "Audio/Source/Virtual" | "Audio/Source/Internal") => true,
            Some("Audio/Duplex") => !is_monitor_port(&link["info"]["output-port-id"]),
            _ => false,
        };
        if is_capture && target == Some("Stream/Input/Audio") {
            activity.is_microphone_in_use = true;
        }
    }
    activity
}

fn is_camera(props: &Value) -> bool {
    props["media.role"] == "Camera"
        || matches!(props["device.api"].as_str(), Some("v4l2" | "libcamera"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: u64, class: &str) -> Value {
        json!({"id": id, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": class}}})
    }

    fn link(from: u64, to: u64) -> Value {
        json!({"type": "PipeWire:Interface:Link", "info": {"output-node-id": from, "input-node-id": to, "state": "active"}})
    }

    fn with_state(mut link: Value, state: &str) -> Value {
        link["info"]["state"] = state.into();
        link
    }

    fn with_output_port(mut link: Value, port: u64) -> Value {
        link["info"]["output-port-id"] = port.into();
        link
    }

    #[test]
    fn active_capture_links_out_of_sources_mean_the_device_is_in_use() {
        let nodes = [
            node(1, "Audio/Source"),
            node(2, "Stream/Input/Audio"),
            node(3, "Video/Source"),
            node(4, "Stream/Input/Video"),
            node(5, "Audio/Sink"),
            json!({"id": 6, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Video/Source", "media.role": "Camera"}}}),
            json!({"id": 7, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Video/Source", "device.api": "libcamera"}}}),
            json!({"id": 8, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Video/Source", "device.api": "v4l2"}}}),
            node(9, "Audio/Source/Virtual"),
            node(10, "Audio/Duplex"),
            json!({"id": 11, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Stream/Input/Audio", "stream.monitor": true}}}),
            node(12, "Audio/Source/Internal"),
            json!({"id": 100, "type": "PipeWire:Interface:Port", "info": {"props": {"port.monitor": true}}}),
            json!({"id": 101, "type": "PipeWire:Interface:Port", "info": {"props": {"port.monitor": false}}}),
        ];
        let cases = [
            (vec![], Activity::default()),
            (
                vec![link(1, 2)],
                Activity {
                    is_microphone_in_use: true,
                    is_screen_shared: false,
                },
            ),
            (vec![link(1, 5)], Activity::default()),
            (
                vec![link(3, 4)],
                Activity {
                    is_microphone_in_use: false,
                    is_screen_shared: true,
                },
            ),
            (vec![link(6, 4)], Activity::default()),
            (vec![link(7, 4)], Activity::default()),
            (vec![link(8, 4)], Activity::default()),
            (
                vec![link(6, 4), link(3, 4)],
                Activity {
                    is_microphone_in_use: false,
                    is_screen_shared: true,
                },
            ),
            (
                vec![link(6, 4), link(1, 2)],
                Activity {
                    is_microphone_in_use: true,
                    is_screen_shared: false,
                },
            ),
            (
                vec![with_state(link(1, 2), "paused"), link(3, 4)],
                Activity {
                    is_microphone_in_use: false,
                    is_screen_shared: true,
                },
            ),
            (
                vec![link(1, 2), with_state(link(3, 4), "paused")],
                Activity {
                    is_microphone_in_use: true,
                    is_screen_shared: false,
                },
            ),
            (
                vec![with_output_port(link(10, 2), 100)],
                Activity::default(),
            ),
            (
                vec![with_output_port(link(10, 2), 101)],
                Activity {
                    is_microphone_in_use: true,
                    is_screen_shared: false,
                },
            ),
            (vec![link(1, 11)], Activity::default()),
            (
                vec![link(12, 2)],
                Activity {
                    is_microphone_in_use: true,
                    is_screen_shared: false,
                },
            ),
            (
                vec![link(9, 2)],
                Activity {
                    is_microphone_in_use: true,
                    is_screen_shared: false,
                },
            ),
            (
                vec![link(10, 2)],
                Activity {
                    is_microphone_in_use: true,
                    is_screen_shared: false,
                },
            ),
        ];
        for (links, expected) in cases {
            let dump: Vec<Value> = nodes.iter().chain(&links).cloned().collect();
            assert_eq!(parse_activity(&Value::from(dump)), expected, "{links:?}");
        }
    }
}
