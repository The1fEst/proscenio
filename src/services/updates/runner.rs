use gtk4::glib;
use proscenio_protocol::{Change, Decoder, Event, Message, Request, Stage};
use std::cell::{Cell, RefCell};
use std::io::Write;
use std::process::{ChildStdin, Stdio};
use std::rc::Rc;

use super::manager::{aur_query, aur_upgrade, package_manager};
use super::output::{decode_events, elapsed_text, exit_code, visible_text, watch_output};
use super::packages::{query_aur_packages, version_change};
use super::{BRANCH, HELPER, Job, REPOSITORY, Updates};
use crate::core::i18n::trf;
use crate::core::{paths, process};
use crate::services::updates::updateprogress::UpdateProgress;

const MERGED_OUTPUT: &str = "exec 2>&1; exec \"$@\"";
const SHELL_UPDATE: &str = "exec 2>&1
set -e
mkdir -p \"$1\"
cd \"$1\"
curl -fsSLO \"$2\"
makepkg -Acfsi --noconfirm
systemctl --user restart proscenio";
const SHELL_COMMAND: &str = "makepkg -Acfsi --noconfirm && systemctl --user restart proscenio";

#[derive(Clone)]
enum AurListing {
    Pending,
    Failed,
    Listed(Vec<String>),
}

struct Session {
    stdin: RefCell<Option<ChildStdin>>,
    job: Job,
    manager: &'static str,
    socket_and_token: RefCell<Option<(String, Vec<u8>)>>,
    planned_changes: RefCell<Option<Vec<Change>>>,
    aur_listing: RefCell<AurListing>,
    aur_names: RefCell<Vec<String>>,
    includes_aur: Cell<bool>,
    is_forwarding: Cell<bool>,
    is_ok: Cell<bool>,
    package_index: Cell<u32>,
}

impl Session {
    fn is_open(&self) -> bool {
        self.stdin.borrow().is_some()
    }

    fn send(&self, request: Request) {
        if let Some(stdin) = self.stdin.borrow_mut().as_mut() {
            let _ = stdin.write_all(&request.encode());
        }
    }

    fn close(&self) {
        self.send(Request::Quit);
        self.stdin.borrow_mut().take();
    }
}

impl Updates {
    fn end(&self, status: i32, start_time_micros: i64) {
        let elapsed = elapsed_text(glib::monotonic_time() - start_time_micros);
        let summary = match exit_code(status) {
            Some(0) => trf("Finished in %1", &[&elapsed]),
            Some(code) => trf(
                "Failed with exit code %1 after %2",
                &[&code.to_string(), &elapsed],
            ),
            None => trf("Stopped after %1", &[&elapsed]),
        };
        if status == 0
            && let Some(progress) = self.progress.borrow_mut().as_mut()
        {
            progress.finish();
        }
        self.push_log(&format!("\n{summary}"));
        self.job.set(None);
        self.last_job_succeeded.set(Some(status == 0));
        self.listeners.notify();
        self.refresh();
    }

    fn run_logged(
        &self,
        command_text: &str,
        line: &[String],
        environment: &[(&str, String)],
        on_line: impl Fn(&str) + 'static,
        on_exit: impl FnOnce(i32) + 'static,
    ) -> Option<u32> {
        let mut command = process::own_scope(None, line);
        command.stdout(Stdio::piped());
        for (name, value) in environment {
            command.env(name, value);
        }
        let mut child = command.spawn().ok()?;
        let Some(stdout) = child.stdout.take() else {
            return None;
        };
        self.push_log(&format!("$ {command_text}"));

        let updates = self.clone();
        let on_bytes = process::split_lines(move |line: Option<&str>| {
            let Some(line) = line else {
                return;
            };
            let line = visible_text(line);
            updates.push_log(&line);
            on_line(&line);
        });
        watch_output(stdout, glib::Pid(child.id() as i32), on_bytes, on_exit);
        Some(child.id())
    }

