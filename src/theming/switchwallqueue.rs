use gtk4::glib;
use std::cell::RefCell;
use std::collections::VecDeque;

use crate::core::process;
use crate::theming::switchwall::is_hex;

thread_local! {
    static QUEUE: RefCell<RequestQueue> = RefCell::default();
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ColorRequest {
    Pick,
    Clear,
    Hex(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Request {
    pub(super) image: Option<String>,
    pub(super) monitor: Option<String>,
    pub(super) mode: Option<String>,
    pub(super) scheme: Option<String>,
    pub(super) color: Option<ColorRequest>,
    pub(super) keeps_wallpaper: bool,
}

impl Request {
    pub(super) fn parse<S: AsRef<str>>(arguments: &[S]) -> Self {
        let mut request = Self::default();
        let mut rest = arguments
            .iter()
            .map(|argument| argument.as_ref())
            .peekable();
        while let Some(argument) = rest.next() {
            match argument {
                "--mode" => request.mode = rest.next().map(str::to_owned),
                "--type" => request.scheme = rest.next().map(str::to_owned),
                "--color" => {
                    request.color = Some(match rest.peek().copied() {
                        Some(value) if is_hex(value) => {
                            rest.next();
                            ColorRequest::Hex(value.to_owned())
                        }
                        Some("clear") => {
                            rest.next();
                            ColorRequest::Clear
                        }
                        _ => ColorRequest::Pick,
                    });
                }
                "--image" => request.image = rest.next().map(str::to_owned),
                "--monitor" => request.monitor = rest.next().map(str::to_owned),
                "--noswitch" => request.keeps_wallpaper = true,
                _ => {
                    if request.image.is_none() {
                        request.image = Some(argument.to_owned());
                    }
                }
            }
        }
        if request.keeps_wallpaper {
            request.image = None;
            request.monitor = None;
        }
        request
    }

    fn is_prompt(&self) -> bool {
        self.image.is_none() && self.color.is_none() && !self.keeps_wallpaper
    }

    fn merge(self, newer: Self) -> Self {
        if self.is_prompt() || newer.is_prompt() {
            return newer;
        }
        let replaces_color = newer.image.is_some() && newer.color.is_none();
        let image = newer.image.or(self.image);
        Self {
            keeps_wallpaper: (self.keeps_wallpaper || newer.keeps_wallpaper) && image.is_none(),
            image,
            monitor: self.monitor,
            mode: newer.mode.or(self.mode),
            scheme: newer.scheme.or(self.scheme),
            color: if replaces_color {
                None
            } else {
                newer.color.or(self.color)
            },
        }
    }

    fn to_arguments(&self) -> Vec<String> {
        let mut arguments = Vec::new();
        for (flag, value) in [
            ("--mode", &self.mode),
            ("--type", &self.scheme),
            ("--image", &self.image),
            ("--monitor", &self.monitor),
        ] {
            if let Some(value) = value {
                arguments.extend([flag.to_owned(), value.clone()]);
            }
        }
        match &self.color {
            Some(ColorRequest::Hex(value)) => {
                arguments.extend(["--color".to_owned(), value.clone()]);
            }
            Some(ColorRequest::Clear) => {
                arguments.extend(["--color".to_owned(), "clear".to_owned()])
            }
            Some(ColorRequest::Pick) => arguments.push("--color".to_owned()),
            None => {}
        }
        if self.keeps_wallpaper {
            arguments.push("--noswitch".to_owned());
        }
        arguments
    }
}

#[derive(Default)]
struct RequestQueue {
    is_running: bool,
    pending_requests: VecDeque<Request>,
}

impl RequestQueue {
    fn submit(&mut self, request: Request) -> Option<Request> {
        if self.is_running {
            if request.image.is_some() && request.monitor.is_none() {
                self.pending_requests
                    .retain(|pending| pending.monitor.is_none() || pending.image.is_none());
            }
            match self
                .pending_requests
                .iter_mut()
                .find(|pending| pending.monitor == request.monitor)
            {
                Some(pending) => *pending = std::mem::take(pending).merge(request),
                None => self.pending_requests.push_back(request),
            }
            return None;
        }
        self.is_running = true;
        Some(request)
    }

    fn finish(&mut self) -> Option<Request> {
        let next = self.pending_requests.pop_front();
        self.is_running = next.is_some();
        next
    }
}

pub fn request(arguments: &[&str]) {
    let request = Request::parse(arguments);
    let Some(request) = QUEUE.with_borrow_mut(|queue| queue.submit(request)) else {
        return;
    };
    glib::spawn_future_local(async move {
        let mut next = Some(request);
        while let Some(request) = next {
            process::finish(build_subcommand(&request)).await;
            next = QUEUE.with_borrow_mut(RequestQueue::finish);
        }
    });
}

fn build_subcommand(request: &Request) -> std::process::Command {
    let mut line = vec![process::executable(), "switchwall".to_owned()];
    line.extend(request.to_arguments());
    process::own_scope(None, &line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_request(arguments: &[&str]) -> Request {
        Request::parse(arguments)
    }

    fn recolor(mode: &str) -> Request {
        parse_request(&["--mode", mode, "--noswitch"])
    }

    #[test]
    fn a_wallpaper_pick_survives_a_later_mode_toggle() {
        let pending = parse_request(&["--mode", "light", "--image", "/b.png"]);
        let merged = pending.merge(recolor("dark"));
        assert_eq!(
            merged,
            parse_request(&["--mode", "dark", "--image", "/b.png"])
        );
    }

    #[test]
    fn a_pending_image_outlives_a_request_that_only_keeps_the_wallpaper() {
        let merged = parse_request(&["--image", "/b.png"]).merge(parse_request(&["--noswitch"]));
        assert_eq!(merged.image.as_deref(), Some("/b.png"));
        assert!(!merged.keeps_wallpaper);
    }

    #[test]
    fn two_requests_that_keep_the_wallpaper_still_keep_it() {
        let merged =
            recolor("light").merge(parse_request(&["--noswitch", "--type", "scheme-neutral"]));
        assert!(merged.keeps_wallpaper);
        assert_eq!(merged.mode.as_deref(), Some("light"));
        assert_eq!(merged.scheme.as_deref(), Some("scheme-neutral"));
    }

    #[test]
    fn a_newer_image_replaces_an_older_one() {
        let merged =
            parse_request(&["--image", "/a.png"]).merge(parse_request(&["--image", "/b.png"]));
        assert_eq!(merged, parse_request(&["--image", "/b.png"]));
    }

    #[test]
    fn a_newer_mode_replaces_an_older_one() {
        let merged = recolor("light").merge(recolor("dark"));
        assert_eq!(merged, recolor("dark"));
    }

    #[test]
    fn an_accent_color_survives_a_later_mode_toggle() {
        let pending = parse_request(&["--noswitch", "--color", "#112233"]);
        let merged = pending.merge(recolor("dark"));
        assert_eq!(merged.color, Some(ColorRequest::Hex("#112233".to_owned())));
        assert_eq!(merged.mode.as_deref(), Some("dark"));
        assert!(merged.keeps_wallpaper);
    }

    #[test]
    fn a_newer_wallpaper_drops_an_older_accent_color_unless_it_brings_its_own() {
        let pending = parse_request(&["--noswitch", "--color", "#112233"]);
        let dropped = pending.clone().merge(parse_request(&["--image", "/b.png"]));
        assert_eq!(dropped.color, None);
        let kept = pending.merge(parse_request(&["--image", "/b.png", "--color", "clear"]));
        assert_eq!(kept.color, Some(ColorRequest::Clear));
    }

    #[test]
    fn a_pending_image_and_a_newer_accent_color_are_both_kept() {
        let merged =
            parse_request(&["--image", "/b.png"]).merge(parse_request(&["--noswitch", "--color"]));
        assert_eq!(merged.image.as_deref(), Some("/b.png"));
        assert_eq!(merged.color, Some(ColorRequest::Pick));
        assert!(!merged.keeps_wallpaper);
    }

    #[test]
    fn a_request_that_opens_the_picker_is_merged_with_nothing() {
        let picker = parse_request(&["--mode", "light"]);
        assert!(picker.is_prompt());
        assert_eq!(recolor("dark").merge(picker.clone()), picker);
        assert_eq!(picker.merge(recolor("dark")), recolor("dark"));
    }

    #[test]
    fn a_request_survives_being_written_as_arguments() {
        for arguments in [
            vec![
                "--mode",
                "dark",
                "--type",
                "scheme-fidelity",
                "--image",
                "/a.png",
            ],
            vec!["--mode", "light", "--noswitch", "--color", "#abcdef"],
            vec!["--noswitch", "--color", "clear"],
            vec!["--noswitch", "--color"],
            vec!["--image", "/a.png", "--monitor", "DP-2"],
            vec![],
        ] {
            let request = parse_request(&arguments);
            assert_eq!(
                parse_request(
                    &request
                        .to_arguments()
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                ),
                request
            );
        }
    }

    #[test]
    fn a_request_with_nothing_running_starts_at_once() {
        let mut queue = RequestQueue::default();
        assert_eq!(queue.submit(recolor("a")), Some(recolor("a")));
        assert!(queue.is_running);
        assert!(queue.pending_requests.is_empty());
    }

    #[test]
    fn a_request_while_running_becomes_pending() {
        let mut queue = RequestQueue::default();
        queue.submit(recolor("a"));
        assert_eq!(queue.submit(recolor("b")), None);
        assert_eq!(queue.pending_requests, [recolor("b")]);
    }

    #[test]
    fn requests_for_different_monitors_wait_apart_and_run_in_order() {
        let mut queue = RequestQueue::default();
        queue.submit(recolor("a"));
        let first = parse_request(&["--image", "/a.png", "--monitor", "DP-1"]);
        let second = parse_request(&["--image", "/b.png", "--monitor", "DP-2"]);
        queue.submit(first.clone());
        queue.submit(second.clone());
        assert_eq!(queue.finish(), Some(first));
        assert_eq!(queue.finish(), Some(second));
        assert_eq!(queue.finish(), None);
        assert!(!queue.is_running);
    }

    #[test]
    fn a_newer_image_for_a_monitor_replaces_the_pending_one_for_that_monitor_only() {
        let mut queue = RequestQueue::default();
        queue.submit(recolor("a"));
        queue.submit(parse_request(&["--image", "/a.png", "--monitor", "DP-1"]));
        queue.submit(parse_request(&["--image", "/b.png", "--monitor", "DP-2"]));
        queue.submit(parse_request(&["--image", "/c.png", "--monitor", "DP-1"]));
        assert_eq!(
            queue.pending_requests,
            [
                parse_request(&["--image", "/c.png", "--monitor", "DP-1"]),
                parse_request(&["--image", "/b.png", "--monitor", "DP-2"]),
            ]
        );
    }

    #[test]
    fn a_newer_image_for_all_monitors_drops_the_older_images_for_single_monitors() {
        let mut queue = RequestQueue::default();
        queue.submit(recolor("a"));
        queue.submit(parse_request(&["--image", "/a.png"]));
        queue.submit(parse_request(&["--image", "/b.png", "--monitor", "DP-1"]));
        queue.submit(parse_request(&["--image", "/c.png"]));
        assert_eq!(
            queue.pending_requests,
            [parse_request(&["--image", "/c.png"])]
        );
    }

    #[test]
    fn a_request_that_keeps_the_wallpaper_has_no_monitor() {
        let request = parse_request(&["--noswitch", "--monitor", "DP-1"]);
        assert_eq!(request.monitor, None);
    }

    #[test]
    fn a_newer_request_replaces_the_pending_one() {
        let mut queue = RequestQueue::default();
        queue.submit(recolor("a"));
        queue.submit(recolor("b"));
        assert_eq!(queue.submit(recolor("c")), None);
        assert_eq!(queue.finish(), Some(recolor("c")));
        assert!(queue.is_running);
        assert_eq!(queue.finish(), None);
    }

    #[test]
    fn finishing_without_a_pending_request_goes_idle() {
        let mut queue = RequestQueue::default();
        queue.submit(recolor("a"));
        assert_eq!(queue.finish(), None);
        assert!(!queue.is_running);
        assert_eq!(queue.submit(recolor("b")), Some(recolor("b")));
    }
}
