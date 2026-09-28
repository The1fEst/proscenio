use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Punct(char),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub path: Vec<String>,
    pub title: String,
}

struct Container {
    title: Option<String>,
    parent: Option<String>,
}

const SECTIONS: [&str; 2] = ["section", "busy_section"];
const SUBSECTIONS: [&str; 2] = ["subsection", "unkept_subsection"];

pub fn settings(source: &str) -> Vec<Found> {
    let tokens = tokenize(source);
    let mut containers: HashMap<String, Container> = HashMap::new();
    let mut found: Vec<Found> = Vec::new();
    for index in 0..tokens.len() {
        if tokens[index] == Token::Ident("let".into()) {
            bind(&tokens, index, &mut containers);
            continue;
        }
        if let Some(fields) = struct_literal_at(&tokens, index) {
            let path = enclosing_container(&tokens, index, &containers)
                .map(|parent| path_of(&containers, &parent))
                .unwrap_or_default();
            for entry in struct_settings(&fields, path) {
                if !found.contains(&entry) {
                    found.push(entry);
                }
            }
            continue;
        }
        let Some((name, arguments)) = call_at(&tokens, index) else {
            continue;
        };
        let method = index > 0 && tokens[index - 1] == Token::Punct('.');
        let entry = if method && SECTIONS.contains(&name) {
            arguments
                .get(1)
                .and_then(|title| literal(title))
                .map(|title| Found {
                    path: Vec::new(),
                    title,
                })
        } else if method && SUBSECTIONS.contains(&name) {
            let path = arguments
                .first()
                .and_then(|parent| container_name(parent))
                .map(|parent| path_of(&containers, &parent))
                .unwrap_or_default();
            arguments
                .get(1)
                .and_then(|title| literal(title))
                .map(|title| Found { path, title })
        } else if name == "notice" {
            None
        } else {
            control(&arguments, &containers).map(|mut entry| {
                if entry.path.is_empty()
                    && let Some(parent) = enclosing_container(&tokens, index, &containers)
                {
                    entry.path = path_of(&containers, &parent);
                }
                entry
            })
        };
        if let Some(entry) = entry.filter(|entry| !entry.title.is_empty())
            && !found.contains(&entry)
        {
            found.push(entry);
        }
    }
    found
}

fn control(arguments: &[&[Token]], containers: &HashMap<String, Container>) -> Option<Found> {
    let tuple = arguments.iter().find_map(|argument| match argument {
        [
            Token::Punct('('),
            icon @ Token::Str(_),
            Token::Punct(','),
            label @ Token::Str(_),
            Token::Punct(')'),
        ] => titled(std::slice::from_ref(icon), std::slice::from_ref(label)),
        _ => None,
    });
    let title = tuple.or_else(|| {
        arguments
            .windows(2)
            .find_map(|pair| titled(pair[0], pair[1]))
    })?;
    let path = arguments
        .iter()
        .filter_map(|argument| container_name(argument))
        .find(|name| containers.contains_key(name))
        .map(|parent| path_of(containers, &parent))
        .unwrap_or_default();
    Some(Found { path, title })
}

fn titled(icon: &[Token], label: &[Token]) -> Option<String> {
    let icon = literal(icon)?;
    icon.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        .then(|| title(label))?
}

fn struct_literal_at(tokens: &[Token], at: usize) -> Option<Vec<(String, String)>> {
    let Token::Ident(name) = &tokens[at] else {
        return None;
    };
    if !name.starts_with(|c: char| c.is_ascii_uppercase())
        || tokens.get(at + 1) != Some(&Token::Punct('{'))
    {
        return None;
    }
    let mut fields = Vec::new();
    let mut depth = 0;
    for index in at + 1..tokens.len() {
        match &tokens[index] {
            Token::Punct('(' | '[' | '{') => depth += 1,
            Token::Punct(')' | ']' | '}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(fields);
                }
            }
            Token::Ident(field) if depth == 1 => {
                if let (Some(Token::Punct(':')), Some(Token::Str(value)), Some(after)) = (
                    tokens.get(index + 1),
                    tokens.get(index + 2),
                    tokens.get(index + 3),
                ) && matches!(after, Token::Punct(',' | '}'))
                    && tokens.get(index.wrapping_sub(1)) != Some(&Token::Punct(':'))
                {
                    fields.push((field.clone(), value.clone()));
                }
            }
            _ => {}
        }
    }
    None
}