    pub(super) fn start_session(&self, start_time_micros: i64, job: Job) -> bool {
        let Some(manager) = package_manager() else {
            return false;
        };
        let mut command = process::own_scope(None, &["pkexec", HELPER, "session"]);
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        let Ok(mut child) = command.spawn() else {
            return false;
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return false;
        };
        self.push_log(&format!("$ pkexec {HELPER} session"));
        let session = Rc::new(Session {
            stdin: RefCell::new(Some(stdin)),
            job,
            manager,
            socket_and_token: RefCell::new(None),
            planned_changes: RefCell::new(None),
            aur_listing: RefCell::new(AurListing::Pending),
            aur_names: RefCell::new(Vec::new()),
            includes_aur: Cell::new(false),
            is_forwarding: Cell::new(false),
            is_ok: Cell::new(true),
            package_index: Cell::new(0),
        });

        let mut decoder = Decoder::default();
        let updates = self.clone();
        let on_bytes = {
            let session = session.clone();
            move |bytes: Option<&[u8]>| {
                for message in decode_events(&mut decoder, bytes) {
                    match message {
                        Ok(event) => updates.handle_session_event(&session, event),
                        Err(error) => updates.push_log(&format!("protocol error: {error:?}")),
                    }
                }
            }
        };

        let updates = self.clone();
        let pid = glib::Pid(child.id() as i32);
        watch_output(stdout, pid, on_bytes, move |code| {
            let status = if code == 0 && !session.is_ok.get() {
                1 << 8
            } else {
                code
            };
            updates.end(status, start_time_micros);
        });
        true
    }

    fn handle_session_event(&self, session: &Rc<Session>, event: Event) {
        if let Some(progress) = self.progress.borrow_mut().as_mut() {
            progress.handle_event(&event);
        }
        self.output_listeners.notify();
        match event {
            Event::Ready {
                version,
                socket,
                token,
            } => {
                if version != proscenio_protocol::VERSION {
                    self.push_log(&format!(
                        "{HELPER} speaks protocol {version}, the shell {}",
                        proscenio_protocol::VERSION
                    ));
                    session.is_ok.set(false);
                    session.close();
                    return;
                }
                session.socket_and_token.replace(Some((socket, token)));
                match session.job {
                    Job::System => self.plan(session),
                    Job::Shell => self.start_build(session),
                }
            }
            Event::Planned(changes) => {
                session.planned_changes.replace(Some(changes));
                self.continue_plan(session);
            }
            Event::Stage(stage) => self.push_log(stage_text(stage)),
            Event::Transaction {
                package_count,
                download_bytes,
            } => self.push_log(&format!(
                "Packages ({package_count}), {} to download",
                glib::format_size(download_bytes)
            )),
            Event::Package {
                index, count, name, ..
            } => {
                if session.package_index.replace(index) != index {
                    self.push_log(&format!("({index}/{count}) {name}"));
                }
            }
            Event::Hook { index, count, name } => {
                self.push_log(&format!("({index}/{count}) {name}"));
            }
            Event::Line(line) => self.push_log(&visible_text(&line)),
            Event::Done { is_ok, message } => {
                session.package_index.set(0);
                if !is_ok {
                    self.push_log(&message);
                    session.is_ok.set(false);
                }
                if session.is_forwarding.get() {
                    return;
                }
                if is_ok && session.includes_aur.get() {
                    self.start_aur(session);
                } else {
                    session.close();
                }
            }
            Event::Download { .. } => {}
        }
    }

    fn plan(&self, session: &Rc<Session>) {
        session.send(Request::Plan);
        let updates = self.clone();
        let session = session.clone();
        glib::spawn_future_local(async move {
            let listing = match query_aur_packages(session.manager).await {
                Some(aur_packages) => AurListing::Listed(aur_packages),
                None => AurListing::Failed,
            };
            session.aur_listing.replace(listing);
            updates.continue_plan(&session);
        });
    }

