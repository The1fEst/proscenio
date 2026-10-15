#[derive(Clone, Copy, PartialEq)]
pub enum Shape {
    Normal,
    Fn,
    Tab,
    Shift,
    Control,
    Space,
    Expand,
    Empty,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Normal,
    Modkey,
    Spacer,
}

pub struct Key {
    pub kind: Kind,
    pub label: &'static str,
    pub shift: Option<&'static str>,
    pub caps: Option<&'static str>,
    pub shape: Shape,
    pub code: u16,
}

pub struct Layout {
    pub name: &'static str,
    pub rows: &'static [&'static [Key]],
}

pub const DEFAULT: &str = "English (US)";

pub fn named(name: &str) -> &'static Layout {
    LAYOUTS
        .iter()
        .find(|layout| layout.name == name)
        .or_else(|| LAYOUTS.iter().find(|layout| layout.name == DEFAULT))
        .unwrap_or(&LAYOUTS[0])
}

const fn key(label: &'static str, shape: Shape, code: u16) -> Key {
    Key {
        kind: Kind::Normal,
        label,
        shift: None,
        caps: None,
        shape,
        code,
    }
}

const fn letter(label: &'static str, shift: &'static str, code: u16) -> Key {
    shifted(label, shift, Shape::Normal, code)
}

const fn shifted(label: &'static str, shift: &'static str, shape: Shape, code: u16) -> Key {
    Key {
        kind: Kind::Normal,
        label,
        shift: Some(shift),
        caps: None,
        shape,
        code,
    }
}

const fn modkey(label: &'static str, shape: Shape, code: u16) -> Key {
    Key {
        kind: Kind::Modkey,
        label,
        shift: None,
        caps: None,
        shape,
        code,
    }
}

const fn shift_key(
    label: &'static str,
    shift: &'static str,
    caps: &'static str,
    shape: Shape,
    code: u16,
) -> Key {
    Key {
        kind: Kind::Modkey,
        label,
        shift: Some(shift),
        caps: Some(caps),
        shape,
        code,
    }
}

const SPACER: Key = Key {
    kind: Kind::Spacer,
    label: "",
    shift: None,
    caps: None,
    shape: Shape::Empty,
    code: 0,
};

const fn function(label: &'static str, code: u16) -> Key {
    key(label, Shape::Fn, code)
}