fn struct_settings(fields: &[(String, String)], path: Vec<String>) -> Vec<Found> {
    let text = |value: &str| title(&[Token::Str(value.to_owned())]);
    let Some(group) = fields
        .iter()
        .find(|(field, _)| field == "title")
        .and_then(|(_, value)| text(value))
    else {
        return Vec::new();
    };
    let inner: Vec<String> = path.iter().cloned().chain([group.clone()]).collect();
    let mut found = vec![Found { path, title: group }];
    found.extend(
        fields
            .iter()
            .filter(|(field, _)| field.ends_with("_text"))
            .filter_map(|(_, value)| text(value))
            .map(|title| Found {
                path: inner.clone(),
                title,
            }),
    );
    found
}

fn enclosing_container(
    tokens: &[Token],
    at: usize,
    containers: &HashMap<String, Container>,
) -> Option<String> {
    let mut depth = 0;
    for token in tokens[..at].iter().rev() {
        match token {
            Token::Punct(')' | ']') => depth += 1,
            Token::Punct('(' | '[') => depth -= 1,
            Token::Punct(';' | '{' | '}') if depth <= 0 => return None,
            Token::Ident(name) if depth <= 0 && containers.contains_key(name) => {
                return Some(name.clone());
            }
            _ => {}
        }
    }
    None
}

fn bind(tokens: &[Token], at: usize, containers: &mut HashMap<String, Container>) {
    let mut index = at + 1;
    if tokens.get(index) == Some(&Token::Ident("mut".into())) {
        index += 1;
    }
    if tokens.get(index) == Some(&Token::Punct('(')) {
        index += 1;
    }
    let Some(Token::Ident(name)) = tokens.get(index) else {
        return;
    };
    let Some(equals) = tokens[index..]
        .iter()
        .position(|token| *token == Token::Punct('='))
        .map(|offset| index + offset)
    else {
        return;
    };
    let Some((call, arguments)) = (equals + 1..tokens.len())
        .take_while(|&at| tokens[at] != Token::Punct(';'))
        .find_map(|at| call_at(tokens, at).map(|(call, arguments)| (at, call, arguments)))
        .map(|(_, call, arguments)| (call, arguments))
    else {
        return;
    };
    let parent_of = |arguments: &[&[Token]], containers: &HashMap<String, Container>| {
        arguments
            .iter()
            .filter_map(|argument| container_name(argument))
            .find(|name| containers.contains_key(name))
    };
    let container = if SECTIONS.contains(&call) {
        Container {
            title: arguments.get(1).and_then(|title| literal(title)),
            parent: None,
        }
    } else if SUBSECTIONS.contains(&call) {
        Container {
            title: arguments.get(1).and_then(|title| literal(title)),
            parent: arguments.first().and_then(|parent| container_name(parent)),
        }
    } else if let Some(parent) = parent_of(&arguments, containers) {
        Container {
            title: None,
            parent: Some(parent),
        }
    } else {
        containers.remove(name);
        return;
    };
    containers.insert(name.clone(), container);
}

fn path_of(containers: &HashMap<String, Container>, name: &str) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = Some(name.to_owned());
    let mut steps = 0;
    while let Some(name) = current {
        let Some(container) = containers.get(&name) else {
            break;
        };
        if let Some(title) = container.title.as_ref().filter(|title| !title.is_empty()) {
            path.insert(0, title.clone());
        }
        current = container.parent.clone();
        steps += 1;
        if steps > 16 {
            break;
        }
    }
    path
}

