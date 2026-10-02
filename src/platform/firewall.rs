use gtk4::glib;
use std::process::Command;

use crate::core::i18n::{tr, trf};
use crate::platform::nmprofile::{self, Family};
use crate::platform::privileged;

pub const COMMAND: &str = "firewall";
const CONF: &str = "/etc/ufw/ufw.conf";
const DEFAULTS: &str = "/etc/default/ufw";
const RULES: [&str; 2] = ["/etc/ufw/user.rules", "/etc/ufw/user6.rules"];
const TUPLE: &str = "### tuple ###";
const ANYWHERE: [&str; 2] = ["0.0.0.0/0", "::/0"];
pub const ACTIONS: [&str; 4] = ["allow", "deny", "reject", "limit"];
pub const PROTOCOLS: [&str; 3] = ["any", "tcp", "udp"];
pub const POLICIES: [&str; 3] = ["deny", "reject", "allow"];

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
    pub enabled: bool,
    pub incoming: String,
    pub rules: Vec<Rule>,
}

fn decode_hex(text: &str) -> String {
    let bytes: Vec<u8> = (0..text.len() / 2)
        .filter_map(|at| u8::from_str_radix(text.get(at * 2..at * 2 + 2)?, 16).ok())
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn anywhere(address: &str) -> bool {
    ANYWHERE.contains(&address)
}

pub fn parse_rules(text: &str) -> Vec<Rule> {
    text.lines()
        .filter_map(|line| line.strip_prefix(TUPLE))
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
            if direction != "in" || !anywhere(destination) || source_port != "any" {
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
                from: if anywhere(source) {
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
    let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
    let mut rules: Vec<Rule> = Vec::new();
    for path in RULES {
        for rule in parse_rules(&read(path)) {
            if !rules.contains(&rule) {
                rules.push(rule);
            }
        }
    }
    State {
        enabled: read(CONF).lines().any(|line| line.trim() == "ENABLED=yes"),
        incoming: policy(&read(DEFAULTS)),
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
    let parts: Vec<&str> = rule
        .port
        .split(',')
        .filter(|part| !part.is_empty())
        .collect();
    for part in &parts {
        let fits = part
            .split(':')
            .map(|number| number.parse::<u16>().ok().filter(|number| *number > 0))
            .collect::<Option<Vec<_>>>()
            .is_some_and(|numbers| numbers.len() <= 2);
        if !fits {
            return Err(trf(
                "%1 is not a port or a range such as 6000:6007",
                &[part],
            ));
        }
    }
    let several = parts.len() > 1 || rule.port.contains(':');
    if several && rule.protocol == "any" {
        return Err(tr("A range or a list of ports needs TCP or UDP"));
    }
    if !rule.from.is_empty() {
        let family = if rule.from.contains(':') {
            Family::V6
        } else {
            Family::V4
        };
        nmprofile::parse_addresses(&rule.from, family)?;
    }
    if rule.comment.contains(['\n', '\'']) {
        return Err(tr("A comment cannot hold quotes or line breaks"));
    }
    Ok(())
}

pub fn ufw_words(rule: &Rule, with_comment: bool) -> Vec<String> {
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
    if with_comment && !rule.comment.is_empty() {
        arguments.extend(["comment".to_owned(), rule.comment.clone()]);
    }
    arguments
}

fn rule_from(words: &[String]) -> Option<Rule> {
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

fn ufw(arguments: &[String]) -> bool {
    Command::new("ufw")
        .args(arguments)
        .env("PATH", "/usr/sbin:/usr/bin")
        .status()
        .is_ok_and(|status| status.success())
}

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let done = |success: bool| {
        if success {
            glib::ExitCode::SUCCESS
        } else {
            glib::ExitCode::FAILURE
        }
    };
    let words = |list: &[&str]| {
        list.iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>()
    };
    match arguments {
        [verb] if verb == "enable" => {
            let enabled = ufw(&words(&["--force", "enable"]));
            let _ = Command::new("systemctl")
                .args(["enable", "ufw.service"])
                .status();
            done(enabled)
        }
        [verb] if verb == "disable" => done(ufw(&words(&["disable"]))),
        [verb, policy] if verb == "default" && POLICIES.contains(&policy.as_str()) => {
            done(ufw(&words(&["default", policy, "incoming"])))
        }
        [verb, rest @ ..] if verb == "add" || verb == "delete" => {
            let Some(rule) = rule_from(rest) else {
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
            done(ufw(&command))
        }
        _ => glib::ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let four = "### tuple ### allow tcp 22 0.0.0.0/0 any 0.0.0.0/0 in comment=7373682074657374\n\
                    -A ufw-user-input -p tcp --dport 22 -j ACCEPT\n\
                    ### tuple ### allow tcp 8080 0.0.0.0/0 any 10.0.0.0/8 in\n\
                    ### tuple ### deny udp 6000:6007 0.0.0.0/0 any 0.0.0.0/0 in\n\
                    ### tuple ### allow any 22 0.0.0.0/0 any 0.0.0.0/0 OpenSSH - in\n\
                    ### tuple ### allow tcp 80 0.0.0.0/0 any 0.0.0.0/0 out\n";
        let six = "### tuple ### allow tcp 22 ::/0 any ::/0 in comment=7373682074657374\n";
        let mut rules = parse_rules(four);
        for found in parse_rules(six) {
            if !rules.contains(&found) {
                rules.push(found);
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
}