pub const LAYOUTS: [Layout; 3] = [
    Layout {
        name: "English (US)",
        rows: &[
            &[
                function("Esc", 1),
                function("F1", 59),
                function("F2", 60),
                function("F3", 61),
                function("F4", 62),
                function("F5", 63),
                function("F6", 64),
                function("F7", 65),
                function("F8", 66),
                function("F9", 67),
                function("F10", 68),
                function("F11", 87),
                function("F12", 88),
                function("PrtSc", 99),
                function("Del", 111),
            ],
            &[
                letter("`", "~", 41),
                letter("1", "!", 2),
                letter("2", "@", 3),
                letter("3", "#", 4),
                letter("4", "$", 5),
                letter("5", "%", 6),
                letter("6", "^", 7),
                letter("7", "&", 8),
                letter("8", "*", 9),
                letter("9", "(", 10),
                letter("0", ")", 11),
                letter("-", "_", 12),
                letter("=", "+", 13),
                key("Backspace", Shape::Expand, 14),
            ],
            &[
                key("Tab", Shape::Tab, 15),
                letter("q", "Q", 16),
                letter("w", "W", 17),
                letter("e", "E", 18),
                letter("r", "R", 19),
                letter("t", "T", 20),
                letter("y", "Y", 21),
                letter("u", "U", 22),
                letter("i", "I", 23),
                letter("o", "O", 24),
                letter("p", "P", 25),
                letter("[", "{", 26),
                letter("]", "}", 27),
                shifted("\\", "|", Shape::Expand, 43),
            ],
            &[
                SPACER,
                SPACER,
                letter("a", "A", 30),
                letter("s", "S", 31),
                letter("d", "D", 32),
                letter("f", "F", 33),
                letter("g", "G", 34),
                letter("h", "H", 35),
                letter("j", "J", 36),
                letter("k", "K", 37),
                letter("l", "L", 38),
                letter(";", ":", 39),
                letter("'", "\"", 40),
                key("Enter", Shape::Expand, 28),
            ],
            &[
                shift_key("Shift", "Shift", "Caps", Shape::Shift, 42),
                letter("z", "Z", 44),
                letter("x", "X", 45),
                letter("c", "C", 46),
                letter("v", "V", 47),
                letter("b", "B", 48),
                letter("n", "N", 49),
                letter("m", "M", 50),
                letter(",", "<", 51),
                letter(".", ">", 52),
                letter("/", "?", 53),
                shift_key("Shift", "Shift", "Caps", Shape::Expand, 54),
            ],
            &[
                modkey("Ctrl", Shape::Control, 29),
                modkey("Alt", Shape::Normal, 56),
                key("Space", Shape::Space, 57),
                modkey("Alt", Shape::Normal, 100),
                key("Menu", Shape::Normal, 139),
                modkey("Ctrl", Shape::Control, 97),
            ],
        ],
    },
    Layout {
        name: "German",
        rows: &[
            &[
                function("Esc", 1),
                function("F1", 59),
                function("F2", 60),
                function("F3", 61),
                function("F4", 62),
                function("F5", 63),
                function("F6", 64),
                function("F7", 65),
                function("F8", 66),
                function("F9", 67),
                function("F10", 68),
                function("F11", 87),
                function("F12", 88),
                function("Druck", 99),
                function("Entf", 111),
            ],
            &[
                letter("^", "°", 41),
                letter("1", "!", 2),
                letter("2", "\"", 3),
                letter("3", "§", 4),
                letter("4", "$", 5),
                letter("5", "%", 6),
                letter("6", "&", 7),
                letter("7", "/", 8),
                letter("8", "(", 9),
                letter("9", ")", 10),
                letter("0", "=", 11),
                letter("ß", "?", 12),
                letter("´", "`", 13),
                key("⟵", Shape::Expand, 14),
            ],
            &[
                key("Tab ⇆", Shape::Tab, 15),
                letter("q", "Q", 16),
                letter("w", "W", 17),
                letter("e", "E", 18),
                letter("r", "R", 19),
                letter("t", "T", 20),
                letter("z", "Z", 21),
                letter("u", "U", 22),
                letter("i", "I", 23),
                letter("o", "O", 24),
                letter("p", "P", 25),
                letter("ü", "Ü", 26),
                letter("+", "*", 27),
                key("↵", Shape::Expand, 28),
            ],
            &[
                SPACER,
                SPACER,
                letter("a", "A", 30),
                letter("s", "S", 31),
                letter("d", "D", 32),
                letter("f", "F", 33),
                letter("g", "G", 34),
                letter("h", "H", 35),
                letter("j", "J", 36),
                letter("k", "K", 37),
                letter("l", "L", 38),
                letter("ö", "Ö", 39),
                letter("ä", "Ä", 40),
                letter("#", "'", 43),
                SPACER,
            ],
            &[
                shift_key("Shift", "Shift ⇧", "Locked ⇩", Shape::Shift, 42),
                letter("<", ">", 86),
                letter("y", "Y", 44),
                letter("x", "X", 45),
                letter("c", "C", 46),
                letter("v", "V", 47),
                letter("b", "B", 48),
                letter("n", "N", 49),
                letter("m", "M", 50),
                letter(",", ";", 51),
                letter(".", ":", 52),
                letter("-", "_", 53),
                shift_key("Shift", "Shift ⇧", "Locked ⇩", Shape::Expand, 54),
            ],
            &[
                modkey("Strg", Shape::Control, 29),
                modkey("Alt", Shape::Normal, 56),
                key("Leertaste", Shape::Space, 57),
                modkey("Alt Gr", Shape::Normal, 100),
                modkey("Strg", Shape::Control, 97),
                key("⇦", Shape::Normal, 105),
                key("⇨", Shape::Normal, 106),
            ],
        ],
    },
    Layout {
        name: "Russian",
        rows: &[
            &[
                function("Esc", 1),
                function("F1", 59),
                function("F2", 60),
                function("F3", 61),
                function("F4", 62),
                function("F5", 63),
                function("F6", 64),
                function("F7", 65),
                function("F8", 66),
                function("F9", 67),
                function("F10", 68),
                function("F11", 87),
                function("F12", 88),
                function("PrtSc", 99),
                function("Del", 111),
            ],
            &[
                letter("ё", "Ё", 41),
                letter("1", "!", 2),
                letter("2", "\"", 3),
                letter("3", "№", 4),
                letter("4", ";", 5),
                letter("5", "%", 6),
                letter("6", ":", 7),
                letter("7", "?", 8),
                letter("8", "*", 9),
                letter("9", "(", 10),
                letter("0", ")", 11),
                letter("-", "_", 12),
                letter("=", "+", 13),
                key("Backspace", Shape::Expand, 14),
            ],
            &[
                key("Tab", Shape::Tab, 15),
                letter("й", "Й", 16),
                letter("ц", "Ц", 17),
                letter("у", "У", 18),
                letter("к", "К", 19),
                letter("е", "Е", 20),
                letter("н", "Н", 21),
                letter("г", "Г", 22),
                letter("ш", "Ш", 23),
                letter("щ", "Щ", 24),
                letter("з", "З", 25),
                letter("х", "Х", 26),
                letter("ъ", "Ъ", 27),
                shifted("\\", "/", Shape::Expand, 43),
            ],
            &[
                SPACER,
                SPACER,
                letter("ф", "Ф", 30),
                letter("ы", "Ы", 31),
                letter("в", "В", 32),
                letter("а", "А", 33),
                letter("п", "П", 34),
                letter("р", "Р", 35),
                letter("о", "О", 36),
                letter("л", "Л", 37),
                letter("д", "Д", 38),
                letter("ж", "Ж", 39),
                letter("э", "Э", 40),
                key("Enter", Shape::Expand, 28),
            ],
            &[
                modkey("Shift", Shape::Shift, 42),
                letter("я", "Я", 44),
                letter("ч", "Ч", 45),
                letter("с", "С", 46),
                letter("м", "М", 47),
                letter("и", "И", 48),
                letter("т", "Т", 49),
                letter("ь", "Ь", 50),
                letter("б", "Б", 51),
                letter("ю", "Ю", 52),
                letter(".", ",", 53),
                modkey("Shift", Shape::Expand, 54),
            ],
            &[
                modkey("Ctrl", Shape::Control, 29),
                modkey("Alt", Shape::Normal, 56),
                key("Space", Shape::Space, 57),
                modkey("Alt", Shape::Normal, 100),
                key("Menu", Shape::Normal, 139),
                modkey("Ctrl", Shape::Control, 97),
            ],
        ],
    },
];
