use std::cell::RefCell;

pub struct Prepared {
    lower: Vec<u16>,
    plain: Vec<u16>,
    bitflags: u32,
    next: RefCell<Option<Vec<usize>>>,
}

struct Search {
    codes: Vec<u16>,
    lower: Vec<u16>,
    contains_space: bool,
    bitflags: u32,
    spaces: Vec<Search>,
}

struct Found {
    score: f64,
    indexes: Vec<usize>,
}

pub fn prepare(target: &str) -> Prepared {
    let plain: Vec<u16> = remove_accents(target).encode_utf16().collect();
    let info = lower_info(target);
    Prepared {
        lower: info.codes,
        plain,
        bitflags: info.bitflags,
        next: RefCell::new(None),
    }
}

pub fn go(search: &str, targets: &[&Prepared]) -> Vec<usize> {
    if search.is_empty() {
        return (0..targets.len()).collect();
    }
    let search = prepare_search(search);
    let mut queue = Queue::default();
    for (index, target) in targets.iter().enumerate() {
        if search.bitflags & target.bitflags != search.bitflags {
            continue;
        }
        let Some(found) = algorithm(&search, target, false) else {
            continue;
        };
        queue.add(found.score, index);
    }
    let mut results = vec![0; queue.len()];
    for slot in results.iter_mut().rev() {
        if let Some((_, index)) = queue.poll() {
            *slot = index;
        }
    }
    results
}

struct LowerInfo {
    codes: Vec<u16>,
    bitflags: u32,
    contains_space: bool,
}

fn lower_info(text: &str) -> LowerInfo {
    let lower = remove_accents(text).to_lowercase();
    let codes: Vec<u16> = lower.encode_utf16().collect();
    let mut bitflags = 0u32;
    let mut contains_space = false;
    for &code in &codes {
        if code == 32 {
            contains_space = true;
            continue;
        }
        let bit = match code {
            97..=122 => code - 97,
            48..=57 => 26,
            0..=127 => 30,
            _ => 31,
        };
        bitflags |= 1 << bit;
    }
    LowerInfo {
        codes,
        bitflags,
        contains_space,
    }
}

fn prepare_search(search: &str) -> Search {
    let search = search.trim();
    let info = lower_info(search);
    let mut spaces = Vec::new();
    if info.contains_space {
        let mut seen: Vec<&str> = Vec::new();
        for word in search.split_whitespace() {
            if seen.contains(&word) {
                continue;
            }
            seen.push(word);
            let word_info = lower_info(word);
            spaces.push(Search {
                codes: word_info.codes,
                lower: word.to_lowercase().encode_utf16().collect(),
                contains_space: false,
                bitflags: word_info.bitflags,
                spaces: Vec::new(),
            });
        }
    }
    Search {
        lower: info.codes.clone(),
        codes: info.codes,
        contains_space: info.contains_space,
        bitflags: info.bitflags,
        spaces,
    }
}

fn algorithm(search: &Search, prepared: &Prepared, allow_spaces: bool) -> Option<Found> {
    if !allow_spaces && search.contains_space {
        return algorithm_spaces(search, prepared);
    }
    let codes = &search.codes;
    let target = &prepared.lower;
    let search_len = codes.len();
    let target_len = target.len();
    if search_len == 0 {
        return None;
    }

    let mut simple: Vec<usize> = Vec::with_capacity(search_len);
    let mut search_index = 0;
    let mut target_index = 0;
    loop {
        if target.get(target_index) == Some(&codes[search_index]) {
            simple.push(target_index);
            search_index += 1;
            if search_index == search_len {
                break;
            }
        }
        target_index += 1;
        if target_index >= target_len {
            return None;
        }
    }

    if prepared.next.borrow().is_none() {
        prepared
            .next
            .replace(Some(next_beginning_indexes(&prepared.plain)));
    }
    let next_ref = prepared.next.borrow();
    let next = next_ref.as_ref()?;

    let mut search_index = 0;
    let mut success_strict = false;
    let mut strict: Vec<usize> = Vec::with_capacity(search_len);
    target_index = if simple[0] == 0 {
        0
    } else {
        next[simple[0] - 1]
    };
    let mut backtracks = 0;
    if target_index != target_len {
        loop {
            if target_index >= target_len {
                if search_index == 0 {
                    break;
                }
                backtracks += 1;
                if backtracks > 200 {
                    break;
                }
                search_index -= 1;
                let last = strict.pop().unwrap_or(0);
                target_index = next[last];
            } else if codes[search_index] == target[target_index] {
                strict.push(target_index);
                search_index += 1;
                if search_index == search_len {
                    success_strict = true;
                    break;
                }
                target_index += 1;
            } else {
                target_index = next[target_index];
            }
        }
    }

    let mut substring = if search_len <= 1 {
        None
    } else {
        index_of(target, &search.lower, simple[0])
    };
    let mut substring_beginning = match substring {
        Some(0) => true,
        Some(at) => next[at - 1] == at,
        None => false,
    };
    if let (Some(at), false) = (substring, substring_beginning) {
        let mut index = 0;
        while index < next.len() {
            if index > at {
                let matched = (0..search_len)
                    .all(|offset| target.get(index + offset) == Some(&codes[offset]));
                if matched {
                    substring = Some(index);
                    substring_beginning = true;
                    break;
                }
            }
            index = next[index];
        }
    }

    let score_of = |matches: &[usize]| -> f64 {
        let mut score = 0.0;
        let mut extra_groups = 0.0;
        for index in 1..search_len {
            if matches[index] as i64 - matches[index - 1] as i64 != 1 {
                score -= matches[index] as f64;
                extra_groups += 1.0;
            }
        }
        let unmatched =
            matches[search_len - 1] as f64 - matches[0] as f64 - (search_len as f64 - 1.0);
        score -= (12.0 + unmatched) * extra_groups;
        if matches[0] != 0 {
            score -= (matches[0] * matches[0]) as f64 * 0.2;
        }
        if !success_strict {
            score *= 1000.0;
        } else {
            let mut beginnings = 1;
            let mut index = next[0];
            while index < target_len {
                beginnings += 1;
                index = next[index];
            }
            if beginnings > 24 {
                score *= (beginnings - 24) as f64 * 10.0;
            }
        }
        let longer = (target_len as f64 - search_len as f64) / 2.0;
        score -= longer;
        let square = 1.0 + (search_len * search_len) as f64;
        if substring.is_some() {
            score /= square;
        }
        if substring_beginning {
            score /= square;
        }
        score -= longer;
        score
    };

    let best: Vec<usize> = match (success_strict, substring, substring_beginning) {
        (false, Some(at), _) | (true, Some(at), true) => (at..at + search_len).collect(),
        (false, None, _) => simple,
        (true, _, _) => strict,
    };
    let score = score_of(&best);
    Some(Found {
        score,
        indexes: best,
    })
}

