use gtk4::gio;
use gtk4::glib::{self, Variant, variant::ObjectPath};
use gtk4::prelude::*;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::core::i18n::tr;
use crate::core::process;
use crate::platform::dbus;
use crate::platform::network::nmprofile::{Profile, SECRET_SETTINGS, Settings};
use crate::services::network::networkmanager::NetworkManager;
use crate::services::network::networkmanager::calls::{self, NO_OBJECT, call, call_raw};
use crate::services::network::networkmanager::model::{ACTIVE_CONNECTION, BUS, DeviceKind, ROOT};
use crate::services::network::wifi::remote_message;

const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const TO_DISK: u32 = 0x1;
const UNKNOWN_CONNECTION_ERROR: &str = "org.freedesktop.NetworkManager.Settings.InvalidConnection";
const NOTHING_STORED_ERRORS: [&str; 2] = [
    "org.freedesktop.NetworkManager.AgentManager.NoSecrets",
    "org.freedesktop.NetworkManager.Settings.Connection.SettingNotFound",
];

pub struct SavedConnection {
    pub profile: Profile,
    pub are_secrets_read: bool,
}

impl SavedConnection {
    fn new(mut profile: Profile, secret_settings: &[Settings], failures: &[glib::Error]) -> Self {
        let are_secrets_read = find_read_failure(failures).is_none();
        if are_secrets_read {
            for settings in secret_settings {
                profile.merge_secrets(settings, true);
            }
        }
        SavedConnection {
            profile,
            are_secrets_read,
        }
    }
}

fn reports_nothing_stored(error: &glib::Error) -> bool {
    gio::DBusError::remote_error(error)
        .is_some_and(|name| NOTHING_STORED_ERRORS.contains(&name.as_str()))
}

fn find_read_failure(failures: &[glib::Error]) -> Option<&glib::Error> {
    failures.iter().find(|error| !reports_nothing_stored(error))
}

fn reports_unknown_connection(error: &glib::Error) -> bool {
    gio::DBusError::remote_error(error).as_deref() == Some(UNKNOWN_CONNECTION_ERROR)
}

async fn find_path(uuid: &str) -> Result<String, glib::Error> {
    let reply = call_raw(
        SETTINGS_PATH,
        SETTINGS,
        "GetConnectionByUuid",
        Some((uuid,).to_variant()),
        "(o)",
        false,
    )
    .await?;
    reply
        .child_value(0)
        .get::<ObjectPath>()
        .map(|path| path.as_str().to_owned())
        .ok_or_else(|| {
            glib::Error::new(
                gio::IOErrorEnum::InvalidData,
                &tr("NetworkManager does not know this connection"),
            )
        })
}

async fn path_of(uuid: &str) -> Result<String, String> {
    find_path(uuid)
        .await
        .map_err(|error| remote_message(&error))
}

async fn read_setting_secrets(
    path: &str,
    profile: &Profile,
    is_interactive: bool,
) -> (Vec<Settings>, Vec<glib::Error>) {
    let mut secret_settings = Vec::new();
    let mut failures = Vec::new();
    for setting in SECRET_SETTINGS {
        if !profile.settings.contains_key(setting) {
            continue;
        }
        let reply = call_raw(
            path,
            CONNECTION,
            "GetSecrets",
            Some((setting,).to_variant()),
            "(a{sa{sv}})",
            is_interactive,
        )
        .await
        .and_then(|reply| {
            reply.child_value(0).get().ok_or_else(|| {
                glib::Error::new(
                    gio::IOErrorEnum::InvalidData,
                    &tr("NetworkManager sent settings that could not be read"),
                )
            })
        });
        match reply {
            Ok(settings) => secret_settings.push(settings),
            Err(error) => {
                let is_fatal = !reports_nothing_stored(&error);
                failures.push(error);
                if is_fatal {
                    break;
                }
            }
        }
    }
    (secret_settings, failures)
}

pub async fn load(uuid: &str) -> Result<SavedConnection, String> {
    let path = path_of(uuid).await?;
    let reply = call(&path, CONNECTION, "GetSettings", None, "(a{sa{sv}})", false).await?;
    let profile = Profile::from_variant(&reply.child_value(0))
        .ok_or_else(|| tr("NetworkManager sent settings that could not be read"))?;
    let (secret_settings, failures) = read_setting_secrets(&path, &profile, false).await;
    Ok(SavedConnection::new(profile, &secret_settings, &failures))
}

