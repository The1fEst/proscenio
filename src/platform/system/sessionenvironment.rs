use std::process::Command;

use crate::core::process;
use crate::platform::hyprland::hypr;
use crate::platform::hyprland::hyprconfig::lua_string;

fn build_eval_request(environment: &[(&str, String)]) -> String {
    let calls: String = environment
        .iter()
        .map(|(name, value)| format!("hl.env({}, {})\n", lua_string(name), lua_string(value)))
        .collect();
    format!("eval {calls}")
}

fn build_activation_update_command(environment: &[(&str, String)]) -> Command {
    let mut command = process::quiet(&["dbus-update-activation-environment", "--systemd"]);
    for (name, value) in environment {
        command.arg(name).env(name, value);
    }
    command
}

pub fn apply(environment: &[(&str, String)]) {
    for (name, value) in environment {
        unsafe {
            if value.is_empty() {
                std::env::remove_var(name);
            } else {
                std::env::set_var(name, value);
            }
        }
    }
    hypr::request(&build_eval_request(environment));
    process::start(build_activation_update_command(environment));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_variable_becomes_an_hl_env_call_with_its_value_quoted() {
        assert_eq!(
            build_eval_request(&[
                ("SHELL", "/usr/bin/fish".to_owned()),
                ("ODD", "a\"b\\c".to_owned()),
                ("EMPTY", String::new()),
            ]),
            "eval hl.env(\"SHELL\", \"/usr/bin/fish\")\nhl.env(\"ODD\", \"a\\\"b\\\\c\")\nhl.env(\"EMPTY\", \"\")\n"
        );
    }

    #[test]
    fn the_activation_update_takes_the_values_from_its_environment_not_its_arguments() {
        let environment = [
            ("http_proxy", "http://fest:secret@proxy.lan:3128".to_owned()),
            ("no_proxy", String::new()),
        ];
        let command = build_activation_update_command(&environment);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["--systemd", "http_proxy", "no_proxy"]
        );
        let variables: std::collections::BTreeMap<_, _> = command
            .get_envs()
            .map(|(name, value)| (name.to_owned(), value.map(ToOwned::to_owned)))
            .collect();
        assert_eq!(
            variables,
            environment
                .map(|(name, value)| (name.into(), Some(value.into())))
                .into()
        );
    }
}
