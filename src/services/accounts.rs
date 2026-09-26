use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;

const BUS: &str = "org.freedesktop.Accounts";
const PATH: &str = "/org/freedesktop/Accounts";
const USER: &str = "org.freedesktop.Accounts.User";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const TIMEOUT: i32 = 2000;
const ADMINISTRATOR: i32 = 1;
const PASSWORD_FAILED: &str = "Could not change the password";

#[derive(Clone, Default)]
pub struct User {
    pub path: String,
    pub user_name: String,
    pub real_name: String,
    pub email: String,
    pub icon_file: String,
    pub administrator: bool,
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

pub fn read(handler: impl FnOnce(User) + 'static) {
    glib::spawn_future_local(async move {
        if let Some(user) = fetch().await {
            handler(user);
        }
    });
}

pub fn set(user: &User, property: &str, value: &str, done: impl FnOnce(User) + 'static) {
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
            done(user);
        }
    });
}

pub fn change_password(current: &str, next: &str, done: impl FnOnce(Result<(), String>) + 'static) {
    let answers = format!("{current}\n{next}\n{next}\n");
    let launcher = gio::SubprocessLauncher::new(
        gio::SubprocessFlags::STDIN_PIPE
            | gio::SubprocessFlags::STDOUT_PIPE
            | gio::SubprocessFlags::STDERR_MERGE,
    );
    launcher.setenv("LC_ALL", "C", true);
    launcher.setenv("LANG", "C", true);
    let Ok(process) = launcher.spawn(&[std::ffi::OsStr::new("passwd")]) else {
        done(Err(PASSWORD_FAILED.to_owned()));
        return;
    };
    glib::spawn_future_local(async move {
        let output = process
            .communicate_utf8_future(Some(answers))
            .await
            .ok()
            .and_then(|(output, _)| output)
            .map(|output| output.to_string())
            .unwrap_or_default();
        done(password_outcome(&output));
    });
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
    Err(complaint.unwrap_or(PASSWORD_FAILED).to_owned())
}

async fn fetch() -> Option<User> {
    let system = gio::bus_get_future(gio::BusType::System).await.ok()?;
    let name = glib::user_name().to_string_lossy().into_owned();
    let found = system
        .call_future(
            Some(BUS),
            PATH,
            BUS,
            "FindUserByName",
            Some(&(name,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT,
        )
        .await
        .ok()?;
    let path = found.child_value(0).str()?.to_owned();
    let reply = system
        .call_future(
            Some(BUS),
            &path,
            PROPERTIES,
            "GetAll",
            Some(&(USER,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT,
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
        user_name: string("UserName"),
        real_name: string("RealName"),
        email: string("Email"),
        icon_file,
        administrator: account_type == ADMINISTRATOR,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            ("", Err(PASSWORD_FAILED.to_owned())),
        ];
        for (output, expected) in cases {
            assert_eq!(password_outcome(output), expected, "{output:?}");
        }
    }
}
