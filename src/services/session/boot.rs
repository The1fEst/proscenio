use std::fmt;

const WINDOWS_LABEL: &str = "Windows Boot Manager";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootNumber(u16);

impl BootNumber {
    fn parse(text: &str) -> Option<Self> {
        if text.len() != 4 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        u16::from_str_radix(text, 16).ok().map(Self)
    }
}

impl fmt::Display for BootNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{:04X}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootEntry {
    pub number: BootNumber,
    pub label: String,
    pub is_current: bool,
}

pub fn parse_boot_list(text: &str) -> Vec<BootEntry> {
    let mut current = None;
    let mut order = Vec::new();
    let mut entries = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("BootCurrent:") {
            current = current.or_else(|| BootNumber::parse(value.trim()));
        } else if let Some(value) = line.strip_prefix("BootOrder:") {
            order = value
                .split(',')
                .filter_map(|item| BootNumber::parse(item.trim()))
                .collect();
        } else if let Some(entry) = parse_entry(line) {
            entries.push(entry);
        }
    }
    for entry in &mut entries {
        entry.is_current = Some(entry.number) == current;
    }
    entries.sort_by_key(|entry| {
        order
            .iter()
            .position(|number| *number == entry.number)
            .unwrap_or(order.len())
    });
    let mut windows = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.label.contains(WINDOWS_LABEL));
    if let (Some((index, _)), None) = (windows.next(), windows.next()) {
        let entry = entries.remove(index);
        entries.insert(0, entry);
    }
    entries
}

