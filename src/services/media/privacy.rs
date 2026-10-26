use serde_json::Value;
use std::collections::HashMap;

use crate::core::process;

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
}