fn algorithm_spaces(search: &Search, prepared: &Prepared) -> Option<Found> {
    let mut seen: Vec<usize> = Vec::new();
    let mut score = 0.0;
    let mut first_seen = 0usize;
    let mut changes: Vec<(usize, usize)> = Vec::new();
    let count = search.spaces.len();
    let reset = |changes: &[(usize, usize)]| {
        if let Some(next) = prepared.next.borrow_mut().as_mut() {
            for &(index, value) in changes.iter().rev() {
                next[index] = value;
            }
        }
    };

    for (position, word) in search.spaces.iter().enumerate() {
        let Some(found) = algorithm(word, prepared, false) else {
            reset(&changes);
            return None;
        };
        if position != count - 1 {
            let consecutive = found.indexes.windows(2).all(|pair| pair[1] - pair[0] == 1);
            if consecutive && let Some(next) = prepared.next.borrow_mut().as_mut() {
                let beginning = found.indexes[found.indexes.len() - 1] + 1;
                let replaced = next[beginning - 1];
                let mut index = beginning - 1;
                loop {
                    if next[index] != replaced {
                        break;
                    }
                    next[index] = beginning;
                    changes.push((index, replaced));
                    if index == 0 {
                        break;
                    }
                    index -= 1;
                }
            }
        }
        score += found.score / count as f64;
        if found.indexes[0] < first_seen {
            score -= (first_seen - found.indexes[0]) as f64 * 2.0;
        }
        first_seen = found.indexes[0];
        for index in found.indexes {
            if !seen.contains(&index) {
                seen.push(index);
            }
        }
    }

    reset(&changes);
    if let Some(whole) = algorithm(search, prepared, true)
        && whole.score > score
    {
        return Some(whole);
    }
    Some(Found {
        score,
        indexes: seen,
    })
}

fn index_of(haystack: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(haystack.len()));
    }
    if needle.len() > haystack.len() {
        return None;
    }
    (from..=haystack.len() - needle.len())
        .find(|&start| haystack[start..start + needle.len()] == *needle)
}

fn beginning_indexes(target: &[u16]) -> Vec<usize> {
    let mut found = Vec::new();
    let mut was_upper = false;
    let mut was_alphanumeric = false;
    for (index, &code) in target.iter().enumerate() {
        let upper = (65..=90).contains(&code);
        let alphanumeric = upper || (97..=122).contains(&code) || (48..=57).contains(&code);
        if upper && !was_upper || !was_alphanumeric || !alphanumeric {
            found.push(index);
        }
        was_upper = upper;
        was_alphanumeric = alphanumeric;
    }
    found
}

fn next_beginning_indexes(target: &[u16]) -> Vec<usize> {
    let len = target.len();
    let beginnings = beginning_indexes(target);
    let mut next = Vec::with_capacity(len);
    let mut last = beginnings.first().copied();
    let mut at = 0;
    for index in 0..len {
        match last {
            Some(value) if value > index => next.push(value),
            _ => {
                at += 1;
                last = beginnings.get(at).copied();
                next.push(last.unwrap_or(len));
            }
        }
    }
    next
}

