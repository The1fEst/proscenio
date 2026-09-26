const THRESHOLD: f64 = 0.2;

pub fn sloppy() -> bool {
    crate::core::config::value_bool("/search/sloppy", false)
}

/// The indices of `texts` scoring above the threshold against `query`, the best first.
pub fn rank<'a>(
    texts: impl Iterator<Item = &'a str>,
    query: &str,
    scoring: fn(&str, &str) -> f64,
) -> Vec<usize> {
    let query = query.to_lowercase();
    let mut scored: Vec<(usize, f64)> = texts
        .enumerate()
        .map(|(index, text)| (index, scoring(&text.to_lowercase(), &query)))
        .filter(|(_, score)| *score > THRESHOLD)
        .collect();
    scored.sort_by(|left, right| right.1.total_cmp(&left.1));
    scored.into_iter().map(|(index, _)| index).collect()
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

pub fn distance(first: &[u16], second: &[u16]) -> usize {
    let (long, short) = if second.len() > first.len() {
        (second, first)
    } else {
        (first, second)
    };
    if short.is_empty() {
        return long.len();
    }
    let mut previous: Vec<usize> = (0..=short.len()).collect();
    let mut current = vec![0; short.len() + 1];
    for (row, long_unit) in long.iter().enumerate() {
        current[0] = row + 1;
        for (column, short_unit) in short.iter().enumerate() {
            let cost = usize::from(long_unit != short_unit);
            current[column + 1] = (previous[column + 1] + 1)
                .min(current[column] + 1)
                .min(previous[column] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[short.len()]
}

fn partial_ratio(short: &[u16], long: &[u16]) -> f64 {
    if short.is_empty() {
        return 1.0;
    }
    if long.len() < short.len() {
        return 0.0;
    }
    (0..=long.len() - short.len())
        .map(|start| {
            let window = &long[start..start + short.len()];
            1.0 - distance(short, window) as f64 / short.len() as f64
        })
        .fold(0.0, f64::max)
}

fn common_prefix(first: &[u16], second: &[u16]) -> usize {
    first
        .iter()
        .zip(second)
        .take_while(|(left, right)| left == right)
        .count()
}

fn contains(haystack: &[u16], needle: &[u16]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

struct Parts {
    full: f64,
    part: f64,
    longest: f64,
    difference: usize,
    prefix: usize,
    nested: bool,
}

fn parts(first: &[u16], second: &[u16]) -> Option<Parts> {
    let longest = first.len().max(second.len());
    if longest == 0 {
        return None;
    }
    let full = 1.0 - distance(first, second) as f64 / longest as f64;
    let part = if first.len() < second.len() {
        partial_ratio(first, second)
    } else {
        partial_ratio(second, first)
    };
    Some(Parts {
        full,
        part,
        longest: longest as f64,
        difference: first.len().abs_diff(second.len()),
        prefix: common_prefix(first, second),
        nested: contains(first, second) || contains(second, first),
    })
}

/// How well a whole name matches a query, from 0 to 1.
pub fn score(name: &str, query: &str) -> f64 {
    let (first, second) = (units(name), units(query));
    if first == second {
        return 1.0;
    }
    let Some(parts) = parts(&first, &second) else {
        return 1.0;
    };
    let mut score = 0.85 * parts.full + 0.15 * parts.part;
    if !first.is_empty() && !second.is_empty() && first[0] != second[0] {
        score -= 0.05;
    }
    if parts.difference >= 3 {
        score -= 0.05 * parts.difference as f64 / parts.longest;
    }
    score += 0.02 * parts.prefix as f64;
    if parts.nested {
        score += 0.06;
    }
    score.clamp(0.0, 1.0)
}

/// How well a longer text matches a query, rating it higher when the query appears inside it.
pub fn text_match_score(text: &str, query: &str) -> f64 {
    let (first, second) = (units(text), units(query));
    if first == second {
        return 1.0;
    }
    let Some(parts) = parts(&first, &second) else {
        return 1.0;
    };
    let mut score = 0.4 * parts.full + 0.6 * parts.part;
    if parts.difference >= 10 {
        score -= 0.02 * parts.difference as f64 / parts.longest;
    }
    score += 0.01 * parts.prefix as f64;
    if parts.nested {
        score += 0.2;
    }
    score.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_match_the_javascript_they_replace() {
        let cases = [
            ("firefox", "firefox", 1.0, 1.0),
            ("firefox", "firefx", 0.9535714285714286, 0.892857142857143),
            ("visual studio code", "code", 0.31, 0.8733333333333333),
            (
                "gimp",
                "gnu image manipulation program",
                0.16499999999999995,
                0.34600000000000003,
            ),
            ("kitty", "kity", 0.8525, 0.8),
            ("", "", 1.0, 1.0),
            ("abc", "", 0.15999999999999998, 0.8),
            ("thunderbird", "thund", 0.6690909090909092, 1.0),
            ("😀 grinning face", "grin", 0.335, 0.885),
        ];
        for (name, query, whole, text) in cases {
            assert!(
                (score(name, query) - whole).abs() < 1e-9,
                "{name} / {query}"
            );
            assert!(
                (text_match_score(name, query) - text).abs() < 1e-9,
                "{name} / {query}"
            );
        }
    }
}