pub async fn fetch_secrets(uuid: &str) -> Result<Vec<Settings>, String> {
    let path = path_of(uuid).await?;
    let reply = call(&path, CONNECTION, "GetSettings", None, "(a{sa{sv}})", false).await?;
    let profile = Profile::from_variant(&reply.child_value(0))
        .ok_or_else(|| tr("NetworkManager sent settings that could not be read"))?;
    let (secret_settings, failures) = read_setting_secrets(&path, &profile, true).await;
    match find_read_failure(&failures) {
        Some(error) => Err(remote_message(error)),
        None => Ok(secret_settings),
    }
}

pub async fn save(profile: &Profile, should_fill_secrets: bool) -> Result<Option<Profile>, String> {
    let path = path_of(&profile.uuid()).await?;
    let mut profile_update = profile.clone();
    if should_fill_secrets {
        let (secret_settings, failures) = read_setting_secrets(&path, profile, true).await;
        if let Some(error) = find_read_failure(&failures) {
            return Err(remote_message(error));
        }
        for settings in secret_settings {
            profile_update.merge_secrets(&settings, false);
        }
    }
    let arguments = Variant::tuple_from_iter([
        profile_update.to_variant(),
        TO_DISK.to_variant(),
        HashMap::<String, Variant>::new().to_variant(),
    ]);
    call(
        &path,
        CONNECTION,
        "Update2",
        Some(arguments),
        "(a{sv})",
        true,
    )
    .await?;
    Ok(should_fill_secrets.then_some(profile_update))
}

pub async fn add(profile: &Profile) -> Result<(), String> {
    let arguments = Variant::tuple_from_iter([
        profile.to_variant(),
        TO_DISK.to_variant(),
        HashMap::<String, Variant>::new().to_variant(),
    ]);
    call(
        SETTINGS_PATH,
        SETTINGS,
        "AddConnection2",
        Some(arguments),
        "(oa{sv})",
        true,
    )
    .await?;
    Ok(())
}

pub async fn delete(uuid: &str) -> Result<(), String> {
    let path = match find_path(uuid).await {
        Ok(path) => path,
        Err(error) if reports_unknown_connection(&error) => return Ok(()),
        Err(error) => return Err(remote_message(&error)),
    };
    call(&path, CONNECTION, "Delete", None, "()", true).await?;
    Ok(())
}

pub async fn is_active(uuid: &str) -> bool {
    let Ok(system) = gio::bus_get_future(gio::BusType::System).await else {
        return false;
    };
    let paths: Vec<ObjectPath> = dbus::property(&system, BUS, ROOT, BUS, "ActiveConnections")
        .await
        .and_then(|paths| paths.get())
        .unwrap_or_default();
    for path in paths {
        let active_uuid =
            dbus::string_property(&system, BUS, path.as_str(), ACTIVE_CONNECTION, "Uuid").await;
        if active_uuid.as_deref() == Some(uuid) {
            return true;
        }
    }
    false
}

pub async fn reactivate(uuid: &str) -> Result<(), String> {
    let path = path_of(uuid).await?;
    let active_connection = calls::activate_connection(&path, NO_OBJECT, NO_OBJECT).await?;
    if calls::wait_for_activation(&active_connection).await {
        return Ok(());
    }
    Err(tr("Could not connect"))
}

pub async fn set_autoconnect(uuid: &str, should_autoconnect: bool) -> Result<(), String> {
    let path = path_of(uuid).await?;
    let reply = call(&path, CONNECTION, "GetSettings", None, "(a{sa{sv}})", false).await?;
    let mut profile = Profile::from_variant(&reply.child_value(0))
        .ok_or_else(|| tr("NetworkManager sent settings that could not be read"))?;
    if profile.is_autoconnect_enabled() == should_autoconnect {
        return Ok(());
    }
    profile.set_autoconnect_enabled(should_autoconnect);
    save(&profile, true).await?;
    Ok(())
}

pub fn list_ethernet_devices() -> Vec<String> {
    NetworkManager::instance()
        .snapshot()
        .devices
        .iter()
        .filter(|device| device.kind == DeviceKind::Ethernet)
        .map(|device| device.interface.clone())
        .collect()
}

