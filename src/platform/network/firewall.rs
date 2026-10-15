use gtk4::glib;
use std::process::Command;

use crate::core::i18n::{tr, trf};
use crate::platform::network::nmprofile::ipconfig::Family;
use crate::platform::security::privileged;

pub const COMMAND: &str = "firewall";
const CONFIG_PATH: &str = "/etc/ufw/ufw.conf";
const DEFAULTS_PATH: &str = "/etc/default/ufw";
const RULE_PATHS: [&str; 2] = ["/etc/ufw/user.rules", "/etc/ufw/user6.rules"];
const TUPLE_MARKER: &str = "### tuple ###";
const ANYWHERE: [&str; 2] = ["0.0.0.0/0", "::/0"];
pub const ACTIONS: [&str; 4] = ["allow", "deny", "reject", "limit"];
pub const PROTOCOLS: [&str; 3] = ["any", "tcp", "udp"];
pub const POLICIES: [&str; 3] = ["deny", "reject", "allow"];
const MULTIPORT_LIMIT: usize = 15;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rule {
    pub action: String,
    pub protocol: String,
    pub port: String,
    pub from: String,
    pub application: String,
    pub comment: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub is_enabled: bool,
    pub incoming: String,
    pub rules: Vec<Rule>,
}

fn decode_hex(text: &str) -> String {
    let bytes = proscenio_protocol::unhex(text).unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn is_anywhere(address: &str) -> bool {
    ANYWHERE.contains(&address)
}

pub fn parse_rules(text: &str) -> Vec<Rule> {
    text.lines()
        .filter_map(|line| line.strip_prefix(TUPLE_MARKER))
        .filter_map(|tuple| {
            let mut words: Vec<&str> = tuple.split_whitespace().collect();
            let comment = match words.last().and_then(|last| last.strip_prefix("comment=")) {
                Some(hex) => {
                    let comment = decode_hex(hex);
                    words.pop();
                    comment
                }
                None => String::new(),
            };
            let (fields, application, direction) = match words.as_slice() {
                [fields @ .., direction] if words.len() == 7 => (fields.to_vec(), "", *direction),
                [fields @ .., application, _, direction] if words.len() == 9 => {
                    (fields.to_vec(), *application, *direction)
                }
                _ => return None,
            };
            let [action, protocol, port, destination, source_port, source] = fields[..] else {
                return None;
            };
            if direction != "in" || !is_anywhere(destination) || source_port != "any" {
                return None;
            }
            Some(Rule {
                action: action.to_owned(),
                protocol: protocol.to_owned(),
                port: if port == "any" {
                    String::new()
                } else {
                    port.to_owned()
                },
                from: if is_anywhere(source) {
                    String::new()
                } else {
                    source.to_owned()
                },
                application: if application == "-" {
                    String::new()
                } else {
                    application.replace("%20", " ")
                },
                comment,
            })
        })
        .collect()
}

fn policy(text: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix("DEFAULT_INPUT_POLICY="))
        .map(|value| match value.trim_matches('"') {
            "ACCEPT" => "allow",
            "REJECT" => "reject",
            _ => "deny",
        })
        .unwrap_or("deny")
        .to_owned()
}

pub fn read() -> State {
    let read_file = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
    let mut rules: Vec<Rule> = Vec::new();
    for path in RULE_PATHS {
        for rule in parse_rules(&read_file(path)) {
            if !rules.contains(&rule) {
                rules.push(rule);
            }
        }
    }
    State {
        is_enabled: read_file(CONFIG_PATH)
            .lines()
            .any(|line| line.trim() == "ENABLED=yes"),
        incoming: policy(&read_file(DEFAULTS_PATH)),
        rules,
    }
}

pub fn check(rule: &Rule) -> Result<(), String> {
    if !ACTIONS.contains(&rule.action.as_str()) || !PROTOCOLS.contains(&rule.protocol.as_str()) {
        return Err(tr("Unknown action or protocol"));
    }
    if rule.port.is_empty() && rule.application.is_empty() && rule.from.is_empty() {
        return Err(tr("Name a port or where the connections come from"));
    }
    let port = if rule.application.is_empty() {
        rule.port.as_str()
    } else {
        ""
    };
    let parts: Vec<&str> = if port.is_empty() {
        Vec::new()
    } else {
        port.split(',').collect()
    };
    for part in &parts {
        let is_valid = part
            .split(':')
            .map(|text| {
                text.parse::<u16>()
                    .ok()
                    .filter(|number| *number > 0 && !text.starts_with('+'))
            })
            .collect::<Option<Vec<_>>>()
            .is_some_and(|numbers| match numbers[..] {
                [_] => true,
                [low, high] => low < high,
                _ => false,
            });
        if !is_valid {
            let invalid_port = if part.is_empty() { port } else { *part };
            return Err(trf(
                "%1 is not a port or a range such as 6000:6007",
                &[invalid_port],
            ));
        }
    }
    if port.matches([',', ':']).count() >= MULTIPORT_LIMIT {
        return Err(trf(
            "A rule takes at most %1 ports, and a range counts as two",
            &[&MULTIPORT_LIMIT.to_string()],
        ));
    }
    let has_several_ports = parts.len() > 1 || port.contains(':');
    if has_several_ports && rule.protocol == "any" {
        return Err(tr("A range or a list of ports needs TCP or UDP"));
    }
    if !rule.from.is_empty() {
        let family = if rule.from.contains(':') {
            Family::V6
        } else {
            Family::V4
        };
        family.network(&rule.from)?;
        if rule
            .from
            .split_once('/')
            .is_some_and(|(_, prefix)| prefix.starts_with('+'))
        {
            return Err(trf("%1 has no valid prefix length", &[&rule.from]));
        }
    }
    if rule.comment.contains(['\n', '\'']) {
        return Err(tr("A comment cannot hold quotes or line breaks"));
    }
    Ok(())
}

