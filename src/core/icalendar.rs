#[derive(Debug, Default, PartialEq)]
pub struct Component {
    pub name: String,
    pub properties: Vec<Property>,
    pub children: Vec<Component>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Property {
    pub name: String,
    pub parameters: Vec<(String, String)>,
    pub value: String,
}

impl Component {
    pub fn property(&self, name: &str) -> Option<&Property> {
        self.properties
            .iter()
            .find(|property| property.name == name)
    }

    pub fn properties_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Property> {
        self.properties
            .iter()
            .filter(move |property| property.name == name)
    }

    pub fn find_all(&self, name: &str) -> Vec<&Component> {
        let mut components = Vec::new();
        self.collect(name, &mut components);
        components
    }

    fn collect<'a>(&'a self, name: &str, components: &mut Vec<&'a Component>) {
        if self.name == name {
            components.push(self);
        }
        for child in &self.children {
            child.collect(name, components);
        }
    }
}

impl Property {
    pub fn parameter(&self, name: &str) -> Option<&str> {
        self.parameters
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn text(&self) -> String {
        unescape_text(&self.value)
    }
}

pub fn parse(text: &str) -> Vec<Component> {
    let mut roots = Vec::new();
    let mut stack: Vec<Component> = Vec::new();
    for line in unfold(text) {
        let Some(property) = parse_line(&line) else {
            continue;
        };
        match property.name.as_str() {
            "BEGIN" => stack.push(Component {
                name: property.value.to_ascii_uppercase(),
                ..Component::default()
            }),
            "END" => {
                let Some(component) = stack.pop() else {
                    continue;
                };
                match stack.last_mut() {
                    Some(parent) => parent.children.push(component),
                    None => roots.push(component),
                }
            }
            _ => {
                if let Some(component) = stack.last_mut() {
                    component.properties.push(property);
                }
            }
        }
    }
    while let Some(component) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(component),
            None => roots.push(component),
        }
    }
    roots
}

fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for chunk in text.split('\n') {
        let line = chunk.strip_suffix('\r').unwrap_or(chunk);
        if let Some(continuation) = line.strip_prefix([' ', '\t'])
            && let Some(previous) = lines.last_mut()
        {
            previous.push_str(continuation);
            continue;
        }
        if !line.is_empty() {
            lines.push(line.to_owned());
        }
    }
    lines
}

fn parse_line(line: &str) -> Option<Property> {
    let mut is_quoted = false;
    let mut colon = None;
    let mut semicolons = Vec::new();
    for (index, character) in line.char_indices() {
        match character {
            '"' => is_quoted = !is_quoted,
            ';' if !is_quoted && colon.is_none() => semicolons.push(index),
            ':' if !is_quoted => {
                colon = Some(index);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let head_end = semicolons.first().copied().unwrap_or(colon);
    let mut parameters = Vec::new();
    for (position, start) in semicolons.iter().enumerate() {
        let end = semicolons.get(position + 1).copied().unwrap_or(colon);
        let Some((key, value)) = line[start + 1..end].split_once('=') else {
            continue;
        };
        parameters.push((key.to_ascii_uppercase(), value.replace('"', "")));
    }
    Some(Property {
        name: line[..head_end].to_ascii_uppercase(),
        parameters,
        value: line[colon + 1..].to_owned(),
    })
}

pub fn unescape_text(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('n' | 'N') => text.push('\n'),
            Some(other) => text.push(other),
            None => text.push('\\'),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_unfolded_and_properties_keep_parameters_and_quoted_colons() {
        let text = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Lunch with\r\n  the team\r\n\
                    DTSTART;TZID=\"Europe/Berlin\";VALUE=DATE-TIME:20261104T120000\r\n\
                    ATTENDEE;CN=\"Doe; Jane: Dr\":mailto:jane@example.com\r\nEND:VEVENT\r\n\
                    END:VCALENDAR\r\n";
        let calendars = parse(text);
        assert_eq!(calendars.len(), 1);
        let event = &calendars[0].children[0];
        assert_eq!(event.name, "VEVENT");
        assert_eq!(
            event.property("SUMMARY").unwrap().value,
            "Lunch with the team"
        );
        let start = event.property("DTSTART").unwrap();
        assert_eq!(start.value, "20261104T120000");
        assert_eq!(start.parameter("TZID"), Some("Europe/Berlin"));
        assert_eq!(start.parameter("VALUE"), Some("DATE-TIME"));
        let attendee = event.property("ATTENDEE").unwrap();
        assert_eq!(attendee.parameter("CN"), Some("Doe; Jane: Dr"));
        assert_eq!(attendee.value, "mailto:jane@example.com");
    }

    #[test]
    fn text_values_lose_their_escapes() {
        assert_eq!(
            unescape_text(r"Room 1\, floor 2\; a\\b\nnext\Nline"),
            "Room 1, floor 2; a\\b\nnext\nline"
        );
    }

    #[test]
    fn names_are_uppercased_and_unterminated_components_are_kept() {
        let calendars = parse("begin:vevent\nsummary:Open\nbegin:valarm\ntrigger:-PT5M");
        assert_eq!(calendars.len(), 1);
        assert_eq!(calendars[0].name, "VEVENT");
        assert_eq!(calendars[0].property("SUMMARY").unwrap().value, "Open");
        assert_eq!(calendars[0].children[0].name, "VALARM");
    }

    #[test]
    fn nested_components_are_found_by_name() {
        let calendars = parse(
            "BEGIN:VCALENDAR\nBEGIN:VTIMEZONE\nEND:VTIMEZONE\nBEGIN:VEVENT\nUID:a\nEND:VEVENT\nBEGIN:VEVENT\nUID:b\nEND:VEVENT\nEND:VCALENDAR",
        );
        let uids: Vec<&str> = calendars[0]
            .find_all("VEVENT")
            .iter()
            .map(|event| event.property("UID").unwrap().value.as_str())
            .collect();
        assert_eq!(uids, ["a", "b"]);
    }
}
