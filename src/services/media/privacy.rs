use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::core::process;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Activity {
    pub is_microphone_in_use: bool,
    pub is_screen_shared: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Application {
    pub name: String,
    pub icon: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoKind {
    Screen,
    Camera,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoUser {
    pub kind: VideoKind,
    pub application: Application,
    pub device: String,
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

pub fn read_video_users(on_users: impl FnOnce(Option<Vec<VideoUser>>) + 'static) {
    process::read_optional(&["pw-dump"], move |output| {
        let dump = output.and_then(|output| serde_json::from_str::<Value>(&output).ok());
        on_users(
            dump.filter(Value::is_array)
                .map(|dump| parse_video_users(&dump)),
        );
    });
}

fn parse_video_users(dump: &Value) -> Vec<VideoUser> {
    let objects = dump.as_array().map(Vec::as_slice).unwrap_or_default();
    let props: HashMap<u64, &Value> = objects
        .iter()
        .filter(|object| object["type"] == "PipeWire:Interface:Node")
        .filter_map(|node| Some((node["id"].as_u64()?, node.pointer("/info/props")?)))
        .collect();
    let find_props = |id: &Value, class: &str| {
        id.as_u64()
            .and_then(|id| props.get(&id).copied())
            .filter(|props| props["media.class"] == class)
    };
    let read_text = |props: &Value, keys: &[&str]| {
        keys.iter()
            .find_map(|key| props[*key].as_str().filter(|value| !value.is_empty()))
            .unwrap_or_default()
            .to_owned()
    };
    let mut users: Vec<VideoUser> = Vec::new();
    for link in objects.iter().filter(|object| {
        object["type"] == "PipeWire:Interface:Link" && object["info"]["state"] == "active"
    }) {
        let (Some(source), Some(stream)) = (
            find_props(&link["info"]["output-node-id"], "Video/Source"),
            find_props(&link["info"]["input-node-id"], "Stream/Input/Video"),
        ) else {
            continue;
        };
        let user = VideoUser {
            kind: if is_camera(source) {
                VideoKind::Camera
            } else {
                VideoKind::Screen
            },
            application: Application {
                name: read_text(
                    stream,
                    &["application.name", "node.name", "node.description"],
                ),
                icon: read_text(
                    stream,
                    &["application.icon-name", "application.process.binary"],
                ),
            },
            device: read_text(source, &["node.description", "node.nick"]),
        };
        if !users.contains(&user) {
            users.push(user);
        }
    }
    users
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

    fn video_dump() -> Vec<Value> {
        vec![
            json!({"id": 1, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Video/Source", "node.name": "xdph-streaming-abc"}}}),
            json!({"id": 2, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Video/Source", "device.api": "v4l2", "node.description": "Integrated Camera"}}}),
            json!({"id": 3, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Stream/Input/Video", "application.name": "OBS Studio", "application.icon-name": "com.obsproject.Studio"}}}),
            json!({"id": 4, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": "Stream/Input/Video", "node.name": "brave", "application.process.binary": "brave"}}}),
            node(5, "Audio/Source"),
            link(1, 3),
            link(2, 4),
            link(2, 3),
        ]
    }

    #[test]
    fn video_links_name_the_application_reading_each_source() {
        let user = |kind, name: &str, icon: &str, device: &str| VideoUser {
            kind,
            application: Application {
                name: name.to_owned(),
                icon: icon.to_owned(),
            },
            device: device.to_owned(),
        };
        assert_eq!(
            parse_video_users(&Value::from(video_dump())),
            [
                user(VideoKind::Screen, "OBS Studio", "com.obsproject.Studio", ""),
                user(VideoKind::Camera, "brave", "brave", "Integrated Camera"),
                user(
                    VideoKind::Camera,
                    "OBS Studio",
                    "com.obsproject.Studio",
                    "Integrated Camera"
                ),
            ]
        );
    }

    #[test]
    fn inactive_and_non_video_links_name_no_video_user() {
        let mut dump = video_dump();
        dump.retain(|object| object["type"] != "PipeWire:Interface:Link");
        dump.extend([with_state(link(1, 3), "paused"), link(5, 3), link(1, 5)]);
        assert_eq!(parse_video_users(&Value::from(dump)), []);
        assert_eq!(parse_video_users(&Value::Null), []);
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
