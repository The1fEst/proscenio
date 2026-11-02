use std::ops::Range;

const MAX_MATCHES: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Forward,
    Backward,
}

fn fold(character: char) -> char {
    let mut lower = character.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(only), None) => only,
        _ => character,
    }
}

fn build_failure_table(pattern: &[char]) -> Vec<usize> {
    let mut table = vec![0; pattern.len()];
    let mut length = 0;
    for index in 1..pattern.len() {
        while length > 0 && pattern[index] != pattern[length] {
            length = table[length - 1];
        }
        if pattern[index] == pattern[length] {
            length += 1;
        }
        table[index] = length;
    }
    table
}

pub fn find_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    let pattern: Vec<char> = query.chars().map(fold).collect();
    if pattern.is_empty() {
        return Vec::new();
    }
    let failure = build_failure_table(&pattern);
    let mut matches = Vec::new();
    let mut length = 0;
    for (offset, character) in text.chars().map(fold).enumerate() {
        while length > 0 && pattern[length] != character {
            length = failure[length - 1];
        }
        if pattern[length] == character {
            length += 1;
        }
        if length == pattern.len() {
            matches.push(offset + 1 - length..offset + 1);
            if matches.len() == MAX_MATCHES {
                break;
            }
            length = failure[length - 1];
        }
    }
    matches
}

pub fn locate(matches: &[Range<usize>], cursor: usize, direction: Direction) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    match direction {
        Direction::Forward => Some(
            matches
                .iter()
                .position(|found| found.start >= cursor)
                .unwrap_or(0),
        ),
        Direction::Backward => Some(
            matches
                .iter()
                .rposition(|found| found.start < cursor)
                .unwrap_or(matches.len() - 1),
        ),
    }
}

pub fn step_index(count: usize, current: usize, direction: Direction) -> usize {
    match direction {
        Direction::Forward => (current + 1) % count,
        Direction::Backward => (current + count - 1) % count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_are_found_by_character_position_whatever_the_case() {
        let cases = [
            (
                "Hello hello HELLO",
                "hello",
                vec![(0, 5), (6, 11), (12, 17)],
            ),
            ("aaaa", "aa", vec![(0, 2), (1, 3), (2, 4)]),
            ("abababc", "abc", vec![(4, 7)]),
            ("Привет, привет", "ПРИВЕТ", vec![(0, 6), (8, 14)]),
            ("日本語の日本", "日本", vec![(0, 2), (4, 6)]),
            ("İstanbul i", "i", vec![(9, 10)]),
            ("two\nlines two", "two", vec![(0, 3), (10, 13)]),
            ("nothing here", "xyz", vec![]),
            ("short", "longer than the text", vec![]),
            ("anything", "", vec![]),
            ("", "a", vec![]),
        ];
        for (text, query, expected) in cases {
            let found: Vec<(usize, usize)> = find_matches(text, query)
                .into_iter()
                .map(|found| (found.start, found.end))
                .collect();
            assert_eq!(found, expected, "{query:?} in {text:?}");
        }
    }

    #[test]
    fn the_number_of_matches_is_capped() {
        let text = "a".repeat(MAX_MATCHES + 10);
        assert_eq!(find_matches(&text, "a").len(), MAX_MATCHES);
    }

    #[test]
    fn the_first_match_after_the_cursor_wins_and_the_search_wraps_around() {
        let matches = [2..4, 10..12, 20..22];
        let cases = [
            (0, Direction::Forward, Some(0)),
            (2, Direction::Forward, Some(0)),
            (3, Direction::Forward, Some(1)),
            (20, Direction::Forward, Some(2)),
            (21, Direction::Forward, Some(0)),
            (0, Direction::Backward, Some(2)),
            (2, Direction::Backward, Some(2)),
            (3, Direction::Backward, Some(0)),
            (10, Direction::Backward, Some(0)),
            (11, Direction::Backward, Some(1)),
            (99, Direction::Backward, Some(2)),
        ];
        for (cursor, direction, expected) in cases {
            assert_eq!(locate(&matches, cursor, direction), expected, "{cursor}");
        }
        assert_eq!(locate(&[], 5, Direction::Forward), None);
        assert_eq!(locate(&[], 5, Direction::Backward), None);
    }

    #[test]
    fn stepping_between_matches_wraps_in_both_directions() {
        assert_eq!(step_index(3, 0, Direction::Forward), 1);
        assert_eq!(step_index(3, 2, Direction::Forward), 0);
        assert_eq!(step_index(3, 1, Direction::Backward), 0);
        assert_eq!(step_index(3, 0, Direction::Backward), 2);
        assert_eq!(step_index(1, 0, Direction::Forward), 0);
        assert_eq!(step_index(1, 0, Direction::Backward), 0);
    }
}
