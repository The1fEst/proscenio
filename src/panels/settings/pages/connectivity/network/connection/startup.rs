use super::{BOND_MODES, NEW, NEW_ENTERPRISE, NEW_OPENVPN};
use crate::core::i18n::tr;
use crate::platform::network::nmprofile::wireless::Security;
use crate::platform::network::nmprofile::{self, BOND, BRIDGE, Profile, VLAN, WIREGUARD, WIRELESS};
use crate::services::network::net::nmcli;
use crate::services::network::nmsettings;

pub(super) struct Start {
    pub(super) profile: Profile,
    pub(super) is_new: bool,
    pub(super) are_secrets_read: bool,
    pub(super) devices: Vec<String>,
    pub(super) ports: Vec<(String, String)>,
}

async fn find_primary() -> Option<String> {
    nmcli(&["-t", "-f", "UUID,TYPE", "connection", "show", "--active"])
        .await
        .stdout
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(_, kind)| *kind != "loopback")
        .map(|(uuid, _)| uuid.to_owned())
}

pub(super) async fn start(argument: Option<String>) -> Result<Start, String> {
    let target = match argument {
        Some(target) => target,
        None => find_primary()
            .await
            .ok_or_else(|| tr("No connection is active, so there is nothing to edit"))?,
    };
    if let Some(ssid) = target.strip_prefix(NEW_ENTERPRISE) {
        let (names, _) = nmsettings::names_and_interfaces().await;
        let mut profile = Profile::new(WIRELESS, &nmprofile::free_name(ssid, &names));
        profile.set_ssid(ssid);
        profile.set_security(Security::Enterprise);
        return Ok(Start {
            devices: Vec::new(),
            profile,
            is_new: true,
            are_secrets_read: true,
            ports: Vec::new(),
        });
    }
    if target == NEW_OPENVPN {
        let (names, _) = nmsettings::names_and_interfaces().await;
        return Ok(Start {
            devices: Vec::new(),
            profile: Profile::new_openvpn(&nmprofile::free_name(&tr("OpenVPN"), &names)),
            is_new: true,
            are_secrets_read: true,
            ports: Vec::new(),
        });
    }
    if let Some(kind) = target.strip_prefix(NEW) {
        let base = match kind {
            WIREGUARD => tr("WireGuard"),
            VLAN => tr("VLAN"),
            BRIDGE => tr("Bridge"),
            BOND => tr("Bond"),
            _ => tr("Wired connection"),
        };
        let (names, interfaces) = nmsettings::names_and_interfaces().await;
        let mut profile = Profile::new(kind, &nmprofile::free_name(&base, &names));
        match kind {
            WIREGUARD => {
                profile.set_interface(&nmprofile::free_interface("wg", &interfaces));
                if let Some(key) = nmsettings::generate_key().await {
                    profile.set_private_key(&key);
                }
            }
            BRIDGE => {
                profile.set_interface(&nmprofile::free_interface("br", &interfaces));
                profile.set_stp_enabled(true);
            }
            BOND => {
                profile.set_interface(&nmprofile::free_interface("bond", &interfaces));
                profile.set_bond_mode(BOND_MODES[0].1);
            }
            _ => {}
        }
        return Ok(Start {
            devices: nmsettings::devices("ethernet").await,
            profile,
            is_new: true,
            are_secrets_read: true,
            ports: Vec::new(),
        });
    }
    let saved_connection = nmsettings::load(&target).await?;
    let profile = saved_connection.profile.normalized();
    let ports = if matches!(profile.kind().as_str(), BRIDGE | BOND) {
        nmsettings::ports(&[profile.interface(), profile.uuid()]).await?
    } else {
        Vec::new()
    };
    Ok(Start {
        devices: nmsettings::devices("ethernet").await,
        profile,
        is_new: false,
        are_secrets_read: saved_connection.are_secrets_read,
        ports,
    })
}
