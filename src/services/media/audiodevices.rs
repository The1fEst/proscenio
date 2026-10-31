use libpulse_binding::def::PortAvailable;
use serde_json::Value;
use std::collections::HashMap;

use crate::core::config;

pub const ALIASES: &str = "/audio/aliases";

pub struct Device {
    pub name: String,
    pub description: String,
    pub nick: String,
    pub ports: Vec<Port>,
    pub active_port: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub description: String,
    pub availability: PortAvailable,
}

pub struct SinkEntry {
    pub name: String,
    pub is_available: bool,
}

pub fn resolve_alias<'a>(
    aliases: &'a HashMap<String, String>,
    name: &str,
    description: &'a str,
) -> &'a str {
    aliases.get(name).map_or(description, String::as_str)
}

pub fn store_alias(name: &str, alias: &str) {
    let mut aliases = config::current().audio_aliases.clone();
    apply_alias(&mut aliases, name, alias);
    let entries: serde_json::Map<String, Value> = aliases
        .into_iter()
        .map(|(name, alias)| (name, Value::from(alias)))
        .collect();
    config::store_value(ALIASES, Value::Object(entries));
}

fn apply_alias(aliases: &mut HashMap<String, String>, name: &str, alias: &str) {
    let alias = alias.trim();
    if alias.is_empty() {
        aliases.remove(name);
    } else {
        aliases.insert(name.to_owned(), alias.to_owned());
    }
}

pub fn is_sink_available(ports: &[PortAvailable]) -> bool {
    ports.is_empty() || ports.iter().any(|port| *port != PortAvailable::No)
}

pub fn find_next_sink<'a>(sinks: &'a [SinkEntry], current_name: &str) -> Option<&'a str> {
    let mut names: Vec<&str> = sinks
        .iter()
        .filter(|sink| sink.is_available || sink.name == current_name)
        .map(|sink| sink.name.as_str())
        .collect();
    names.sort_unstable();
    let next_index = names
        .iter()
        .position(|name| *name == current_name)
        .map_or(0, |index| index + 1);
    let next = names.get(next_index).or(names.first())?;
    (*next != current_name).then_some(*next)
}

pub fn filter_usable_ports(ports: Vec<Port>) -> Vec<Port> {
    ports
        .into_iter()
        .filter(|port| port.availability != PortAvailable::No)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_ports_are_dropped_and_unknown_ones_kept() {
        let port = |name: &str, availability| Port {
            name: name.to_owned(),
            description: name.to_owned(),
            availability,
        };
        let ports = filter_usable_ports(vec![
            port("speakers", PortAvailable::Yes),
            port("headphones", PortAvailable::No),
            port("line-out", PortAvailable::Unknown),
        ]);
        let names: Vec<&str> = ports.iter().map(|port| port.name.as_str()).collect();
        assert_eq!(names, ["speakers", "line-out"]);
    }

    #[test]
    fn an_alias_replaces_the_description_of_its_device_only() {
        let aliases = HashMap::from([("sink_a".to_owned(), "Desk".to_owned())]);
        assert_eq!(resolve_alias(&aliases, "sink_a", "Speakers"), "Desk");
        assert_eq!(resolve_alias(&aliases, "sink_b", "Speakers"), "Speakers");
    }

    #[test]
    fn an_alias_is_stored_trimmed_and_an_empty_one_removes_it() {
        let mut aliases = HashMap::new();
        apply_alias(&mut aliases, "sink_a", "  Desk ");
        apply_alias(&mut aliases, "sink_b", "Living room");
        assert_eq!(aliases["sink_a"], "Desk");
        apply_alias(&mut aliases, "sink_a", "   ");
        apply_alias(&mut aliases, "missing", "");
        assert_eq!(
            aliases,
            HashMap::from([("sink_b".to_owned(), "Living room".to_owned())])
        );
    }

    #[test]
    fn a_sink_is_available_without_ports_or_with_a_port_that_may_be_in_use() {
        use PortAvailable::{No, Unknown, Yes};
        for (ports, expected) in [
            (&[][..], true),
            (&[No, Yes][..], true),
            (&[No, Unknown][..], true),
            (&[No, No][..], false),
        ] {
            assert_eq!(is_sink_available(ports), expected, "{ports:?}");
        }
    }

    #[test]
    fn cycling_goes_to_the_next_available_sink_by_name_and_wraps() {
        let sinks = |entries: &[(&str, bool)]| -> Vec<SinkEntry> {
            entries
                .iter()
                .map(|(name, is_available)| SinkEntry {
                    name: (*name).to_owned(),
                    is_available: *is_available,
                })
                .collect()
        };
        let listed = sinks(&[("c", true), ("a", true), ("d", false), ("b", true)]);
        for (current_name, expected) in [
            ("a", Some("b")),
            ("b", Some("c")),
            ("c", Some("a")),
            ("d", Some("a")),
            ("missing", Some("a")),
        ] {
            assert_eq!(
                find_next_sink(&listed, current_name),
                expected,
                "{current_name}"
            );
        }
        assert_eq!(find_next_sink(&sinks(&[("a", true)]), "a"), None);
        assert_eq!(
            find_next_sink(&sinks(&[("a", true), ("b", false)]), "a"),
            None
        );
        assert_eq!(find_next_sink(&[], "a"), None);
    }
}
