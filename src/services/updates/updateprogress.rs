use proscenio_protocol::{Event, Stage};

const CHECK: f64 = 0.1;
const WORK: f64 = 0.8;
const HOOKS: f64 = 0.1;
const DOWNLOAD: f64 = 0.4;
const BUILD: f64 = 0.6;

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Checking,
    Downloading {
        received_bytes: u64,
        total_bytes: u64,
    },
    Installing {
        index: u32,
        count: u32,
        name: String,
    },
    Hooks {
        index: u32,
        count: u32,
    },
    Building {
        name: String,
        built_count: u32,
        count: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Repositories,
    Aur,
}

#[derive(Clone, Copy, Debug, Default)]
struct Part {
    weight: f64,
    check: f64,
    work: f64,
    hooks: f64,
    is_done: bool,
}

impl Part {
    fn fraction(&self) -> f64 {
        if self.is_done {
            return 1.0;
        }
        CHECK * self.check + WORK * self.work + HOOKS * self.hooks
    }
}

#[derive(Clone, Debug)]
pub struct UpdateProgress {
    repositories: Part,
    aur: Part,
    phase: Phase,
    download_bytes: u64,
    packages_to_build: u32,
    built_count: u32,
    step: Step,
    displayed_fraction: f64,
}

impl UpdateProgress {
    pub fn system(repository_count: usize, aur_count: usize, has_aur: bool) -> Self {
        let (repository_weight, aur_weight) = match (has_aur, repository_count > 0, aur_count > 0) {
            (false, _, _) => (1.0, 0.0),
            (true, true, false) => (0.9, 0.1),
            (true, false, true) => (0.1, 0.9),
            _ => (0.5, 0.5),
        };
        UpdateProgress {
            repositories: Part {
                weight: repository_weight,
                ..Part::default()
            },
            aur: Part {
                weight: aur_weight,
                ..Part::default()
            },
            phase: Phase::Repositories,
            download_bytes: 0,
            packages_to_build: 0,
            built_count: 0,
            step: Step::Checking,
            displayed_fraction: 0.0,
        }
    }

    pub fn step(&self) -> &Step {
        &self.step
    }

    pub fn fraction(&mut self) -> f64 {
        let fraction = self.repositories.weight * self.repositories.fraction()
            + self.aur.weight * self.aur.fraction();
        self.displayed_fraction = self.displayed_fraction.max(fraction.min(1.0));
        self.displayed_fraction
    }

    pub fn finish(&mut self) {
        self.repositories.is_done = true;
        self.aur.is_done = true;
    }

    pub fn start_aur(&mut self) {
        self.repositories.is_done = true;
        self.phase = Phase::Aur;
        self.step = Step::Checking;
    }

    pub fn handle_event(&mut self, event: &Event) {
        let phase = self.phase;
        let part = match phase {
            Phase::Repositories => &mut self.repositories,
            Phase::Aur => &mut self.aur,
        };
        match event {
            Event::Stage(Stage::Synchronize) => self.step = Step::Checking,
            Event::Download {
                received_bytes,
                total_bytes,
            } if phase == Phase::Repositories => {
                let ratio = ratio(*received_bytes, *total_bytes);
                if part.check < 1.0 {
                    part.check = ratio;
                } else if self.download_bytes > 0 {
                    part.work = part.work.max(DOWNLOAD * ratio);
                }
                if part.check >= 1.0 {
                    self.step = Step::Downloading {
                        received_bytes: *received_bytes,
                        total_bytes: *total_bytes,
                    };
                }
            }
            Event::Transaction {
                package_count,
                download_bytes,
            } if phase == Phase::Repositories => {
                part.check = 1.0;
                self.download_bytes = *download_bytes;
                if *package_count == 0 {
                    part.is_done = true;
                } else if *download_bytes == 0 {
                    part.work = DOWNLOAD;
                }
            }
            Event::Stage(Stage::Install) if phase == Phase::Repositories => {
                part.work = part.work.max(DOWNLOAD);
            }
            Event::Package {
                index,
                count,
                name,
                percent,
            } => {
                let install_start = match phase {
                    Phase::Repositories => DOWNLOAD,
                    Phase::Aur => BUILD,
                };
                let install_fraction = (index.saturating_sub(1) as f64 + *percent as f64 / 100.0)
                    / (*count).max(1) as f64;
                part.work = part
                    .work
                    .max(install_start + (1.0 - install_start) * install_fraction.min(1.0));
                self.step = Step::Installing {
                    index: *index,
                    count: *count,
                    name: name.clone(),
                };
            }
            Event::Stage(Stage::Hooks) => part.work = 1.0,
            Event::Hook { index, count, .. } => {
                part.work = 1.0;
                part.hooks = part.hooks.max(ratio(*index as u64, *count as u64));
                self.step = Step::Hooks {
                    index: *index,
                    count: *count,
                };
            }
            Event::Done { .. } if phase == Phase::Repositories => part.is_done = true,
            _ => {}
        }
    }

    pub fn handle_aur_line(&mut self, line: &str) {
        if self.phase != Phase::Aur {
            return;
        }
        let line = line.trim_end();
        if line.trim() == "there is nothing to do" {
            self.aur.is_done = true;
        } else if let Some(count) = parse_count(line, "Aur (") {
            self.aur.check = 1.0;
            self.packages_to_build = count;
            if count == 0 {
                self.aur.is_done = true;
            }
        } else if let Some(rest) = line.strip_prefix("==> Making package: ") {
            self.step = Step::Building {
                name: rest.split_whitespace().next().unwrap_or(rest).to_owned(),
                built_count: self.built_count,
                count: self.packages_to_build,
            };
        } else if line.starts_with("==> Finished making:") {
            self.built_count += 1;
            self.aur.work = self
                .aur
                .work
                .max(BUILD * ratio(self.built_count as u64, self.packages_to_build as u64));
        }
    }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    (numerator as f64 / denominator as f64).min(1.0)
}