fn parse_entry(line: &str) -> Option<BootEntry> {
    let rest = line.strip_prefix("Boot")?;
    let number = BootNumber::parse(rest.get(..4)?)?;
    let rest = &rest[4..];
    let rest = rest.strip_prefix('*').unwrap_or(rest);
    if !rest.starts_with(' ') {
        return None;
    }
    let label = rest.trim_start_matches(' ').split('\t').next()?.trim();
    (!label.is_empty()).then(|| BootEntry {
        number,
        label: label.to_owned(),
        is_current: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_rows(text: &str) -> Vec<(String, String, bool)> {
        parse_boot_list(text)
            .into_iter()
            .map(|entry| (entry.number.to_string(), entry.label, entry.is_current))
            .collect()
    }

    fn make_row(number: &str, label: &str, is_current: bool) -> (String, String, bool) {
        (number.to_owned(), label.to_owned(), is_current)
    }

    #[test]
    fn a_boot_number_is_exactly_four_hex_digits() {
        let cases = [
            ("0000", Some("0000")),
            ("000a", Some("000A")),
            ("FFFF", Some("FFFF")),
            ("00000", None),
            ("000", None),
            ("", None),
            ("+FFF", None),
            ("-001", None),
            ("0x1F", None),
            ("12 3", None),
            ("GHIJ", None),
            ("٠١٢٣", None),
            ("0001;", None),
        ];
        for (text, expected) in cases {
            assert_eq!(
                BootNumber::parse(text).map(|number| number.to_string()),
                expected.map(str::to_owned),
                "{text:?}"
            );
        }
    }

    #[test]
    fn entries_come_in_boot_order_with_the_current_one_marked_and_the_path_dropped() {
        let text = "BootCurrent: 0001\n\
            Timeout: 1 seconds\n\
            BootOrder: 0001,0000,0006,0003,0007,0008,0009,000A,000B\n\
            Boot0000* Windows Boot Manager\tHD(1,GPT,db658915-0aae-4e58-9426-04f31c1a9f33,0x800,0x64000)/\\EFI\\Microsoft\\Boot\\bootmgfw.efi57494e444f5753000100000088000000\n\
            Boot0001* Linux Boot Manager\tHD(1,GPT,aef86a36-6348-4cef-a1e2-d0d139deea27,0x800,0x200000)/\\EFI\\systemd\\systemd-bootx64.efi\n\
            Boot0003* UEFI: Generic USB Flash Disk 7.76, Partition 1\tPciRoot(0x0)/Pci(0x8,0x1)/USB(3,0)/HD(1,MBR,0x9fd746aa,0x800,0x1deb800)0000424f\n\
            Boot0006* UEFI OS\tHD(1,GPT,aef86a36-6348-4cef-a1e2-d0d139deea27,0x800,0x200000)/\\EFI\\BOOT\\BOOTX64.EFI0000424f\n\
            Boot0009* UEFI:CD/DVD Drive\tBBS(129,,0x0)\n\
            Boot000A* UEFI:Removable Device\tBBS(130,,0x0)\n";
        assert_eq!(
            parse_rows(text),
            [
                make_row("0000", "Windows Boot Manager", false),
                make_row("0001", "Linux Boot Manager", true),
                make_row("0006", "UEFI OS", false),
                make_row(
                    "0003",
                    "UEFI: Generic USB Flash Disk 7.76, Partition 1",
                    false
                ),
                make_row("0009", "UEFI:CD/DVD Drive", false),
                make_row("000A", "UEFI:Removable Device", false),
            ]
        );
    }

    #[test]
    fn inactive_and_unordered_entries_follow_the_boot_order_in_listed_order() {
        let text = "BootCurrent: 0010\n\
            BootNext: 0001\n\
            BootOrder: 0010,0001\n\
            Boot0000  Windows Boot Manager\n\
            Boot0001* Limine\n\
            Boot0010* Fedora\tHD(1,GPT)\n\
            Boot001a  Ubuntu\n\
            Boot001B* Arch\n\
            BootXYZW* Not an entry\n\
            Boot0002*NoSpace\n\
            Boot0003\n\
            Boot0004* \tHD(1,GPT)/\\EFI\\BOOT\\BOOTX64.EFI\n\
            Boot0005   \n";
        assert_eq!(
            parse_rows(text),
            [
                make_row("0000", "Windows Boot Manager", false),
                make_row("0010", "Fedora", true),
                make_row("0001", "Limine", false),
                make_row("001A", "Ubuntu", false),
                make_row("001B", "Arch", false),
            ]
        );
    }

    #[test]
    fn windows_moves_first_only_when_one_entry_names_it() {
        let text = "BootOrder: 0001,0002,0003\n\
            Boot0001* Linux Boot Manager\n\
            Boot0002* Windows Boot Manager\tHD(1,GPT)\n\
            Boot0003* Windows Boot Manager\tHD(2,GPT)\n";
        assert_eq!(
            parse_rows(text),
            [
                make_row("0001", "Linux Boot Manager", false),
                make_row("0002", "Windows Boot Manager", false),
                make_row("0003", "Windows Boot Manager", false),
            ]
        );
        let text = "BootOrder: 0001,0002\n\
            Boot0001* Linux Boot Manager\n\
            Boot0002* Old Windows Boot Manager copy\n";
        assert_eq!(
            parse_rows(text),
            [
                make_row("0002", "Old Windows Boot Manager copy", false),
                make_row("0001", "Linux Boot Manager", false),
            ]
        );
    }

    #[test]
    fn a_list_without_current_or_order_keeps_the_printed_order() {
        assert_eq!(
            parse_rows("Boot0002* Second\nBoot0001* First\n"),
            [
                make_row("0002", "Second", false),
                make_row("0001", "First", false)
            ]
        );
        assert_eq!(
            parse_rows("BootCurrent: zz\nBootOrder: 00G1,0001\nBoot0002* Second\nBoot0001* First"),
            [
                make_row("0001", "First", false),
                make_row("0002", "Second", false)
            ]
        );
    }

    #[test]
    fn output_without_entries_lists_nothing() {
        for text in [
            "",
            "\n",
            "EFI variables are not supported on this system.\n",
            "BootCurrent: 0001\nBootOrder: 0001\n",
        ] {
            assert_eq!(parse_rows(text), [], "{text:?}");
        }
    }
}
