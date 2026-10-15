use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy, variant::ObjectPath};
use gtk4::prelude::*;

use crate::core::i18n::tr;
use crate::platform::dbus::NO_TIMEOUT;
use crate::platform::security::crypt;
use crate::platform::security::pam;
use crate::services::network::wifi::remote_message;

const BUS: &str = "org.freedesktop.Accounts";
const PATH: &str = "/org/freedesktop/Accounts";
const USER: &str = "org.freedesktop.Accounts.User";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const TIMEOUT_MILLIS: i32 = 2000;
const ADMINISTRATOR: i32 = 1;
const ASK_AT_LOGIN: i32 = 1;
const USER_NAME_LENGTH: usize = 32;
const FACE_SIZE: i32 = 256;
const PASSWORD_FAILURE_MESSAGE: &str = "Could not change the password";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct User {
    pub path: String,
    pub uid: u64,
    pub user_name: String,
    pub real_name: String,
    pub email: String,
    pub icon_file: String,
    pub is_administrator: bool,
    pub has_password: bool,
}

impl User {
    pub fn display_name(&self) -> &str {
        if self.real_name.is_empty() {
            &self.user_name
        } else {
            &self.real_name
        }
    }
}

pub fn read(on_user: impl FnOnce(User) + 'static) {
    glib::spawn_future_local(async move {
        if let Some(user) = fetch().await {
            on_user(user);
        }
    });
}

pub fn set(user: &User, property: &str, value: &str, on_done: impl FnOnce(User) + 'static) {
    let (path, method, value) = (
        user.path.clone(),
        format!("Set{property}"),
        value.to_owned(),
    );
    glib::spawn_future_local(async move {
        if let Ok(system) = gio::bus_get_future(gio::BusType::System).await {
            let _ = system
                .call_future(
                    Some(BUS),
                    &path,
                    USER,
                    &method,
                    Some(&(value,).to_variant()),
                    None,
                    gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION,
                    -1,
                )
                .await;
        }
        if let Some(user) = fetch().await {
            on_done(user);
        }
    });
}

pub fn change_password(
    current: &str,
    next: &str,
    on_done: impl FnOnce(Result<(), String>) + 'static,
) {
    let mut answers = [current, "\n", next, "\n", next, "\n"]
        .concat()
        .into_bytes();
    let launcher = gio::SubprocessLauncher::new(
        gio::SubprocessFlags::STDIN_PIPE
            | gio::SubprocessFlags::STDOUT_PIPE
            | gio::SubprocessFlags::STDERR_MERGE,
    );
    launcher.setenv("LC_ALL", "C", true);
    launcher.setenv("LANG", "C", true);
    let Ok(process) = launcher.spawn(&[std::ffi::OsStr::new("passwd")]) else {
        pam::wipe(&mut answers);
        on_done(Err(tr(PASSWORD_FAILURE_MESSAGE)));
        return;
    };
    glib::spawn_future_local(async move {
        let output = answer(&process, answers).await;
        on_done(password_outcome(&output));
    });
}

async fn answer(process: &gio::Subprocess, mut answers: Vec<u8>) -> String {
    if let Some(input) = process.stdin_pipe() {
        answers = match input
            .write_all_future(answers, glib::Priority::DEFAULT)
            .await
        {
            Ok((answers, _, _)) | Err((answers, _)) => answers,
        };
    }
    pam::wipe(&mut answers);
    process
        .communicate_utf8_future(Some(String::new()))
        .await
        .ok()
        .and_then(|(output, _)| output)
        .map(|output| output.to_string())
        .unwrap_or_default()
}

fn password_outcome(output: &str) -> Result<(), String> {
    if output.contains("updated successfully") {
        return Ok(());
    }
    let complaint = output
        .split("passwd:")
        .skip(1)
        .filter_map(|part| part.lines().next())
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "password unchanged")
        .last();
    Err(complaint.map_or_else(|| tr(PASSWORD_FAILURE_MESSAGE), str::to_owned))
}

