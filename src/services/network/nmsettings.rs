use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy, variant::ObjectPath};
use gtk4::prelude::*;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::core::i18n::tr;
use crate::platform::dbus::NO_TIMEOUT;
use crate::platform::network::nmprofile::{Profile, SECRET_SETTINGS, Settings};
use crate::services::network::net::{BUS, NmcliOutput, nmcli, split_escaped};
use crate::services::network::wifi::remote_message;

const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const CALL_TIMEOUT_MILLIS: i32 = 120_000;
const TO_DISK: u32 = 0x1;
const NMCLI_NOT_FOUND_EXIT_CODE: i32 = 10;
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

async fn call(
    path: &str,
    interface: &str,
    method: &str,
    arguments: Option<Variant>,
    reply: &str,
    is_interactive: bool,
) -> Result<Variant, String> {
    call_raw(path, interface, method, arguments, reply, is_interactive)
        .await
        .map_err(|error| remote_message(&error))
}

async fn call_raw(
    path: &str,
    interface: &str,
    method: &str,
    arguments: Option<Variant>,
    reply: &str,
    is_interactive: bool,
) -> Result<Variant, glib::Error> {
    let system = gio::bus_get_future(gio::BusType::System).await?;
    let (flags, timeout) = if is_interactive {
        (
            gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION,
            NO_TIMEOUT,
        )
    } else {
        (gio::DBusCallFlags::NONE, CALL_TIMEOUT_MILLIS)
    };
    system
        .call_future(
            Some(BUS),
            path,
            interface,
            method,
            arguments.as_ref(),
            VariantTy::new(reply).ok(),
            flags,
            timeout,
        )
        .await
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
    nmcli(&["-t", "-f", "UUID", "connection", "show", "--active"])
        .await
        .stdout
        .lines()
        .any(|line| line == uuid)
}

pub async fn reactivate(uuid: &str) -> Result<(), String> {
    let output = nmcli(&["connection", "up", "uuid", uuid]).await;
    if output.is_success {
        return Ok(());
    }
    Err(output
        .stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_start_matches("Error: ").to_owned())
        .unwrap_or_else(|| tr("Could not connect")))
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

pub async fn devices(kind: &str) -> Vec<String> {
    nmcli(&["-t", "-f", "DEVICE,TYPE", "device"])
        .await
        .stdout
        .lines()
        .filter_map(|line| {
            let fields = split_escaped(line);
            (fields.get(1).map(String::as_str) == Some(kind)).then(|| fields[0].clone())
        })
        .collect()
}

pub async fn ports(controller: &[String]) -> Result<Vec<(String, String)>, String> {
    let listing = nmcli(&["-t", "-f", "UUID,PORT", "connection", "show"])
        .await
        .into_stdout()?;
    let port_uuids: Vec<String> = listing
        .lines()
        .map(split_escaped)
        .filter_map(|fields| match fields.as_slice() {
            [uuid, port, ..] if !port.is_empty() && port != "--" => Some(uuid.clone()),
            _ => None,
        })
        .collect();
    if port_uuids.is_empty() {
        return Ok(Vec::new());
    }
    let mut arguments = vec![
        "-g",
        "connection.uuid,connection.master,connection.interface-name",
        "connection",
        "show",
    ];
    for uuid in &port_uuids {
        arguments.extend(["uuid", uuid.as_str()]);
    }
    let details = read_port_details(nmcli(&arguments).await)?;
    Ok(parse_port_details(&details, &port_uuids, controller))
}

fn read_port_details(output: NmcliOutput) -> Result<String, String> {
    if output.exit_code == Some(NMCLI_NOT_FOUND_EXIT_CODE) {
        return Ok(output.stdout);
    }
    output.into_stdout()
}

fn parse_port_details(
    details: &str,
    port_uuids: &[String],
    controller: &[String],
) -> Vec<(String, String)> {
    let unescape = |line: &str| split_escaped(line).concat();
    let mut lines = details.lines();
    let mut port_connections = Vec::new();
    while let Some(uuid) = lines.next() {
        if !port_uuids.iter().any(|known| known == uuid) {
            continue;
        }
        let (Some(master), Some(device)) = (lines.next(), lines.next()) else {
            break;
        };
        let master = unescape(master);
        if !master.is_empty() && controller.contains(&master) {
            port_connections.push((uuid.to_owned(), unescape(device)));
        }
    }
    port_connections
}

pub async fn names_and_interfaces() -> (Vec<String>, Vec<String>) {
    let output = nmcli(&["-t", "-f", "NAME,UUID", "connection", "show"]).await;
    let mut names = Vec::new();
    let mut arguments = vec!["-g", "connection.interface-name", "connection", "show"];
    let rows: Vec<Vec<String>> = output.stdout.lines().map(split_escaped).collect();
    for fields in &rows {
        if let [name, uuid, ..] = fields.as_slice() {
            names.push(name.clone());
            arguments.extend(["uuid", uuid.as_str()]);
        }
    }
    let interfaces = nmcli(&arguments)
        .await
        .stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    (names, interfaces)
}

pub async fn import(kind: &str, file: &str) -> Result<String, String> {
    let output = nmcli(&["connection", "import", "type", kind, "file", file]).await;
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
    crate::core::process::capture_text(command)
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
    fn port_details_of_many_connections_are_read_from_one_listing() {
        let port_uuids = [
            "uuid-a",
            "uuid-gone",
            "uuid-b",
            "uuid-c",
            "uuid-d",
            "uuid-e",
        ]
        .map(str::to_owned);
        let controller = ["br0".to_owned(), "uuid-bridge".to_owned(), String::new()];
        let details = concat!(
            "uuid-a\nbr0\neth0\\:1\n\n",
            "uuid-e\n\neth4\n\n",
            "uuid-c\nuuid-b\neth2\n\n",
            "uuid-d\nbond0\neth3\n\n",
            "uuid-b\nuuid-bridge\n\n",
        );
        assert_eq!(
            parse_port_details(details, &port_uuids, &controller),
            [
                ("uuid-a".to_owned(), "eth0:1".to_owned()),
                ("uuid-b".to_owned(), String::new()),
            ]
        );
    }

    #[test]
    fn a_vanished_port_connection_leaves_the_others_listed() {
        let details = "uuid-a\nbr0\neth0\n";
        let new_output = |exit_code, stdout: &str| NmcliOutput {
            is_success: exit_code == 0,
            exit_code: Some(exit_code),
            stdout: stdout.to_owned(),
            stderr: "Error: uuid-gone - no such connection profile.\n".to_owned(),
        };
        assert_eq!(
            read_port_details(new_output(10, details)),
            Ok(details.to_owned())
        );
        assert_eq!(
            read_port_details(new_output(8, "")),
            Err("uuid-gone - no such connection profile.".to_owned())
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