fn remove_accents(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for letter in text.chars() {
        if latin(letter) {
            let mut buffer = [0u8; 4];
            out.push_str(&gtk4::glib::normalize(
                letter.encode_utf8(&mut buffer),
                gtk4::glib::NormalizeMode::Default,
            ));
        } else {
            out.push(letter);
        }
    }
    out.chars()
        .filter(|letter| !('\u{300}'..='\u{36f}').contains(letter))
        .collect()
}

fn latin(letter: char) -> bool {
    matches!(letter as u32,
        0x41..=0x5A | 0x61..=0x7A | 0xAA | 0xBA | 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0x2B8
        | 0x2E0..=0x2E4 | 0x1D00..=0x1D25 | 0x1D2C..=0x1D5C | 0x1D62..=0x1D65
        | 0x1D6B..=0x1D77 | 0x1D79..=0x1DBE | 0x1E00..=0x1EFF | 0x2071 | 0x207F
        | 0x2090..=0x209C | 0x212A | 0x212B | 0x2132 | 0x214E | 0x2160..=0x2188
        | 0x2C60..=0x2C7F | 0xA722..=0xA787 | 0xA78B..=0xA7CA | 0xA7F2..=0xA7FF
        | 0xAB30..=0xAB5A | 0xAB5C..=0xAB64 | 0xAB66..=0xAB69 | 0xFB00..=0xFB06
        | 0xFF21..=0xFF3A | 0xFF41..=0xFF5A)
}

#[derive(Default)]
struct Queue {
    items: Vec<(f64, usize)>,
}

impl Queue {
    fn len(&self) -> usize {
        self.items.len()
    }

    fn add(&mut self, score: f64, index: usize) {
        let item = (score, index);
        let mut at = self.items.len();
        self.items.push(item);
        while at > 0 {
            let parent = (at - 1) >> 1;
            if item.0 >= self.items[parent].0 {
                break;
            }
            self.items[at] = self.items[parent];
            at = parent;
        }
        self.items[at] = item;
    }

    fn poll(&mut self) -> Option<(f64, usize)> {
        if self.items.is_empty() {
            return None;
        }
        let top = self.items[0];
        let last = self.items.pop()?;
        if !self.items.is_empty() {
            self.items[0] = last;
            self.sift_down();
        }
        Some(top)
    }

    fn sift_down(&mut self) {
        let len = self.items.len();
        let value = self.items[0];
        let mut at = 0;
        let mut child = 1;
        while child < len {
            let right = child + 1;
            at = child;
            if right < len && self.items[right].0 < self.items[child].0 {
                at = right;
            }
            self.items[(at - 1) >> 1] = self.items[at];
            child = 1 + (at << 1);
        }
        let mut parent = if at == 0 { 0 } else { (at - 1) >> 1 };
        while at > 0 && value.0 < self.items[parent].0 {
            self.items[at] = self.items[parent];
            at = parent;
            parent = if at == 0 { 0 } else { (at - 1) >> 1 };
        }
        self.items[at] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::{go, prepare};

    fn order(search: &str, targets: &[&str]) -> Vec<usize> {
        let prepared: Vec<_> = targets.iter().map(|target| prepare(target)).collect();
        let references: Vec<_> = prepared.iter().collect();
        go(search, &references)
    }

    const TARGETS: [&str; 20] = [
        "Firefox ",
        "Files ",
        "Visual Studio Code ",
        "Kitty ",
        "Font Viewer ",
        "Fish ",
        "GNU Image Manipulation Program ",
        "Firewall ",
        "Dolphin ",
        "System Settings ",
        "Steam ",
        "Strawberry ",
        "Café ",
        "Разработка ",
        "Terminal ",
        "KWrite ",
        "Kate ",
        "Krita ",
        "Kdenlive ",
        "Qt Designer ",
    ];

    const EXPECTED: [(&str, &[usize]); 23] = [
        ("f", &[5, 1, 0, 7, 4, 12]),
        ("fi", &[5, 1, 0, 7, 4]),
        ("fir", &[0, 7, 4]),
        ("k", &[16, 3, 17, 15, 18]),
        ("kt", &[16, 3, 17, 15]),
        ("ki", &[3, 17, 15, 18]),
        ("st", &[10, 11, 2, 9]),
        ("straw", &[11]),
        ("straw berry", &[11]),
        ("sys set", &[9]),
        ("cafe", &[12]),
        ("разр", &[13]),
        ("gim", &[6]),
        ("code", &[2]),
        ("vsc", &[2]),
        ("te", &[14, 10, 16, 9, 15, 19, 11, 4, 2]),
        ("e", &[14, 10, 18, 16, 12, 1, 0, 7, 19, 9, 15, 11, 4, 6, 2]),
        ("de", &[19, 18, 2]),
        ("qt d", &[19]),
        ("zzz", &[]),
        ("file fire", &[]),
        ("  ", &[]),
        ("Fi", &[5, 1, 0, 7, 4]),
    ];

    #[test]
    fn orders_results_as_the_shells_fuzzysort_does() {
        for (search, expected) in EXPECTED {
            assert_eq!(
                order(search, &TARGETS),
                expected.to_vec(),
                "search {search:?}"
            );
        }
    }
}