async fn list_profiles() -> Result<Vec<Profile>, String> {
    let reply = call(
        SETTINGS_PATH,
        SETTINGS,
        "ListConnections",
        None,
        "(ao)",
        false,
    )
    .await?;
    let paths: Vec<ObjectPath> = reply.child_value(0).get().unwrap_or_default();
    let mut profiles = Vec::new();
    for path in paths {
        let Ok(reply) = call_raw(
            path.as_str(),
            CONNECTION,
            "GetSettings",
            None,
            "(a{sa{sv}})",
            false,
        )
        .await
        else {
            continue;
        };
        profiles.extend(Profile::from_variant(&reply.child_value(0)));
    }
    Ok(profiles)
}

pub async fn ports(controller: &[String]) -> Result<Vec<(String, String)>, String> {
    Ok(find_ports(&list_profiles().await?, controller))
}

fn find_ports(profiles: &[Profile], controller: &[String]) -> Vec<(String, String)> {
    profiles
        .iter()
        .filter(|profile| {
            let profile_controller = profile.controller();
            profile.is_port()
                && !profile_controller.is_empty()
                && controller.contains(&profile_controller)
        })
        .map(|profile| (profile.uuid(), profile.interface()))
        .collect()
}

pub async fn find_uuids(id: &str) -> Vec<String> {
    list_profiles()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|profile| profile.id() == id)
        .map(Profile::uuid)
        .collect()
}

pub async fn names_and_interfaces() -> (Vec<String>, Vec<String>) {
    let profiles = list_profiles().await.unwrap_or_default();
    let names = profiles.iter().map(Profile::id).collect();
    let interfaces = profiles
        .iter()
        .map(Profile::interface)
        .filter(|interface| !interface.is_empty())
        .collect();
    (names, interfaces)
}

struct NmcliOutput {
    is_success: bool,
    stdout: String,
    stderr: String,
}