fn parse_count(line: &str, prefix: &str) -> Option<u32> {
    line.strip_prefix(prefix)?.split_once(')')?.0.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    fn package(index: u32, count: u32, percent: u32) -> Event {
        Event::Package {
            index,
            count,
            name: "a".to_owned(),
            percent,
        }
    }

    #[test]
    fn the_bar_is_shared_by_the_sources_that_have_packages() {
        let weights =
            |progress: UpdateProgress| (progress.repositories.weight, progress.aur.weight);
        assert_eq!(weights(UpdateProgress::system(159, 3, true)), (0.5, 0.5));
        assert_eq!(weights(UpdateProgress::system(159, 0, true)), (0.9, 0.1));
        assert_eq!(weights(UpdateProgress::system(0, 3, true)), (0.1, 0.9));
        assert_eq!(weights(UpdateProgress::system(159, 3, false)), (1.0, 0.0));
    }

    #[test]
    fn the_repositories_move_through_check_download_install_and_hooks() {
        let mut progress = UpdateProgress::system(4, 0, true);
        progress.handle_event(&Event::Stage(Stage::Synchronize));
        progress.handle_event(&Event::Download {
            received_bytes: 50,
            total_bytes: 100,
        });
        assert!(is_close(progress.fraction(), 0.9 * CHECK * 0.5));
        progress.handle_event(&Event::Transaction {
            package_count: 4,
            download_bytes: 1000,
        });
        assert!(is_close(progress.fraction(), 0.9 * CHECK));
        progress.handle_event(&Event::Stage(Stage::Download));
        progress.handle_event(&Event::Download {
            received_bytes: 500,
            total_bytes: 1000,
        });
        assert_eq!(
            progress.step(),
            &Step::Downloading {
                received_bytes: 500,
                total_bytes: 1000
            }
        );
        assert!(is_close(
            progress.fraction(),
            0.9 * (CHECK + WORK * DOWNLOAD * 0.5)
        ));
        progress.handle_event(&Event::Stage(Stage::Install));
        progress.handle_event(&package(3, 4, 0));
        assert!(is_close(
            progress.fraction(),
            0.9 * (CHECK + WORK * (DOWNLOAD + (1.0 - DOWNLOAD) * 0.5))
        ));
        progress.handle_event(&Event::Stage(Stage::Hooks));
        progress.handle_event(&Event::Hook {
            index: 2,
            count: 4,
            name: "a".to_owned(),
        });
        assert!(is_close(
            progress.fraction(),
            0.9 * (CHECK + WORK + HOOKS * 0.5)
        ));
        progress.handle_event(&Event::Done {
            is_ok: true,
            message: String::new(),
        });
        assert!(is_close(progress.fraction(), 0.9));
        progress.start_aur();
        progress.handle_aur_line(" there is nothing to do");
        assert!(is_close(progress.fraction(), 1.0));
    }

    #[test]
    fn cached_packages_skip_the_download_part() {
        let mut progress = UpdateProgress::system(2, 0, false);
        progress.handle_event(&Event::Transaction {
            package_count: 2,
            download_bytes: 0,
        });
        assert!(is_close(progress.fraction(), CHECK + WORK * DOWNLOAD));
    }

    #[test]
    fn aur_packages_are_built_then_installed_through_the_session() {
        let mut progress = UpdateProgress::system(0, 2, true);
        progress.handle_event(&Event::Transaction {
            package_count: 0,
            download_bytes: 0,
        });
        assert!(is_close(progress.fraction(), 0.1));
        progress.start_aur();
        for line in [
            ":: Looking for AUR upgrades...",
            "Aur (2) a-bin-2-1  b-bin-3-1",
            "==> Making package: a-bin 2-1 (Wed 14 Oct 2026)",
        ] {
            progress.handle_aur_line(line);
        }
        assert_eq!(
            progress.step(),
            &Step::Building {
                name: "a-bin".to_owned(),
                built_count: 0,
                count: 2
            }
        );
        assert!(is_close(progress.fraction(), 0.1 + 0.9 * CHECK));
        progress.handle_aur_line("==> Finished making: a-bin 2-1 (Wed 14 Oct 2026)");
        progress.handle_aur_line("==> Finished making: b-bin 3-1 (Wed 14 Oct 2026)");
        assert!(is_close(
            progress.fraction(),
            0.1 + 0.9 * (CHECK + WORK * BUILD)
        ));
        progress.handle_event(&Event::Transaction {
            package_count: 2,
            download_bytes: 0,
        });
        progress.handle_event(&package(2, 2, 100));
        assert!(is_close(progress.fraction(), 0.1 + 0.9 * (CHECK + WORK)));
        progress.finish();
        assert!(is_close(progress.fraction(), 1.0));
    }

    #[test]
    fn the_bar_never_moves_back() {
        let mut progress = UpdateProgress::system(2, 0, false);
        progress.handle_event(&Event::Transaction {
            package_count: 2,
            download_bytes: 0,
        });
        progress.handle_event(&package(2, 2, 50));
        let before = progress.fraction();
        progress.handle_event(&package(1, 2, 0));
        assert!(progress.fraction() >= before);
    }
}
