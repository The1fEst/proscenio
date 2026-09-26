use crate::platform::hyprconfig::{render, settings_path};

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub class: String,
    pub rule: String,
    pub value: String,
}

fn skip(text: &str) -> &str {
    text.trim_start_matches(char::is_whitespace)
}

fn word<'a>(text: &'a str, expected: &str) -> Option<&'a str> {
    skip(text).strip_prefix(expected)
}

pub fn parse_line(line: &str) -> Option<Rule> {
    let rest = line.trim().strip_prefix("hl.window_rule({")?;
    let rest = word(rest, "match")?;
    let rest = word(rest, "=")?;
    let rest = word(rest, "{")?;
    let rest = word(rest, "class")?;
    let rest = word(rest, "=")?;
    let rest = word(rest, "\"^(")?;
    let quote = rest.find('"')?;
    let class = rest[..quote].strip_suffix(")$")?;
    let rest = word(&rest[quote + 1..], "}")?;
    let rest = skip(word(rest, ",")?);
    let name_end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let rule = &rest[..name_end];
    let rest = skip(word(&rest[name_end..], "=")?);
    let close = rest.find('}')?;
    if rest[close..].trim_end() != "})" {
        return None;
    }
    Some(Rule {
        class: class.to_owned(),
        rule: rule.to_owned(),
        value: rest[..close].trim_end().trim_matches('"').to_owned(),
    })
}

fn line_for(class: &str, rule: &str, value: &str) -> String {
    format!(
        "hl.window_rule({{ match = {{ class = \"^({class})$\" }}, {rule} = {} }})",
        render(value)
    )
}

pub fn parse(text: &str) -> Vec<Rule> {
    text.lines().filter_map(parse_line).collect()
}

fn is_rule(line: &str, class: &str, rule: &str) -> bool {
    parse_line(line).is_some_and(|found| found.class == class && found.rule == rule)
}

fn kept(text: &str, class: &str, rule: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !is_rule(line, class, rule))
        .map(str::to_owned)
        .collect()
}

fn joined(lines: &[String]) -> String {
    format!("{}\n", lines.join("\n").trim_end_matches('\n'))
}

pub fn with_rule(text: &str, class: &str, rule: &str, value: &str) -> String {
    let mut lines = kept(text, class, rule);
    lines.push(line_for(class, rule, value));
    joined(&lines)
}

pub fn without_rule(text: &str, class: &str, rule: &str) -> String {
    joined(&kept(text, class, rule))
}

fn contents() -> String {
    std::fs::read_to_string(settings_path()).unwrap_or_default()
}

pub fn read() -> Vec<Rule> {
    parse(&contents())
}

pub fn add(class: &str, rule: &str, value: &str) -> std::io::Result<()> {
    std::fs::write(settings_path(), with_rule(&contents(), class, rule, value))
}

pub fn remove(class: &str, rule: &str) -> std::io::Result<()> {
    std::fs::write(settings_path(), without_rule(&contents(), class, rule))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "-- mine\nhl.window_rule({ match = { class = \"^(kitty)$\" }, float = true })\nhl.window_rule({ match = { class = \"^(firefox)$\" }, opacity = 0.9 })\n  hl.window_rule({match={class=\"^(steam)$\"},workspace=\"3\"})  \nhl.window_rule({ match = { class = \"kitty\" }, float = true })\n";

    fn rule(class: &str, rule: &str, value: &str) -> Rule {
        Rule {
            class: class.to_owned(),
            rule: rule.to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn only_lines_in_the_settings_apps_shape_are_rules() {
        assert_eq!(
            parse(FILE),
            [
                rule("kitty", "float", "true"),
                rule("firefox", "opacity", "0.9"),
                rule("steam", "workspace", "3"),
            ]
        );
    }

    #[test]
    fn adding_replaces_the_same_rule_and_removing_leaves_the_rest() {
        assert_eq!(
            with_rule(FILE, "kitty", "float", "false"),
            "-- mine\nhl.window_rule({ match = { class = \"^(firefox)$\" }, opacity = 0.9 })\n  hl.window_rule({match={class=\"^(steam)$\"},workspace=\"3\"})  \nhl.window_rule({ match = { class = \"kitty\" }, float = true })\nhl.window_rule({ match = { class = \"^(kitty)$\" }, float = false })\n"
        );
        assert_eq!(
            with_rule("", "code", "workspace", "special:magic"),
            "hl.window_rule({ match = { class = \"^(code)$\" }, workspace = \"special:magic\" })\n"
        );
        assert_eq!(
            without_rule(FILE, "steam", "workspace"),
            "-- mine\nhl.window_rule({ match = { class = \"^(kitty)$\" }, float = true })\nhl.window_rule({ match = { class = \"^(firefox)$\" }, opacity = 0.9 })\nhl.window_rule({ match = { class = \"kitty\" }, float = true })\n"
        );
    }
}
