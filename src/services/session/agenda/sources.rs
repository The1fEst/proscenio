use gtk4::glib::{self, Variant};
use std::collections::HashMap;

const SOURCE_INTERFACE: &str = "org.gnome.evolution.dataserver.Source";

type Interfaces = HashMap<String, HashMap<String, Variant>>;

#[derive(Debug, PartialEq)]
pub struct Source {
    pub uid: String,
    pub color: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct Target {
    pub uid: String,
    pub name: String,
    pub is_selected: bool,
}

const READ_ONLY_BACKENDS: [&str; 4] = ["webcal", "http", "contacts", "weather"];

pub fn from_managed_objects(objects: &Variant) -> Vec<Source> {
    rank(objects, parse, |source| &source.uid)
}

pub fn targets_from_managed_objects(objects: &Variant) -> Vec<Target> {
    rank(objects, parse_target, |target| &target.uid)
}

fn rank<T>(
    objects: &Variant,
    parse: fn(&str, &str) -> Option<(i64, T)>,
    uid_of: fn(&T) -> &str,
) -> Vec<T> {
    let Some(objects) = objects.get::<HashMap<glib::variant::ObjectPath, Interfaces>>() else {
        return Vec::new();
    };
    let mut ranked: Vec<(i64, T)> = objects
        .values()
        .filter_map(|interfaces| {
            let properties = interfaces.get(SOURCE_INTERFACE)?;
            let uid = properties.get("UID")?.str()?;
            let data = properties.get("Data")?.str()?;
            parse(uid, data)
        })
        .collect();
    ranked.sort_by(|(first_order, first), (second_order, second)| {
        (first_order, uid_of(first)).cmp(&(second_order, uid_of(second)))
    });
    ranked.into_iter().map(|(_, item)| item).collect()
}

fn parse_target(uid: &str, data: &str) -> Option<(i64, Target)> {
    if lookup(data, "Data Source", "Enabled") == Some("false") {
        return None;
    }
    let backend = lookup(data, "Calendar", "BackendName")?;
    if backend.is_empty() || READ_ONLY_BACKENDS.contains(&backend) {
        return None;
    }
    let order = lookup(data, "Calendar", "Order")
        .and_then(|order| order.parse().ok())
        .unwrap_or(0);
    Some((
        order,
        Target {
            uid: uid.to_owned(),
            name: lookup(data, "Data Source", "DisplayName")
                .filter(|name| !name.is_empty())
                .unwrap_or(uid)
                .to_owned(),
            is_selected: lookup(data, "Calendar", "Selected") == Some("true"),
        },
    ))
}

fn parse(uid: &str, data: &str) -> Option<(i64, Source)> {
    if lookup(data, "Data Source", "Enabled") == Some("false") {
        return None;
    }
    if lookup(data, "Calendar", "Selected") != Some("true") {
        return None;
    }
    let order = lookup(data, "Calendar", "Order")
        .and_then(|order| order.parse().ok())
        .unwrap_or(0);
    let color = lookup(data, "Calendar", "Color")
        .filter(|color| !color.is_empty())
        .map(str::to_owned);
    Some((
        order,
        Source {
            uid: uid.to_owned(),
            color,
        },
    ))
}

fn lookup_section<'a>(data: &'a str, section: &str) -> Option<impl Iterator<Item = &'a str>> {
    let header = format!("[{section}]");
    let mut lines = data.lines().map(str::trim);
    lines.find(|line| *line == header)?;
    Some(lines.take_while(|line| !line.starts_with('[')))
}

