use alpm::Alpm;
use proscenio_protocol::Change;
use std::collections::BTreeSet;

const MAX_LISTED_CHANGES: usize = 5;

fn describe_change(change: &Change) -> String {
    let from = if change.from.is_empty() {
        "-"
    } else {
        &change.from
    };
    let to = if change.to.is_empty() {
        "removed"
    } else {
        &change.to
    };
    format!("{} {from} -> {to}", change.name)
}

fn list_changes<'a>(mut descriptions: impl Iterator<Item = &'a String>) -> String {
    let mut listed: Vec<String> = descriptions
        .by_ref()
        .take(MAX_LISTED_CHANGES)
        .cloned()
        .collect();
    let unlisted_count = descriptions.count();
    if unlisted_count > 0 {
        listed.push(format!("and {unlisted_count} more"));
    }
    listed.join(", ")
}

pub fn compare_changes(confirmed: &[Change], actual: &[Change]) -> Result<(), String> {
    let describe_all =
        |changes: &[Change]| -> BTreeSet<String> { changes.iter().map(describe_change).collect() };
    let (confirmed, actual) = (describe_all(confirmed), describe_all(actual));
    if confirmed == actual {
        return Ok(());
    }
    let parts: Vec<String> = [
        ("now", list_changes(actual.difference(&confirmed))),
        ("no longer", list_changes(confirmed.difference(&actual))),
    ]
    .into_iter()
    .filter(|(_, list)| !list.is_empty())
    .map(|(label, list)| format!("{label}: {list}"))
    .collect();
    Err(format!(
        "the packages to update changed since they were listed, nothing was installed ({})",
        parts.join("; ")
    ))
}

pub fn collect_changes(alpm: &Alpm) -> Vec<Change> {
    let installed_version = |name: &str| {
        alpm.localdb()
            .pkg(name)
            .map(|package| package.version().as_str().to_owned())
            .unwrap_or_default()
    };
    let mut changes: Vec<Change> = alpm
        .trans_add()
        .iter()
        .map(|package| {
            Change::new(
                package.name(),
                &installed_version(package.name()),
                package.version().as_str(),
            )
        })
        .collect();
    changes.extend(
        alpm.trans_remove()
            .iter()
            .map(|package| Change::new(package.name(), package.version().as_str(), "")),
    );
    changes.sort_by(|left, right| left.name.cmp(&right.name));
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_confirmed_packages_may_be_installed() {
        let confirmed = [
            Change::new("linux", "7.2.9-1", "7.2.10-1"),
            Change::new("libfoo", "", "1.0-1"),
        ];
        let reordered = [confirmed[1].clone(), confirmed[0].clone()];
        assert_eq!(compare_changes(&confirmed, &reordered), Ok(()));
        assert_eq!(compare_changes(&[], &[]), Ok(()));

        let extra = [
            confirmed.to_vec(),
            vec![Change::new("glibc", "2.42-1", "2.43-1")],
        ]
        .concat();
        let message = compare_changes(&confirmed, &extra).unwrap_err();
        assert!(message.contains("glibc 2.42-1 -> 2.43-1"), "{message}");
        assert!(message.contains("nothing was installed"), "{message}");

        let missing = compare_changes(&confirmed, &confirmed[..1]).unwrap_err();
        assert_eq!(
            missing,
            "the packages to update changed since they were listed, nothing was installed (no longer: libfoo - -> 1.0-1)"
        );

        let newer = [
            Change::new("linux", "7.2.9-1", "7.2.11-1"),
            confirmed[1].clone(),
        ];
        let message = compare_changes(&confirmed, &newer).unwrap_err();
        assert!(
            message.contains("7.2.11-1") && message.contains("7.2.10-1"),
            "{message}"
        );

        let removal = [confirmed.to_vec(), vec![Change::new("oldlib", "2.0-1", "")]].concat();
        assert!(compare_changes(&confirmed, &removal).is_err());
    }

    #[test]
    fn a_mismatch_lists_a_few_packages_and_counts_the_rest() {
        let many: Vec<Change> = (0..MAX_LISTED_CHANGES + 3)
            .map(|index| Change::new(&format!("package{index}"), "1-1", "2-1"))
            .collect();
        let message = compare_changes(&[], &many).unwrap_err();
        assert_eq!(message.matches("-> 2-1").count(), MAX_LISTED_CHANGES);
        assert!(message.contains("and 3 more"), "{message}");
    }
}