async fn fetch() -> Option<User> {
    let system = gio::bus_get_future(gio::BusType::System).await.ok()?;
    let name = glib::user_name().to_string_lossy().into_owned();
    let reply = system
        .call_future(
            Some(BUS),
            PATH,
            BUS,
            "FindUserByName",
            Some(&(name,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT_MILLIS,
        )
        .await
        .ok()?;
    let path = reply.child_value(0).str()?.to_owned();
    user_at(&system, path).await
}

pub async fn others() -> Vec<User> {
    let Ok(system) = gio::bus_get_future(gio::BusType::System).await else {
        return Vec::new();
    };
    let Ok(reply) = system
        .call_future(
            Some(BUS),
            PATH,
            BUS,
            "ListCachedUsers",
            None,
            VariantTy::new("(ao)").ok(),
            gio::DBusCallFlags::NONE,
            TIMEOUT_MILLIS,
        )
        .await
    else {
        return Vec::new();
    };
    let own_name = glib::user_name().to_string_lossy().into_owned();
    let paths: Vec<ObjectPath> = reply.child_value(0).get().unwrap_or_default();
    let mut users = Vec::new();
    for path in paths {
        if let Some(user) = user_at(&system, path.as_str().to_owned()).await
            && user.user_name != own_name
        {
            users.push(user);
        }
    }
    users.sort_by_key(|user| user.display_name().to_lowercase());
    users
}

async fn call(
    path: &str,
    interface: &str,
    method: &str,
    arguments: Variant,
) -> Result<Variant, String> {
    gio::bus_get_future(gio::BusType::System)
        .await
        .map_err(|error| error.message().to_owned())?
        .call_future(
            Some(BUS),
            path,
            interface,
            method,
            Some(&arguments),
            None,
            gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION,
            NO_TIMEOUT,
        )
        .await
        .map_err(|error| remote_message(&error))
}

pub async fn create(
    user_name: &str,
    real_name: &str,
    is_administrator: bool,
) -> Result<String, String> {
    let kind = if is_administrator { ADMINISTRATOR } else { 0 };
    let reply = call(
        PATH,
        BUS,
        "CreateUser",
        (user_name, real_name, kind).to_variant(),
    )
    .await?;
    reply
        .child_value(0)
        .get::<ObjectPath>()
        .map(|path| path.as_str().to_owned())
        .ok_or_else(|| tr("AccountsService did not name the new account"))
}

pub async fn delete(uid: u64, should_remove_files: bool) -> Result<(), String> {
    call(
        PATH,
        BUS,
        "DeleteUser",
        (uid as i64, should_remove_files).to_variant(),
    )
    .await?;
    Ok(())
}

pub async fn set_administrator(path: &str, is_administrator: bool) -> Result<(), String> {
    let kind = if is_administrator { ADMINISTRATOR } else { 0 };
    call(path, USER, "SetAccountType", (kind,).to_variant()).await?;
    Ok(())
}

pub async fn set_icon(path: &str, file: &str) -> Result<(), String> {
    call(path, USER, "SetIconFile", (file,).to_variant()).await?;
    Ok(())
}

pub async fn face_from(file: &str) -> Result<String, String> {
    let file = file.to_owned();
    let target = glib::user_runtime_dir().join("proscenio").join("face.png");
    gio::spawn_blocking(move || {
        let make_unreadable_error = || tr("The picture could not be read");
        let (_, width, height) =
            gtk4::gdk_pixbuf::Pixbuf::file_info(&file).ok_or_else(make_unreadable_error)?;
        let shorter_side = width.min(height).max(1);
        let scale = |side: i32| {
            (side as i64 * FACE_SIZE as i64 / shorter_side as i64).max(FACE_SIZE as i64) as i32
        };
        let picture =
            gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(&file, scale(width), scale(height), false)
                .map_err(|_| make_unreadable_error())?;
        let square = picture.new_subpixbuf(
            (picture.width() - FACE_SIZE) / 2,
            (picture.height() - FACE_SIZE) / 2,
            FACE_SIZE,
            FACE_SIZE,
        );
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        square
            .savev(&target, "png", &[])
            .map_err(|error| error.message().to_owned())?;
        Ok(target.to_string_lossy().into_owned())
    })
    .await
    .unwrap_or_else(|_| Err(tr("The picture could not be read")))
}

pub async fn set_password(path: &str, password: &str) -> Result<(), String> {
    let password_hash = crypt::hash(password).ok_or_else(|| tr(PASSWORD_FAILURE_MESSAGE))?;
    call(path, USER, "SetPassword", (password_hash, "").to_variant()).await?;
    Ok(())
}

pub async fn ask_password_at_login(path: &str) -> Result<(), String> {
    call(path, USER, "SetPasswordMode", (ASK_AT_LOGIN,).to_variant()).await?;
    Ok(())
}

pub fn is_valid_user_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first == '_')
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-')
        })
        && name.len() <= USER_NAME_LENGTH
}

