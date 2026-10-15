use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;

use super::fields::note;
use super::{BUTTON_SPACING, Editor, Part};
use crate::core::i18n::{tr, trf};
use crate::platform::network::nmprofile::{BOND, BRIDGE, Profile};
use crate::services::network::nmsettings;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::icon_button;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const DIALOG_WIDTH: f64 = 400.0;

pub(super) struct Actions {
    pub(super) save_button: RippleButton,
    pub(super) revert_button: RippleButton,
    pub(super) delete_button: Option<RippleButton>,
    pub(super) status: gtk4::Label,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Operation {
    Save,
    SecretRead,
    Delete,
}

async fn sync_ports(
    controller: &Profile,
    should_recreate_ports: bool,
    port_devices: &[String],
    ports: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let interface = controller.interface();
    let should_autoconnect = controller.is_autoconnect_enabled();
    let previous_ports = ports.clone();
    for (uuid, device) in &previous_ports {
        if should_recreate_ports || !port_devices.contains(device) {
            nmsettings::delete(uuid).await?;
            ports.retain(|(port_uuid, _)| port_uuid != uuid);
        } else {
            nmsettings::set_autoconnect(uuid, should_autoconnect).await?;
        }
    }
    for device in port_devices {
        let has_port = ports.iter().any(|(_, known)| known == device);
        if !has_port {
            let mut port = Profile::new_port(&interface, &controller.kind(), device);
            port.set_autoconnect_enabled(should_autoconnect);
            nmsettings::add(&port).await?;
            ports.push((port.uuid(), device.clone()));
        }
    }
    Ok(())
}

fn pick_port_devices(
    current_devices: &[String],
    previous_devices: &[String],
    stored_ports: Option<&[(String, String)]>,
) -> Vec<String> {
    match stored_ports {
        Some(stored_ports) if current_devices == previous_devices => stored_ports
            .iter()
            .map(|(_, device)| device.clone())
            .collect(),
        _ => current_devices.to_vec(),
    }
}

impl Editor {
    pub(super) fn actions_section(self: &Rc<Self>) {
        let section = self.section("", "");
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
        let (save_button, _) = icon_button(&self.theme, "save", true, &tr("Save"));
        save_button.connect_clicked({
            let editor = Rc::downgrade(self);
            move |_| {
                if let Some(editor) = editor.upgrade() {
                    editor.save();
                }
            }
        });
        row.append(&save_button);
        let (revert_button, _) = icon_button(&self.theme, "undo", false, &tr("Revert"));
        revert_button.connect_clicked({
            let editor = Rc::downgrade(self);
            move |_| {
                if let Some(editor) = editor.upgrade() {
                    let saved_profile = editor.saved_profile.borrow().clone();
                    editor.draft.replace(saved_profile);
                    let devices = editor
                        .saved_ports
                        .borrow()
                        .iter()
                        .map(|(_, device)| device.clone())
                        .collect();
                    editor.port_devices.replace(devices);
                    editor.errors.borrow_mut().clear();
                    editor
                        .are_secrets_edited
                        .set(editor.are_saved_secrets_edited.get());
                    editor.outcome.replace(None);
                    editor.queue_rebuild();
                }
            }
        });
        row.append(&revert_button);
        let mut delete_button = None;
        if !self.is_new.get() && matches!(self.part.get(), Part::Main) {
            row.append(&windowdialog::spacer());
            let (button, _) = icon_button(&self.theme, "delete", false, &tr("Delete"));
            button.connect_clicked({
                let editor = Rc::downgrade(self);
                move |_| {
                    if let Some(editor) = editor.upgrade() {
                        editor.confirm_delete();
                    }
                }
            });
            row.append(&button);
            delete_button = Some(button);
        }
        section.append(&row);
        let status = note();
        section.append(&status);
        self.actions.replace(Some(Actions {
            save_button,
            revert_button,
            delete_button,
            status,
        }));
    }

    fn begin(&self, operation: Operation) -> bool {
        if self.operation.get().is_some() {
            return false;
        }
        self.operation.set(Some(operation));
        self.outcome.replace(None);
        self.refresh();
        true
    }