async fn run_nmcli(arguments: &[&str]) -> NmcliOutput {
    let mut command = process::command(&["nmcli"]);
    command.args(arguments).env("LANG", "C").env("LC_ALL", "C");
    let Some(output) = process::capture(command).await else {
        return NmcliOutput {
            is_success: false,
            stdout: String::new(),
            stderr: String::new(),
        };
    };
    NmcliOutput {
        is_success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

pub async fn import(kind: &str, file: &str) -> Result<String, String> {
    let output = run_nmcli(&["connection", "import", "type", kind, "file", file]).await;
    if !output.is_success {
        return Err(output
            .stderr
            .trim()
            .trim_start_matches("Error: ")
            .to_owned());
    }
    imported_uuid(&output.stdout).ok_or_else(|| tr("NetworkManager did not name the connection"))
}

fn imported_uuid(output: &str) -> Option<String> {
    let start = output.rfind('(')? + 1;
    let end = start + output[start..].find(')')?;
    Some(output[start..end].to_owned())
}

pub async fn generate_key() -> Option<String> {
    let mut command = Command::new("wg");
    command.arg("genkey").stdin(Stdio::null());
    process::capture_text(command)
        .await
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
}

pub async fn public_key(private_key: &str) -> Option<String> {
    let key = private_key.trim().to_owned();
    gio::spawn_blocking(move || {
        let mut child = Command::new("wg")
            .arg("pubkey")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        child
            .stdin
            .take()?
            .write_all(format!("{key}\n").as_bytes())
            .ok()?;
        let output = child.wait_with_output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_import_names_the_new_connection_by_its_uuid() {
        assert_eq!(
            imported_uuid(
                "Connection 'wg0 (home)' (0b5a3d7e-2c8d-4b5e-9a39-1c4a6f3e2b10) successfully added.\n"
            ),
            Some("0b5a3d7e-2c8d-4b5e-9a39-1c4a6f3e2b10".to_owned())
        );
    }

    #[test]
    fn only_no_secrets_and_a_missing_setting_count_as_nothing_stored() {
        let remote = |name| gio::DBusError::new_for_dbus_error(name, "message");
        assert!(reports_nothing_stored(&remote(
            "org.freedesktop.NetworkManager.AgentManager.NoSecrets"
        )));
        assert!(reports_nothing_stored(&remote(
            "org.freedesktop.NetworkManager.Settings.Connection.SettingNotFound"
        )));
        for name in [
            "org.freedesktop.NetworkManager.Settings.PermissionDenied",
            "org.freedesktop.NetworkManager.AgentManager.UserCanceled",
            "org.freedesktop.DBus.Error.NoReply",
        ] {
            assert!(!reports_nothing_stored(&remote(name)));
        }
        assert!(!reports_nothing_stored(&glib::Error::new(
            gio::IOErrorEnum::TimedOut,
            "Timeout was reached"
        )));
    }

    #[test]
    fn ports_are_the_port_profiles_of_the_controller() {
        use crate::platform::network::nmprofile::{BOND, BRIDGE, WIRED};
        let new_port = |uuid: &str, controller: &str, kind: &str, device: &str| {
            let mut port = Profile::new_port(controller, kind, device);
            let mut connection = port.settings["connection"].clone();
            connection.insert("uuid".to_owned(), uuid.to_variant());
            port.settings.insert("connection".to_owned(), connection);
            port
        };
        let profiles = [
            new_port("uuid-a", "br0", BRIDGE, "eth0:1"),
            new_port("uuid-e", "", BRIDGE, "eth4"),
            new_port("uuid-d", "bond0", BOND, "eth3"),
            new_port("uuid-b", "uuid-bridge", BRIDGE, ""),
            Profile::new(WIRED, "br0"),
        ];
        let controller = ["br0".to_owned(), "uuid-bridge".to_owned(), String::new()];
        assert_eq!(
            find_ports(&profiles, &controller),
            [
                ("uuid-a".to_owned(), "eth0:1".to_owned()),
                ("uuid-b".to_owned(), String::new()),
            ]
        );
    }

    #[test]
    fn only_an_unknown_uuid_counts_as_already_deleted() {
        let new_remote_error = |name| gio::DBusError::new_for_dbus_error(name, "message");
        assert!(reports_unknown_connection(&new_remote_error(
            "org.freedesktop.NetworkManager.Settings.InvalidConnection"
        )));
        assert!(!reports_unknown_connection(&new_remote_error(
            "org.freedesktop.NetworkManager.Settings.PermissionDenied"
        )));
        assert!(!reports_unknown_connection(&glib::Error::new(
            gio::IOErrorEnum::InvalidData,
            "message"
        )));
    }

    #[test]
    fn a_setting_with_nothing_stored_counts_as_read() {
        let new_remote_error = |name| gio::DBusError::new_for_dbus_error(name, "message");
        let no_secrets = new_remote_error("org.freedesktop.NetworkManager.AgentManager.NoSecrets");
        let setting_not_found =
            new_remote_error("org.freedesktop.NetworkManager.Settings.Connection.SettingNotFound");
        let permission_denied =
            new_remote_error("org.freedesktop.NetworkManager.Settings.PermissionDenied");
        let find_failure_name = |failures: &[glib::Error]| {
            find_read_failure(failures).and_then(gio::DBusError::remote_error)
        };
        assert_eq!(find_failure_name(&[]), None);
        assert_eq!(
            find_failure_name(&[no_secrets.clone(), setting_not_found]),
            None
        );
        for failures in [
            [permission_denied.clone(), no_secrets.clone()],
            [no_secrets, permission_denied],
        ] {
            assert_eq!(
                find_failure_name(&failures).as_deref(),
                Some("org.freedesktop.NetworkManager.Settings.PermissionDenied")
            );
        }
    }

    #[test]
    fn a_partly_read_profile_shows_none_of_its_secrets() {
        use crate::platform::network::nmprofile::wireless::Security;
        use crate::platform::network::nmprofile::{EAP, WIRELESS};
        let new_remote_error = |name| gio::DBusError::new_for_dbus_error(name, "message");
        let mut profile = Profile::new(WIRELESS, "Office");
        profile.set_security(Security::Enterprise);
        let secret_settings = [Settings::from([(
            EAP.to_owned(),
            HashMap::from([("password".to_owned(), "stored".to_variant())]),
        )])];
        for (failure_name, are_secrets_read, password) in [
            (
                "org.freedesktop.NetworkManager.AgentManager.NoSecrets",
                true,
                "stored",
            ),
            (
                "org.freedesktop.NetworkManager.Settings.PermissionDenied",
                false,
                "",
            ),
        ] {
            let saved_connection = SavedConnection::new(
                profile.clone(),
                &secret_settings,
                &[new_remote_error(failure_name)],
            );
            assert_eq!(saved_connection.are_secrets_read, are_secrets_read);
            assert_eq!(saved_connection.profile.eap().password, password);
        }
    }
}
