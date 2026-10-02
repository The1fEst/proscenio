use gtk4::gio;
use gtk4::glib::{Variant, VariantTy, variant::ObjectPath};
use gtk4::prelude::*;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::core::i18n::tr;
use crate::platform::nmprofile::{Profile, SECRET_SETTINGS, Settings};
use crate::services::net::{BUS, nmcli, split_escaped};
use crate::services::wifi::remote_message;

const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const CALL_TIMEOUT: i32 = 120_000;
const TO_DISK: u32 = 0x1;

pub struct Loaded {
    pub profile: Profile,
    pub secrets: bool,
}

async fn call(
    path: &str,
    interface: &str,
    method: &str,
    arguments: Option<Variant>,
    reply: &str,
    interactive: bool,
) -> Result<Variant, String> {
    let system = gio::bus_get_future(gio::BusType::System)
        .await
        .map_err(|error| error.message().to_owned())?;
    let flags = if interactive {
        gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION
    } else {
        gio::DBusCallFlags::NONE
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
            CALL_TIMEOUT,
        )
        .await
        .map_err(|error| remote_message(&error))
}

async fn path_of(uuid: &str) -> Result<String, String> {
    let reply = call(
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
        .ok_or_else(|| tr("NetworkManager does not know this connection"))
}

async fn secrets(path: &str, profile: &Profile, interactive: bool) -> Vec<Settings> {
    let mut found = Vec::new();
    for setting in SECRET_SETTINGS {
        if !profile.settings.contains_key(setting) {
            continue;
        }
        let reply = call(
            path,
            CONNECTION,
            "GetSecrets",
            Some((setting,).to_variant()),
            "(a{sa{sv}})",
            interactive,
        )
        .await;
        if let Some(secrets) = reply.ok().and_then(|reply| reply.child_value(0).get()) {
            found.push(secrets);
        }
    }
    found
}

pub async fn load(uuid: &str) -> Result<Loaded, String> {
    let path = path_of(uuid).await?;
    let reply = call(&path, CONNECTION, "GetSettings", None, "(a{sa{sv}})", false).await?;
    let mut profile = Profile::from_variant(&reply.child_value(0))
        .ok_or_else(|| tr("NetworkManager sent settings that could not be read"))?;
    let wanted = SECRET_SETTINGS
        .iter()
        .filter(|setting| profile.settings.contains_key(**setting))
        .count();
    let found = secrets(&path, &profile, false).await;
    for secrets in &found {
        profile.merge_secrets(secrets, true);
    }
    Ok(Loaded {
        profile,
        secrets: found.len() == wanted,
    })
}

pub async fn fetch_secrets(uuid: &str) -> Result<Vec<Settings>, String> {
    let path = path_of(uuid).await?;
    let reply = call(&path, CONNECTION, "GetSettings", None, "(a{sa{sv}})", false).await?;
    let profile = Profile::from_variant(&reply.child_value(0))
        .ok_or_else(|| tr("NetworkManager sent settings that could not be read"))?;
    Ok(secrets(&path, &profile, true).await)
}

pub async fn save(profile: &Profile, fill_secrets: bool) -> Result<(), String> {
    let path = path_of(&profile.uuid()).await?;
    let mut sending = profile.clone();
    if fill_secrets {
        for secrets in secrets(&path, profile, true).await {
            sending.merge_secrets(&secrets, false);
        }
    }
    let arguments = Variant::tuple_from_iter([
        sending.to_variant(),
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
    Ok(())
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
    let path = path_of(uuid).await?;
    call(&path, CONNECTION, "Delete", None, "()", true).await?;
    Ok(())
}

pub async fn active(uuid: &str) -> bool {
    nmcli(&["-t", "-f", "UUID", "connection", "show", "--active"])
        .await
        .output
        .lines()
        .any(|line| line == uuid)
}

pub async fn reactivate(uuid: &str) -> Result<(), String> {
    let finished = nmcli(&["connection", "up", "uuid", uuid]).await;
    if finished.success {
        return Ok(());
    }
    Err(finished
        .errors
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_start_matches("Error: ").to_owned())
        .unwrap_or_else(|| tr("Could not connect")))
}

pub async fn devices(kind: &str) -> Vec<String> {
    nmcli(&["-t", "-f", "DEVICE,TYPE", "device"])
        .await
        .output
        .lines()
        .filter_map(|line| {
            let fields = split_escaped(line);
            (fields.get(1).map(String::as_str) == Some(kind)).then(|| fields[0].clone())
        })
        .collect()
}

pub async fn names_and_interfaces() -> (Vec<String>, Vec<String>) {
    let listed = nmcli(&["-t", "-f", "NAME,UUID", "connection", "show"]).await;
    let mut names = Vec::new();
    let mut arguments = vec!["-g", "connection.interface-name", "connection", "show"];
    let rows: Vec<Vec<String>> = listed.output.lines().map(split_escaped).collect();
    for fields in &rows {
        if let [name, uuid, ..] = fields.as_slice() {
            names.push(name.clone());
            arguments.extend(["uuid", uuid.as_str()]);
        }
    }
    let interfaces = nmcli(&arguments)
        .await
        .output
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    (names, interfaces)
}

pub async fn import(kind: &str, file: &str) -> Result<String, String> {
    let finished = nmcli(&["connection", "import", "type", kind, "file", file]).await;
    if !finished.success {
        return Err(finished
            .errors
            .trim()
            .trim_start_matches("Error: ")
            .to_owned());
    }
    imported_uuid(&finished.output).ok_or_else(|| tr("NetworkManager did not name the connection"))
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
}