pub fn user_name_from(real_name: &str) -> String {
    let name: String = real_name
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase()
        .chars()
        .filter(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        .collect();
    name.trim_start_matches(|character: char| character.is_ascii_digit())
        .chars()
        .take(USER_NAME_LENGTH)
        .collect()
}

async fn user_at(system: &gio::DBusConnection, path: String) -> Option<User> {
    let reply = system
        .call_future(
            Some(BUS),
            &path,
            PROPERTIES,
            "GetAll",
            Some(&(USER,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT_MILLIS,
        )
        .await
        .ok()?;
    let properties = glib::VariantDict::new(Some(&reply.child_value(0)));
    let string = |key: &str| {
        properties
            .lookup_value(key, None)
            .as_ref()
            .and_then(Variant::str)
            .unwrap_or_default()
            .to_owned()
    };
    let icon_file = Some(string("IconFile"))
        .filter(|icon| std::path::Path::new(icon).is_file())
        .unwrap_or_default();
    let account_type = properties
        .lookup_value("AccountType", None)
        .and_then(|value| value.get::<i32>())
        .unwrap_or(0);
    Some(User {
        path,
        uid: properties
            .lookup_value("Uid", None)
            .and_then(|value| value.get::<u64>())
            .unwrap_or_default(),
        user_name: string("UserName"),
        real_name: string("RealName"),
        email: string("Email"),
        icon_file,
        is_administrator: account_type == ADMINISTRATOR,
        has_password: properties
            .lookup_value("PasswordMode", None)
            .and_then(|value| value.get::<i32>())
            .unwrap_or(0)
            != ASK_AT_LOGIN,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_names_are_lower_case_and_start_from_the_first_name() {
        assert_eq!(user_name_from("Анна Ivanova"), "");
        assert_eq!(user_name_from("Jo Doe"), "jo");
        assert_eq!(user_name_from("42 Agent"), "");
        assert_eq!(user_name_from("R2D2"), "r2d2");
        assert!(is_valid_user_name("jo"));
        assert!(is_valid_user_name("_build-1"));
        assert!(!is_valid_user_name("Jo"));
        assert!(!is_valid_user_name("1jo"));
        assert!(!is_valid_user_name(""));
        assert!(!is_valid_user_name(&"a".repeat(33)));
    }

    #[test]
    fn password_outcome_keeps_passwds_last_complaint() {
        let cases = [
            (
                "Changing password for fest.\nCurrent password: New password: Retype new password: passwd: password updated successfully\n",
                Ok(()),
            ),
            (
                "Current password: passwd: Authentication token manipulation error\npasswd: password unchanged\n",
                Err("Authentication token manipulation error".to_owned()),
            ),
            (
                "New password: BAD PASSWORD: The password is shorter than 8 characters\npasswd: Have exhausted maximum number of retries for service\npasswd: password unchanged\n",
                Err("Have exhausted maximum number of retries for service".to_owned()),
            ),
            ("", Err(tr(PASSWORD_FAILURE_MESSAGE))),
        ];
        for (output, expected) in cases {
            assert_eq!(password_outcome(output), expected, "{output:?}");
        }
    }

    #[test]
    fn the_answers_reach_the_child_and_end_its_input() {
        let context = glib::MainContext::new();
        let output = context.block_on(async {
            let process = gio::Subprocess::newv(
                &[std::ffi::OsStr::new("cat")],
                gio::SubprocessFlags::STDIN_PIPE | gio::SubprocessFlags::STDOUT_PIPE,
            )
            .unwrap();
            answer(&process, b"old\nnew\nnew\n".to_vec()).await
        });
        assert_eq!(output, "old\nnew\nnew\n");
    }
}