pub fn ufw_words(rule: &Rule, includes_comment: bool) -> Vec<String> {
    let mut arguments = vec![rule.action.clone(), "in".to_owned()];
    arguments.extend([
        "from".to_owned(),
        if rule.from.is_empty() {
            "any".to_owned()
        } else {
            rule.from.clone()
        },
    ]);
    arguments.extend(["to".to_owned(), "any".to_owned()]);
    if !rule.application.is_empty() {
        arguments.extend(["app".to_owned(), rule.application.clone()]);
    } else {
        if !rule.port.is_empty() {
            arguments.extend(["port".to_owned(), rule.port.clone()]);
        }
        if rule.protocol != "any" {
            arguments.extend(["proto".to_owned(), rule.protocol.clone()]);
        }
    }
    if includes_comment && !rule.comment.is_empty() {
        arguments.extend(["comment".to_owned(), rule.comment.clone()]);
    }
    arguments
}

fn parse_rule_words(words: &[String]) -> Option<Rule> {
    let [action, protocol, port, from, application, comment] = words else {
        return None;
    };
    Some(Rule {
        action: action.clone(),
        protocol: protocol.clone(),
        port: port.clone(),
        from: from.clone(),
        application: application.clone(),
        comment: comment.clone(),
    })
}

fn rule_words(rule: &Rule) -> Vec<String> {
    vec![
        rule.action.clone(),
        rule.protocol.clone(),
        rule.port.clone(),
        rule.from.clone(),
        rule.application.clone(),
        rule.comment.clone(),
    ]
}

pub async fn request(words: Vec<String>) -> Result<(), String> {
    privileged::request(COMMAND, words, tr("The firewall was not changed")).await
}

pub async fn add(rule: &Rule) -> Result<(), String> {
    let mut words = vec!["add".to_owned()];
    words.extend(rule_words(rule));
    request(words).await
}

pub async fn delete(rule: &Rule) -> Result<(), String> {
    let mut words = vec!["delete".to_owned()];
    words.extend(rule_words(rule));
    request(words).await
}

