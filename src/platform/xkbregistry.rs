const REGISTRIES: [&str; 2] = [
    "/usr/share/X11/xkb/rules/evdev.xml",
    "/usr/local/share/X11/xkb/rules/evdev.xml",
];

#[derive(Debug, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub text: String,
    pub children: Vec<Element>,
}

impl Element {
    fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.name == name)
    }

    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |child| child.name == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub code: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub code: String,
    pub name: String,
    pub variants: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OptionGroup {
    pub code: String,
    pub name: String,
    pub options: Vec<Item>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalogue {
    pub layouts: Vec<Layout>,
    pub option_groups: Vec<OptionGroup>,
}

fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else {
            break;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(character) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (at, character) in text.char_indices() {
        match (quote, character) {
            (None, '"' | '\'') => quote = Some(character),
            (Some(open), _) if open == character => quote = None,
            (None, '>') => return Some(at),
            _ => {}
        }
    }
    None
}

fn close(stack: &mut Vec<Element>, root: &mut Option<Element>) {
    let Some(done) = stack.pop() else {
        return;
    };
    match stack.last_mut() {
        Some(parent) => parent.children.push(done),
        None => *root = Some(done),
    }
}

pub fn parse_xml(text: &str) -> Option<Element> {
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->")? + 3..];
        } else if let Some(after) = rest.strip_prefix("<![CDATA[") {
            let end = after.find("]]>")?;
            if let Some(top) = stack.last_mut().filter(|top| top.children.is_empty()) {
                top.text.push_str(&after[..end]);
            }
            rest = &after[end + 3..];
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = &rest[tag_end(rest)? + 1..];
        } else if let Some(after) = rest.strip_prefix("</") {
            rest = &after[after.find('>')? + 1..];
            close(&mut stack, &mut root);
        } else if let Some(after) = rest.strip_prefix('<') {
            let end = tag_end(after)?;
            let inside = &after[..end];
            let name_end = inside
                .find(|c: char| c.is_whitespace() || c == '/')
                .unwrap_or(inside.len());
            stack.push(Element {
                name: inside[..name_end].to_owned(),
                ..Element::default()
            });
            if inside.ends_with('/') {
                close(&mut stack, &mut root);
            }
            rest = &after[end + 1..];
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            if let Some(top) = stack.last_mut().filter(|top| top.children.is_empty()) {
                top.text.push_str(&decode(&rest[..end]));
            }
            rest = &rest[end..];
        }
    }
    root
}

fn item(element: &Element) -> Option<Item> {
    let config = element.child("configItem")?;
    let code = config.child("name").map(|name| name.text.as_str())?;
    if code.is_empty() {
        return None;
    }
    let name = config
        .child("description")
        .map(|description| description.text.as_str())
        .filter(|text| !text.is_empty())
        .unwrap_or(code);
    Some(Item {
        code: code.to_owned(),
        name: name.to_owned(),
    })
}

pub fn catalogue_of(root: &Element) -> Catalogue {
    let layouts = root
        .child("layoutList")
        .into_iter()
        .flat_map(|list| list.all("layout"))
        .filter_map(|element| {
            let layout = item(element)?;
            let variants = element
                .child("variantList")
                .into_iter()
                .flat_map(|list| list.all("variant"))
                .filter_map(item)
                .collect();
            Some(Layout {
                code: layout.code,
                name: layout.name,
                variants,
            })
        })
        .collect();
    let option_groups = root
        .child("optionList")
        .into_iter()
        .flat_map(|list| list.all("group"))
        .filter_map(|element| {
            let group = item(element)?;
            Some(OptionGroup {
                code: group.code,
                name: group.name,
                options: element.all("option").filter_map(item).collect(),
            })
        })
        .collect();
    Catalogue {
        layouts,
        option_groups,
    }
}

pub fn load() -> Catalogue {
    REGISTRIES
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| parse_xml(&text))
        .map(|root| catalogue_of(&root))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE xkbConfigRegistry SYSTEM "xkb.dtd">
<xkbConfigRegistry version="1.1">
  <!-- models are not read -->
  <modelList><model><configItem><name>pc105</name></configItem></model></modelList>
  <layoutList>
    <layout>
      <configItem>
        <name>us</name>
        <shortDescription>en</shortDescription>
        <description>English (US)</description>
      </configItem>
      <variantList>
        <variant>
          <configItem popularity="exotic">
            <name>dvorak</name>
            <description>English (Dvorak)</description>
          </configItem>
        </variant>
        <variant><configItem><name>intl</name><description/></configItem></variant>
      </variantList>
    </layout>
    <layout><configItem><name>ru</name><description>Russian &amp; more</description></configItem></layout>
    <layout><configItem><name></name></configItem></layout>
  </layoutList>
  <optionList>
    <group allowMultipleSelection="true">
      <configItem><name>grp</name><description>Switching to another layout</description></configItem>
      <option><configItem><name>grp:alt_shift_toggle</name><description>Alt+Shift</description></configItem></option>
    </group>
  </optionList>
</xkbConfigRegistry>
"#;

    fn item(code: &str, name: &str) -> Item {
        Item {
            code: code.to_owned(),
            name: name.to_owned(),
        }
    }

    #[test]
    fn the_registry_lists_layouts_with_variants_and_option_groups() {
        let root = parse_xml(REGISTRY).expect("the sample parses");
        assert_eq!(
            catalogue_of(&root),
            Catalogue {
                layouts: vec![
                    Layout {
                        code: "us".to_owned(),
                        name: "English (US)".to_owned(),
                        variants: vec![item("dvorak", "English (Dvorak)"), item("intl", "intl"),],
                    },
                    Layout {
                        code: "ru".to_owned(),
                        name: "Russian & more".to_owned(),
                        variants: vec![],
                    },
                ],
                option_groups: vec![OptionGroup {
                    code: "grp".to_owned(),
                    name: "Switching to another layout".to_owned(),
                    options: vec![item("grp:alt_shift_toggle", "Alt+Shift")],
                }],
            }
        );
    }
}