fn call_at(tokens: &[Token], at: usize) -> Option<(&str, Vec<&[Token]>)> {
    let Token::Ident(name) = &tokens[at] else {
        return None;
    };
    if tokens.get(at + 1) != Some(&Token::Punct('(')) {
        return None;
    }
    let mut arguments = Vec::new();
    let mut depth = 0;
    let mut start = at + 2;
    for index in at + 1..tokens.len() {
        match tokens[index] {
            Token::Punct('(' | '[' | '{') => depth += 1,
            Token::Punct(')' | ']' | '}') => {
                depth -= 1;
                if depth == 0 {
                    if start < index {
                        arguments.push(&tokens[start..index]);
                    }
                    return Some((name.as_str(), arguments));
                }
            }
            Token::Punct(',') if depth == 1 => {
                arguments.push(&tokens[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    None
}

fn literal(argument: &[Token]) -> Option<String> {
    match argument {
        [Token::Str(text)] | [Token::Punct('&'), Token::Str(text)] => Some(text.clone()),
        _ => None,
    }
}

fn title(argument: &[Token]) -> Option<String> {
    let text = literal(argument).or_else(|| shell_format(argument))?;
    let first = text.chars().next()?;
    (first.is_uppercase() || text.contains(' ')).then_some(text)
}

fn shell_format(argument: &[Token]) -> Option<String> {
    let [
        Token::Punct('&'),
        Token::Ident(format),
        Token::Punct('!'),
        Token::Punct('('),
        Token::Str(text),
        Token::Punct(','),
        rest @ ..,
    ] = argument
    else {
        return None;
    };
    let shell_name = [
        Token::Ident("shell".into()),
        Token::Punct(':'),
        Token::Punct(':'),
        Token::Ident("name".into()),
        Token::Punct('('),
        Token::Punct(')'),
        Token::Punct(')'),
    ];
    (format == "format" && rest == shell_name && text.matches("{}").count() == 1)
        .then(|| text.clone())
}

fn container_name(argument: &[Token]) -> Option<String> {
    match argument {
        [Token::Ident(name)] | [Token::Punct('&'), Token::Ident(name)] => Some(name.clone()),
        _ => None,
    }
}

fn tokenize(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        let next = chars.get(index + 1).copied();
        if c.is_whitespace() {
            index += 1;
        } else if c == '/' && next == Some('/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while index < chars.len() {
                if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                    depth += 1;
                    index += 2;
                } else if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    depth -= 1;
                    index += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    index += 1;
                }
            }
        } else if c == '"' {
            let (text, end) = quoted(&chars, index + 1);
            tokens.push(Token::Str(text));
            index = end;
        } else if (c == 'r' || c == 'b') && raw_start(&chars, index).is_some() {
            let (hashes, open) = raw_start(&chars, index).unwrap_or((0, index));
            let mut end = open + 1;
            let mut text = String::new();
            while end < chars.len() {
                if chars[end] == '"' && (1..=hashes).all(|n| chars.get(end + n) == Some(&'#')) {
                    break;
                }
                text.push(chars[end]);
                end += 1;
            }
            tokens.push(Token::Str(text));
            index = end + 1 + hashes;
        } else if c == 'b' && next == Some('"') {
            let (text, end) = quoted(&chars, index + 2);
            tokens.push(Token::Str(text));
            index = end;
        } else if c == '\'' {
            if next == Some('\\') {
                index += 2;
                while index < chars.len() && chars[index] != '\'' {
                    index += 1;
                }
                index += 1;
            } else if chars.get(index + 2) == Some(&'\'') {
                index += 3;
            } else {
                index += 1;
                while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_')
                {
                    index += 1;
                }
            }
        } else if c.is_alphanumeric() || c == '_' {
            let start = index;
            while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_') {
                index += 1;
            }
            tokens.push(Token::Ident(chars[start..index].iter().collect()));
        } else {
            tokens.push(Token::Punct(c));
            index += 1;
        }
    }
    tokens
}