fn run_ufw(arguments: &[String]) -> bool {
    Command::new("ufw")
        .args(arguments)
        .env("PATH", "/usr/sbin:/usr/bin")
        .status()
        .is_ok_and(|status| status.success())
}

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let to_exit_code = |is_success: bool| {
        if is_success {
            glib::ExitCode::SUCCESS
        } else {
            glib::ExitCode::FAILURE
        }
    };
    let to_words = |list: &[&str]| {
        list.iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>()
    };
    match arguments {
        [verb] if verb == "enable" => {
            let is_success = run_ufw(&to_words(&["--force", "enable"]));
            let _ = Command::new("systemctl")
                .args(["enable", "ufw.service"])
                .status();
            to_exit_code(is_success)
        }
        [verb] if verb == "disable" => to_exit_code(run_ufw(&to_words(&["disable"]))),
        [verb, policy] if verb == "default" && POLICIES.contains(&policy.as_str()) => {
            to_exit_code(run_ufw(&to_words(&["default", policy, "incoming"])))
        }
        [verb, rest @ ..] if verb == "add" || verb == "delete" => {
            let Some(rule) = parse_rule_words(rest) else {
                return glib::ExitCode::FAILURE;
            };
            if let Err(problem) = check(&rule) {
                eprintln!("ERROR: {problem}");
                return glib::ExitCode::FAILURE;
            }
            let mut command = Vec::new();
            if verb == "delete" {
                command.push("delete".to_owned());
            }
            command.extend(ufw_words(&rule, verb == "add"));
            to_exit_code(run_ufw(&command))
        }
        _ => glib::ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_comment_decodes_only_from_hex_digits() {
        assert_eq!(decode_hex("6869"), "hi");
        assert_eq!(decode_hex("+a+b"), "");
    }

    fn rule(action: &str, protocol: &str, port: &str, from: &str, comment: &str) -> Rule {
        Rule {
            action: action.to_owned(),
            protocol: protocol.to_owned(),
            port: port.to_owned(),
            from: from.to_owned(),
            comment: comment.to_owned(),
            ..Rule::default()
        }
    }

    #[test]
    fn rules_come_from_ufw_tuples_once_for_both_families() {
        let ipv4_text = "### tuple ### allow tcp 22 0.0.0.0/0 any 0.0.0.0/0 in comment=7373682074657374\n\
                    -A ufw-user-input -p tcp --dport 22 -j ACCEPT\n\
                    ### tuple ### allow tcp 8080 0.0.0.0/0 any 10.0.0.0/8 in\n\
                    ### tuple ### deny udp 6000:6007 0.0.0.0/0 any 0.0.0.0/0 in\n\
                    ### tuple ### allow any 22 0.0.0.0/0 any 0.0.0.0/0 OpenSSH - in\n\
                    ### tuple ### allow tcp 80 0.0.0.0/0 any 0.0.0.0/0 out\n";
        let ipv6_text = "### tuple ### allow tcp 22 ::/0 any ::/0 in comment=7373682074657374\n";
        let mut rules = parse_rules(ipv4_text);
        for ipv6_rule in parse_rules(ipv6_text) {
            if !rules.contains(&ipv6_rule) {
                rules.push(ipv6_rule);
            }
        }
        assert_eq!(
            rules,
            [
                rule("allow", "tcp", "22", "", "ssh test"),
                rule("allow", "tcp", "8080", "10.0.0.0/8", ""),
                rule("deny", "udp", "6000:6007", "", ""),
                Rule {
                    application: "OpenSSH".to_owned(),
                    ..rule("allow", "any", "22", "", "")
                },
            ]
        );
        assert_eq!(policy("DEFAULT_INPUT_POLICY=\"REJECT\"\n"), "reject");
    }

    #[test]
    fn a_rule_becomes_ufw_words_and_bad_ones_are_refused() {
        assert_eq!(
            ufw_words(&rule("allow", "tcp", "8080", "10.0.0.0/8", "web"), true).join(" "),
            "allow in from 10.0.0.0/8 to any port 8080 proto tcp comment web"
        );
        assert_eq!(
            ufw_words(&rule("limit", "any", "22", "", "x"), false).join(" "),
            "limit in from any to any port 22"
        );
        assert!(check(&rule("allow", "any", "6000:6007", "", "")).is_err());
        assert!(check(&rule("allow", "udp", "6000:6007,53", "", "")).is_ok());
        assert!(check(&rule("allow", "tcp", "0", "", "")).is_err());
        assert!(check(&rule("allow", "tcp", "22", "nowhere", "")).is_err());
        assert!(check(&rule("drop", "tcp", "22", "", "")).is_err());
        assert!(check(&rule("allow", "tcp", "", "", "")).is_err());
    }

    #[test]
    fn a_rule_takes_one_source_and_what_ufw_counts_as_fifteen_ports_at_most() {
        assert!(check(&rule("allow", "tcp", "22", "fd00::/64", "")).is_ok());
        assert!(check(&rule("allow", "tcp", "22", "10.0.0.1, 10.0.0.2", "")).is_err());
        assert!(check(&rule("allow", "tcp", "22", "10.0.0.1 10.0.0.2", "")).is_err());
        let ports = |count: u16| {
            (1..=count)
                .map(|port| port.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        assert!(check(&rule("allow", "tcp", &ports(15), "", "")).is_ok());
        let ports_and_range = |count: u16| format!("{},20:30", ports(count));
        assert!(check(&rule("allow", "tcp", &ports_and_range(13), "", "")).is_ok());
        let limit_error = Err(trf(
            "A rule takes at most %1 ports, and a range counts as two",
            &["15"],
        ));
        assert_eq!(
            check(&rule("allow", "tcp", &ports(16), "", "")),
            limit_error
        );
        assert_eq!(
            check(&rule("allow", "tcp", &ports_and_range(14), "", "")),
            limit_error
        );
    }

    #[test]
    fn an_application_rule_is_checked_without_its_ports() {
        let samba = Rule {
            application: "Samba".to_owned(),
            ..rule("allow", "any", "137,138,139,445", "", "")
        };
        assert_eq!(check(&samba), Ok(()));
        assert_eq!(
            ufw_words(&samba, false).join(" "),
            "allow in from any to any app Samba"
        );
    }

    #[test]
    fn a_sign_before_a_prefix_or_port_is_refused() {
        assert_eq!(
            check(&rule("allow", "tcp", "22", "10.0.0.0/+8", "")),
            Err(trf("%1 has no valid prefix length", &["10.0.0.0/+8"]))
        );
        for port in ["+22", "6000:+6007"] {
            assert_eq!(
                check(&rule("allow", "tcp", port, "", "")),
                Err(trf(
                    "%1 is not a port or a range such as 6000:6007",
                    &[port]
                ))
            );
        }
    }

    #[test]
    fn a_port_list_has_no_empty_entries_and_a_range_goes_up() {
        for port in ["80,,443", "80,443,", ",80", "6007:6000", "22:22"] {
            assert_eq!(
                check(&rule("allow", "tcp", port, "", "")),
                Err(trf(
                    "%1 is not a port or a range such as 6000:6007",
                    &[port]
                ))
            );
        }
    }
}
