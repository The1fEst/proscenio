use gtk4::glib::{self, Variant};
use std::collections::HashMap;

const SOURCE_INTERFACE: &str = "org.gnome.evolution.dataserver.Source";

type Interfaces = HashMap<String, HashMap<String, Variant>>;

#[derive(Debug, PartialEq)]
pub struct Source {
    pub uid: String,
    pub color: Option<String>,
}

pub fn from_managed_objects(objects: &Variant) -> Vec<Source> {
    let Some(objects) = objects.get::<HashMap<glib::variant::ObjectPath, Interfaces>>() else {
        return Vec::new();
    };
    let mut ranked_sources: Vec<(i64, Source)> = objects
        .values()
        .filter_map(|interfaces| {
            let properties = interfaces.get(SOURCE_INTERFACE)?;
            let uid = properties.get("UID")?.str()?;
            let data = properties.get("Data")?.str()?;
            parse(uid, data)
        })
        .collect();
    ranked_sources.sort_by(|(first_order, first), (second_order, second)| {
        (first_order, &first.uid).cmp(&(second_order, &second.uid))
    });
    ranked_sources
        .into_iter()
        .map(|(_, source)| source)
        .collect()
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