fn raw_start(chars: &[char], at: usize) -> Option<(usize, usize)> {
    let mut index = at;
    if chars.get(index) == Some(&'b') {
        index += 1;
    }
    if chars.get(index) != Some(&'r') {
        return None;
    }
    index += 1;
    let mut hashes = 0;
    while chars.get(index) == Some(&'#') {
        hashes += 1;
        index += 1;
    }
    (chars.get(index) == Some(&'"')).then_some((hashes, index))
}

fn quoted(chars: &[char], from: usize) -> (String, usize) {
    let mut text = String::new();
    let mut index = from;
    while index < chars.len() && chars[index] != '"' {
        if chars[index] != '\\' {
            text.push(chars[index]);
            index += 1;
            continue;
        }
        index += 1;
        match chars.get(index) {
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('u') => {
                let close = chars[index..].iter().position(|&c| c == '}').unwrap_or(0);
                let hex: String = chars[index + 2..index + close].iter().collect();
                if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    text.push(c);
                }
                index += close;
            }
            Some('\n') => {
                while chars.get(index + 1).is_some_and(|c| c.is_whitespace()) {
                    index += 1;
                }
            }
            Some(&other) => text.push(other),
            None => {}
        }
        index += 1;
    }
    (text, index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(path: &[&str], title: &str) -> Found {
        Found {
            path: path.iter().map(|part| (*part).to_owned()).collect(),
            title: title.to_owned(),
        }
    }

    #[test]
    fn controls_are_placed_under_their_section_and_subsection() {
        let source = r#"
            pub fn build(context: &Context) -> Rc<Page> {
                // page.config_switch(&main, "x", "Commented out", "/x", true);
                let main = page.section("lock", "Screen");
                let blurred = page.subsection(&main, "Style: Blurred", "");
                page.config_switch(&blurred, "blur_on", "Enable blur", BLUR, true);
                let (radius_row, radius) = page.config_spin(&blurred, "", "Blur radius", "/r", 100, (0, 300), 10);
                let row = page.row(&blurred);
                hyprrows::switch(&page, &row, &options, "blur_on", "Behind windows", "decoration:blur:enabled");
                hyprrows::spin(&page, &row, &options, &spin("blur_circular", "Passes", 1));
                option_switch(&page, &blurred, &options, ("animation", "Reduced motion"), |o| true);
                idle_timeout_row(&page, &main, &options, &IdleTimeout {
                    what: "lock",
                    title: "Automatic Screen Lock",
                    tip: "Locks the session after a period of inactivity",
                    switch_icon: "lock_clock",
                    switch_text: "Lock the session",
                    fallback_minutes: 30,
                });
                page.config_switch(&main, "water_drop", &format!("Use Hyprlock (instead of {})", shell::name()), "/h", false);
                page.selection(&main, vec![choice("Top", "vertical_align_top", "top")], "/p", Value::Null, |_| {});
                let (content, _) = page.unkept_subsection(&main, &group.label, "");
                page.config_switch(&content, "check", "Inside a named group", "/g", false);
            }
        "#;
        assert_eq!(
            settings(source),
            vec![
                found(&[], "Screen"),
                found(&["Screen"], "Style: Blurred"),
                found(&["Screen", "Style: Blurred"], "Enable blur"),
                found(&["Screen", "Style: Blurred"], "Blur radius"),
                found(&["Screen", "Style: Blurred"], "Behind windows"),
                found(&["Screen", "Style: Blurred"], "Passes"),
                found(&["Screen", "Style: Blurred"], "Reduced motion"),
                found(&["Screen"], "Automatic Screen Lock"),
                found(&["Screen", "Automatic Screen Lock"], "Lock the session"),
                found(&["Screen"], "Use Hyprlock (instead of {})"),
                found(&["Screen"], "Inside a named group"),
            ]
        );
    }

    #[test]
    fn strings_keep_their_escapes_and_lifetimes_are_not_quotes() {
        let source =
            r#"fn f<'a>(x: &'a str) { page.switch(&p, "info", "Show \"Locked\" text", |_| {}); }"#;
        assert_eq!(settings(source), vec![found(&[], "Show \"Locked\" text")]);
    }
}