    pub(super) fn reveal_secrets(self: &Rc<Self>) {
        if !self.begin(Operation::SecretRead) {
            return;
        }
        let uuid = self.saved_profile.borrow().uuid();
        let editor = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let fetch_result = nmsettings::fetch_secrets(&uuid).await;
            let Some(editor) = editor.upgrade() else {
                return;
            };
            editor.operation.set(None);
            match fetch_result {
                Ok(secret_sets) => {
                    for secrets in &secret_sets {
                        editor.draft.borrow_mut().merge_secrets(secrets, false);
                        editor
                            .saved_profile
                            .borrow_mut()
                            .merge_secrets(secrets, false);
                    }
                    editor.are_secrets_read.set(true);
                    editor.are_saved_secrets_edited.set(false);
                    editor.queue_rebuild();
                }
                Err(message) => {
                    editor.outcome.replace(Some(Err(message)));
                    editor.refresh();
                }
            }
        });
    }

    fn save(self: &Rc<Self>) {
        if !self.begin(Operation::Save) {
            return;
        }
        let profile = self.draft.borrow().clone();
        let is_new = self.is_new.get();
        let are_secrets_read = self.are_secrets_read.get();
        let are_unread_secrets_edited = !are_secrets_read && self.are_secrets_edited.get();
        let should_fill_secrets = are_unread_secrets_edited
            || (!are_secrets_read && profile.has_changed_peer_with_stored_key());
        let port_devices = self.port_devices.borrow().clone();
        let mut ports = self.saved_ports.borrow().clone();
        let previous_interface = if is_new {
            String::new()
        } else {
            self.saved_profile.borrow().interface()
        };
        let mut stale_interfaces = self.stale_port_interfaces.borrow().clone();
        if !previous_interface.is_empty()
            && previous_interface != profile.interface()
            && !stale_interfaces.contains(&previous_interface)
        {
            stale_interfaces.push(previous_interface);
        }
        stale_interfaces.retain(|name| *name != profile.interface());
        let editor = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let uuid = profile.uuid();
            let write_result = if is_new {
                nmsettings::add(&profile).await.map(|()| None)
            } else {
                nmsettings::save(&profile, should_fill_secrets).await
            };
            let (write_result, stored_profile) = match write_result {
                Ok(stored_profile) => (Ok(()), stored_profile),
                Err(message) => (Err(message), None),
            };
            let is_stored = write_result.is_ok();
            let mut sync_result = Ok(());
            if is_stored && matches!(profile.kind().as_str(), BRIDGE | BOND) {
                let should_recreate_ports = !stale_interfaces.is_empty();
                sync_result =
                    sync_ports(&profile, should_recreate_ports, &port_devices, &mut ports).await;
                let mut controller_ids = vec![profile.interface(), uuid.clone()];
                if sync_result.is_err() {
                    controller_ids.extend(stale_interfaces.iter().cloned());
                }
                if let Ok(listed_ports) = nmsettings::ports(&controller_ids).await {
                    ports = listed_ports;
                }
            }
            let outcome = match write_result {
                Ok(()) if !is_new && nmsettings::is_active(&uuid).await => {
                    nmsettings::reactivate(&uuid)
                        .await
                        .map(|()| tr("Saved and reconnected"))
                }
                Ok(()) => Ok(tr("Saved")),
                Err(message) => Err(message),
            };
            let are_ports_synced = is_stored && sync_result.is_ok();
            let outcome = sync_result.and(outcome);
            let Some(editor) = editor.upgrade() else {
                return;
            };
            editor.operation.set(None);
            let devices = pick_port_devices(
                &editor.port_devices.borrow(),
                &port_devices,
                are_ports_synced.then_some(ports.as_slice()),
            );
            editor.port_devices.replace(devices);
            editor.saved_ports.replace(ports);
            let is_draft_saved =
                *editor.draft.borrow() == profile && editor.errors.borrow().is_empty();
            if is_stored {
                editor.is_port_sync_pending.set(!are_ports_synced);
                if are_ports_synced {
                    stale_interfaces.clear();
                }
                editor.stale_port_interfaces.replace(stale_interfaces);
                editor.draft.borrow_mut().rebase_stored_peer_keys(&profile);
                let mut profile = profile;
                profile.reset_stored_peer_keys();
                editor.saved_profile.replace(profile);
                editor.is_new.set(false);
                if is_draft_saved {
                    editor.are_secrets_edited.set(false);
                }
                editor
                    .are_saved_secrets_edited
                    .set(are_unread_secrets_edited && !is_draft_saved);
                editor.retitle();
            }
            editor.outcome.replace(Some(outcome));
            if is_new && is_stored {
                editor.queue_rebuild();
            } else if let Some(mut stored_profile) = stored_profile
                && is_draft_saved
            {
                stored_profile.reset_stored_peer_keys();
                editor.draft.replace(stored_profile.clone());
                editor.saved_profile.replace(stored_profile);
                editor.are_secrets_read.set(true);
                editor.queue_rebuild();
            } else {
                editor.refresh();
            }
        });
    }

    fn confirm_delete(self: &Rc<Self>) {
        let name = self.saved_profile.borrow().id();
        let dialog = WindowDialog::new(&self.theme, None);
        dialog.set_background_width(DIALOG_WIDTH);
        dialog.column.add(
            &windowdialog::title(&trf("Delete %1?", &[&name])),
            Place::wide(),
        );
        let description =
            text::styled(&tr("Its settings and saved passwords are removed for good"));
        text::set_color(&description, "colOnSurfaceVariant");
        description.set_wrap(true);
        description.set_xalign(0.0);
        dialog.column.add(&description, Place::wide());
        let (buttons, place) = windowdialog::button_row();
        buttons.append(&windowdialog::spacer());
        let cancel_button = windowdialog::button(&self.theme, &tr("Cancel"));
        cancel_button.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        buttons.append(&cancel_button);
        let delete_button = windowdialog::button(&self.theme, &tr("Delete"));
        delete_button.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let editor = Rc::downgrade(self);
            move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                if !editor.begin(Operation::Delete) {
                    return;
                }
                let uuid = editor.saved_profile.borrow().uuid();
                let ports = editor.saved_ports.borrow().clone();
                let editor = Rc::downgrade(&editor);
                glib::spawn_future_local(async move {
                    let mut deleted_count = 0;
                    let mut delete_result = Ok(());
                    for (port, _) in &ports {
                        delete_result = nmsettings::delete(port).await;
                        if delete_result.is_err() {
                            break;
                        }
                        deleted_count += 1;
                    }
                    if delete_result.is_ok() {
                        delete_result = nmsettings::delete(&uuid).await;
                    }
                    let Some(editor) = editor.upgrade() else {
                        return;
                    };
                    match delete_result {
                        Ok(()) => (editor.go_back)(),
                        Err(message) => {
                            editor.operation.set(None);
                            let (deleted_ports, remaining_ports) = ports.split_at(deleted_count);
                            editor.port_devices.borrow_mut().retain(|device| {
                                let is_on_device =
                                    |(_, port_device): &(String, String)| port_device == device;
                                !deleted_ports.iter().any(is_on_device)
                                    || remaining_ports.iter().any(is_on_device)
                            });
                            editor.saved_ports.replace(remaining_ports.to_vec());
                            editor.outcome.replace(Some(Err(message)));
                            editor.refresh();
                            if !deleted_ports.is_empty() {
                                editor.queue_rebuild();
                            }
                        }
                    }
                });
            }
        });
        buttons.append(&delete_button);
        dialog.column.add(&buttons, place);
        (self.present)(dialog);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_switched_while_a_save_runs_outlast_it() {
        let devices = |names: &[&str]| -> Vec<String> {
            names.iter().map(|name| (*name).to_owned()).collect()
        };
        let stored_ports = [
            ("uuid-1".to_owned(), "eth0".to_owned()),
            ("uuid-2".to_owned(), "eth1".to_owned()),
        ];
        let previous_devices = devices(&["eth0", "eth1", "eth2"]);
        let switched_devices = devices(&["eth0"]);
        assert_eq!(
            pick_port_devices(&previous_devices, &previous_devices, Some(&stored_ports)),
            devices(&["eth0", "eth1"])
        );
        for stored_ports in [Some(&stored_ports[..]), None] {
            assert_eq!(
                pick_port_devices(&switched_devices, &previous_devices, stored_ports),
                switched_devices
            );
        }
        assert_eq!(
            pick_port_devices(&previous_devices, &previous_devices, None),
            previous_devices
        );
    }
}
