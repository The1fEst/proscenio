use crate::platform::hyprland::hyprconfig::{self, Area, lua_string, read_lua_string, render};

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub class: String,
    pub rule: String,
    pub value: String,
}

fn skip(text: &str) -> &str {
    text.trim_start_matches(char::is_whitespace)
}

fn strip_word<'a>(text: &'a str, expected: &str) -> Option<&'a str> {
    skip(text).strip_prefix(expected)
}

const REGEX_SPECIAL: &str = "\\.+*?()|[]{}^$";

fn escape_regex(text: &str) -> String {
    let mut pattern = String::new();
    for character in text.chars() {
        if REGEX_SPECIAL.contains(character) {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

fn unescape_regex(pattern: &str) -> String {
    let mut text = String::new();
    let mut characters = pattern.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\'
            && let Some(special) =
                characters.next_if(|candidate| REGEX_SPECIAL.contains(*candidate))
        {
            text.push(special);
            continue;
        }
        text.push(character);
    }
    text
}

pub fn parse_line(line: &str) -> Option<Rule> {
    let rest = line.trim().strip_prefix("hl.window_rule({")?;
    let rest = strip_word(rest, "match")?;
    let rest = strip_word(rest, "=")?;
    let rest = strip_word(rest, "{")?;
    let rest = strip_word(rest, "class")?;
    let rest = strip_word(rest, "=")?;
    let (pattern, rest) = read_lua_string(skip(rest))?;
    let class = unescape_regex(pattern.strip_prefix("^(")?.strip_suffix(")$")?);
    let rest = strip_word(rest, "}")?;
    let rest = skip(strip_word(rest, ",")?);
    let name_end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let rule = &rest[..name_end];
    let rest = skip(strip_word(&rest[name_end..], "=")?);
    let (value, rest) = match read_lua_string(rest) {
        Some(value_and_rest) => value_and_rest,
        None => {
            let close = rest.find('}')?;
            (rest[..close].trim_end().to_owned(), &rest[close..])
        }
    };
    if rest.trim() != "})" {
        return None;
    }
    Some(Rule {
        class,
        rule: rule.to_owned(),
        value,
    })
}

fn line_for(class: &str, rule: &str, value: &str) -> String {
    format!(
        "hl.window_rule({{ match = {{ class = {} }}, {rule} = {} }})",
        lua_string(&format!("^({})$", escape_regex(class))),
        render(value)
    )
}

pub fn parse(text: &str) -> Vec<Rule> {
    text.lines().filter_map(parse_line).collect()
}

fn is_rule(line: &str, class: &str, rule: &str) -> bool {
    parse_line(line).is_some_and(|existing| existing.class == class && existing.rule == rule)
}

fn lines_without_rule(text: &str, class: &str, rule: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !is_rule(line, class, rule))
        .map(str::to_owned)
        .collect()
}

fn join_lines(lines: &[String]) -> String {
    format!("{}\n", lines.join("\n").trim_end_matches('\n'))
}

pub fn with_rule(text: &str, class: &str, rule: &str, value: &str) -> String {
    let mut lines = lines_without_rule(text, class, rule);
    lines.push(line_for(class, rule, value));
    join_lines(&lines)
}

pub fn without_rule(text: &str, class: &str, rule: &str) -> String {
    join_lines(&lines_without_rule(text, class, rule))
}

pub fn read() -> Vec<Rule> {
    parse(&hyprconfig::read(Area::Apps))
}

pub fn add(class: &str, rule: &str, value: &str) -> std::io::Result<()> {
    let text = with_rule(&hyprconfig::read_for_edit(Area::Apps)?, class, rule, value);
    hyprconfig::write(Area::Apps, &text)
}

pub fn remove(class: &str, rule: &str) -> std::io::Result<()> {
    let text = without_rule(&hyprconfig::read_for_edit(Area::Apps)?, class, rule);
    hyprconfig::write(Area::Apps, &text)
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

    #[test]
    fn a_class_is_matched_literally_and_quotes_stay_inside_the_strings() {
        assert_eq!(
            with_rule("", "Minecraft* 1.20.1", "workspace", "a\"b\\c"),
            "hl.window_rule({ match = { class = \"^(Minecraft\\\\* 1\\\\.20\\\\.1)$\" }, workspace = \"a\\\"b\\\\c\" })\n"
        );
        for class in [
            "x)$\" }, float = true }) os.execute(\"touch /tmp/x\") --",
            "a\\\"b",
            "two\nlines",
            "org.kde.dolphin",
        ] {
            let text = with_rule("", class, "workspace", "}) os.execute(\"x\") --\n");
            assert_eq!(text.lines().count(), 1);
            assert_eq!(
                parse(&text),
                [rule(class, "workspace", "}) os.execute(\"x\") --\n")]
            );
        }
        assert_eq!(
            with_rule(
                "hl.window_rule({ match = { class = \"^(org.kde.dolphin)$\" }, float = true })\n",
                "org.kde.dolphin",
                "float",
                "false"
            ),
            "hl.window_rule({ match = { class = \"^(org\\\\.kde\\\\.dolphin)$\" }, float = false })\n"
        );
        assert_eq!(
            parse(
                "hl.window_rule({ match = { class = \"^(steam_app_\\\\d\\\\.)$\" }, pin = true })"
            ),
            [rule("steam_app_\\d.", "pin", "true")]
        );
    }
}