    fn continue_plan(&self, session: &Rc<Session>) {
        let planned_changes = session.planned_changes.borrow().clone();
        let aur_listing = session.aur_listing.borrow().clone();
        let Some(changes) = planned_changes else {
            return;
        };
        if !session.is_open() {
            return;
        }
        match aur_listing {
            AurListing::Pending => {}
            AurListing::Failed => {
                self.push_log("Could not list the AUR packages to update");
                session.is_ok.set(false);
                session.close();
            }
            AurListing::Listed(aur_packages) => {
                self.show_plan(&changes, aur_packages.clone());
                self.install_plan(session, changes, &aur_packages);
            }
        }
    }

    fn install_plan(&self, session: &Rc<Session>, changes: Vec<Change>, aur_packages: &[String]) {
        let aur_names: Vec<String> = aur_packages
            .iter()
            .filter_map(|line| version_change(line))
            .map(|(name, _, _)| name.to_owned())
            .collect();
        let has_aur = aur_query(session.manager).is_some();
        self.progress.replace(Some(UpdateProgress::system(
            changes.len(),
            aur_names.len(),
            has_aur,
        )));
        session.includes_aur.set(has_aur && !aur_names.is_empty());
        session.aur_names.replace(aur_names);
        self.output_listeners.notify();
        session.send(Request::Upgrade(changes));
    }

    fn start_aur(&self, session: &Rc<Session>) {
        let Some((socket, token)) = session.socket_and_token.borrow().clone() else {
            session.close();
            return;
        };
        if let Some(progress) = self.progress.borrow_mut().as_mut() {
            progress.start_aur();
        }
        let names = session.aur_names.borrow().clone();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let command = aur_upgrade(session.manager, &names);
        let line: Vec<String> = ["bash", "-c", MERGED_OUTPUT, "bash"]
            .into_iter()
            .chain(command.iter().copied())
            .map(str::to_owned)
            .collect();
        let updates = self.clone();
        self.run_forwarded(
            session,
            (socket, token),
            &command.join(" "),
            &line,
            Vec::new(),
            move |line| {
                if let Some(progress) = updates.progress.borrow_mut().as_mut() {
                    progress.handle_aur_line(line);
                }
            },
        );
    }

    fn start_build(&self, session: &Rc<Session>) {
        let Some(socket_and_token) = session.socket_and_token.borrow().clone() else {
            session.close();
            return;
        };
        let line: Vec<String> = [
            "bash".to_owned(),
            "-c".to_owned(),
            SHELL_UPDATE.to_owned(),
            "bash".to_owned(),
            paths::cache()
                .join("package")
                .to_string_lossy()
                .into_owned(),
            format!("https://raw.githubusercontent.com/{REPOSITORY}/{BRANCH}/packaging/PKGBUILD"),
        ]
        .into();
        self.run_forwarded(
            session,
            socket_and_token,
            SHELL_COMMAND,
            &line,
            vec![("PACMAN_AUTH", HELPER.to_owned())],
            |_| {},
        );
    }

    fn run_forwarded(
        &self,
        session: &Rc<Session>,
        (socket, token): (String, Vec<u8>),
        command_text: &str,
        line: &[String],
        mut environment: Vec<(&str, String)>,
        on_line: impl Fn(&str) + 'static,
    ) {
        session.is_forwarding.set(true);
        environment.push((proscenio_protocol::SOCKET_VARIABLE, socket));
        environment.push((
            proscenio_protocol::TOKEN_VARIABLE,
            proscenio_protocol::hex(&token),
        ));
        let child_pid = self.run_logged(command_text, line, &environment, on_line, {
            let session = session.clone();
            move |status| {
                if status != 0 {
                    session.is_ok.set(false);
                }
                session.close();
            }
        });
        match child_pid {
            Some(pid) => session.send(Request::Allow(pid)),
            None => {
                session.is_ok.set(false);
                session.close();
            }
        }
    }
}

fn stage_text(stage: Stage) -> &'static str {
    match stage {
        Stage::Synchronize => ":: Synchronizing package databases...",
        Stage::Prepare => ":: Resolving dependencies...",
        Stage::Download => ":: Retrieving packages...",
        Stage::Install => ":: Processing package changes...",
        Stage::Hooks => ":: Running post-transaction hooks...",
    }
}