fn lookup<'a>(data: &'a str, section: &str, key: &str) -> Option<&'a str> {
    lookup_section(data, section)?.find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::prelude::*;

    const LOCAL: &str = "\n[Data Source]\nDisplayName=Personal\nDisplayName[de]=Persönlich\nEnabled=true\nParent=local-stub\n\n[Calendar]\nBackendName=local\nColor=#62a0ea\nSelected=true\nOrder=2\n\n[Refresh]\nEnabled=true\n";

    fn build_objects(sources: &[(&str, &str)]) -> Variant {
        let map: HashMap<glib::variant::ObjectPath, Interfaces> = sources
            .iter()
            .enumerate()
            .map(|(index, (uid, data))| {
                let properties: HashMap<String, Variant> = [
                    ("UID".to_owned(), (*uid).to_variant()),
                    ("Data".to_owned(), (*data).to_variant()),
                ]
                .into();
                (
                    glib::variant::ObjectPath::try_from(format!("/Source_{index}")).unwrap(),
                    HashMap::from([(SOURCE_INTERFACE.to_owned(), properties)]),
                )
            })
            .collect();
        map.to_variant()
    }

    #[test]
    fn a_selected_enabled_calendar_gives_its_uid_and_color() {
        assert_eq!(
            parse("system-calendar", LOCAL),
            Some((
                2,
                Source {
                    uid: "system-calendar".into(),
                    color: Some("#62a0ea".into())
                }
            ))
        );
    }

    #[test]
    fn sources_that_are_disabled_unselected_or_not_calendars_are_left_out() {
        let disabled_data = LOCAL.replace("Enabled=true\nParent", "Enabled=false\nParent");
        assert_eq!(parse("a", &disabled_data), None);
        let unselected_data = LOCAL.replace("Selected=true", "Selected=false");
        assert_eq!(parse("b", &unselected_data), None);
        let no_selection_data = LOCAL.replace("Selected=true\n", "");
        assert_eq!(parse("c", &no_selection_data), None);
        let task_list_data = LOCAL.replace("[Calendar]", "[Task List]");
        assert_eq!(parse("d", &task_list_data), None);
    }

    #[test]
    fn only_enabled_calendars_with_a_writable_backend_are_targets() {
        let target = |uid: &str, data: &str| parse_target(uid, data).map(|(_, target)| target);
        assert_eq!(
            target("system-calendar", LOCAL),
            Some(Target {
                uid: "system-calendar".into(),
                name: "Personal".into(),
                is_selected: true
            })
        );
        let unselected_data = LOCAL.replace("Selected=true", "Selected=false");
        assert!(!target("a", &unselected_data).unwrap().is_selected);
        for backend in READ_ONLY_BACKENDS {
            let data = LOCAL.replace("BackendName=local", &format!("BackendName={backend}"));
            assert_eq!(target("b", &data), None);
        }
        let disabled_data = LOCAL.replace("Enabled=true\nParent", "Enabled=false\nParent");
        assert_eq!(target("c", &disabled_data), None);
        let task_list_data = LOCAL.replace("[Calendar]", "[Task List]");
        assert_eq!(target("d", &task_list_data), None);
        let unnamed_data = LOCAL.replace("DisplayName=Personal\n", "");
        assert_eq!(target("e", &unnamed_data).unwrap().name, "e");
    }

    #[test]
    fn targets_keep_unselected_calendars_in_order() {
        let unselected_data = LOCAL.replace("Selected=true", "Selected=false");
        let targets = targets_from_managed_objects(&build_objects(&[
            ("zeta", LOCAL),
            ("alpha", &unselected_data),
            ("birthdays", &LOCAL.replace("local", "contacts")),
        ]));
        let uids: Vec<&str> = targets.iter().map(|target| target.uid.as_str()).collect();
        assert_eq!(uids, ["alpha", "zeta"]);
    }

    #[test]
    fn keys_of_other_sections_do_not_count_and_a_missing_color_is_none() {
        let data =
            "[Data Source]\nEnabled=true\n[Calendar]\nSelected=true\n[Refresh]\nColor=#ff0000\n";
        let (order, source) = parse("a", data).unwrap();
        assert_eq!((order, source.color), (0, None));
    }

    #[test]
    fn managed_objects_give_the_calendars_ordered_by_order_then_uid() {
        let first_order_data = LOCAL.replace("Order=2", "Order=0");
        let sources = from_managed_objects(&build_objects(&[
            ("zeta", LOCAL),
            ("alpha", &first_order_data),
            ("mid", &first_order_data),
            ("proxy", "[Data Source]\nEnabled=true\n[Proxy]\nMethod=0\n"),
        ]));
        let uids: Vec<&str> = sources.iter().map(|source| source.uid.as_str()).collect();
        assert_eq!(uids, ["alpha", "mid", "zeta"]);
    }
}
