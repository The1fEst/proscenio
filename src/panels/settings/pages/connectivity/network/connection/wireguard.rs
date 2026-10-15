use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;

use super::fields::{note, show_note};
use super::{Editor, MTU_MAX, NOTE_START, PORT_MAX};
use crate::core::i18n::{tr, trf};
use crate::core::tools;
use crate::panels::settings::content::Page;
use crate::platform::network::nmprofile::wireguard::Peer;
use crate::platform::network::nmprofile::{self, Profile, WIREGUARD};
use crate::services::network::nmsettings;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::icon_button;

const KEEPALIVE_MAX: i64 = 3600;

type PeerEdit = Box<dyn FnOnce(&mut Peer) -> Result<(), String>>;

impl Editor {
    pub(super) fn wireguard(self: &Rc<Self>, page: &Page) {
        let section = self.section("vpn_key", &tr("WireGuard"));
        let profile = self.draft.borrow().clone();
        self.field(
            &section,
            &tr("Interface name"),
            &profile.interface(),
            |text, profile| {
                profile.set_interface(text.trim());
                Ok(())
            },
        );
        let keys_section = self.subsection(page, &section, &tr("Keys"), "");
        let public_key_label = text::styled_sized("", pixel_size::SMALLER);
        text::set_color(&public_key_label, "colSubtext");
        public_key_label.set_xalign(0.0);
        public_key_label.set_selectable(true);
        public_key_label.set_wrap(true);
        public_key_label.set_wrap_mode(gtk4::pango::WrapMode::Char);
        public_key_label.set_margin_start(NOTE_START);
        let show_public = {
            let public_key_label = public_key_label.downgrade();
            move |key: String| {
                let Some(public_key_label) = public_key_label.upgrade() else {
                    return;
                };
                public_key_label.set_visible(nmprofile::wireguard::is_valid_key(&key));
                if !public_key_label.is_visible() {
                    return;
                }
                glib::spawn_future_local(async move {
                    let public_key = nmsettings::public_key(&key).await.unwrap_or_default();
                    public_key_label.set_text(&trf("Public key: %1", &[&public_key]));
                });
            }
        };
        show_public(profile.private_key());
        let private_key_field = self.secret_field(
            &keys_section,
            &tr("Private key"),
            &profile.private_key(),
            profile.is_agent_owned(WIREGUARD, "private-key-flags"),
            move |text, profile| {
                profile.set_private_key(text);
                show_public(text.trim().to_owned());
                Ok(())
            },
        );
        keys_section.append(&public_key_label);
        let (generate_button, _) = icon_button(&self.theme, "key", true, &tr("Generate a new key"));
        generate_button.set_halign(gtk4::Align::Start);
        generate_button.connect_clicked({
            let private_key_field = Rc::downgrade(&private_key_field);
            move |_| {
                let private_key_field = private_key_field.clone();
                glib::spawn_future_local(async move {
                    let key = nmsettings::generate_key().await;
                    if let (Some(private_key_field), Some(key)) = (private_key_field.upgrade(), key)
                    {
                        private_key_field.set_text(&key);
                    }
                });
            }
        });
        let missing_tools = tools::find_missing(&[&tools::WG]);
        if !missing_tools.is_empty() {
            generate_button.set_sensitive(false);
            self.hold(page.unkept_tip(
                &generate_button,
                &tools::missing_message(&missing_tools, &tr("no key is made")),
            ));
        }
        keys_section.append(&generate_button);
        self.spin(
            &section,
            "settings_input_antenna",
            &tr("Listen port (0 picks one)"),
            (0, PORT_MAX),
            profile.listen_port() as i64,
            |profile, port| profile.set_listen_port(port as u32),
        );
        self.spin(
            &section,
            "straighten",
            &tr("MTU (0 picks it automatically)"),
            (0, MTU_MAX),
            profile.mtu() as i64,
            |profile, mtu| profile.set_mtu(mtu as u32),
        );
    }

    pub(super) fn peers(self: &Rc<Self>, page: &Page) {
        let section = self.section("hub", &tr("Peers"));
        let peers = self.draft.borrow().peers();
        let are_secrets_read = self.are_secrets_read.get();
        if peers.is_empty() {
            let empty_note = note();
            show_note(&empty_note, Some(&tr("No peers yet")), "colSubtext");
            section.append(&empty_note);
        }
        for (index, peer) in peers.iter().enumerate() {
            let group = self.subsection(
                page,
                &section,
                &trf("Peer %1", &[&(index + 1).to_string()]),
                "",
            );
            let change = move |edit: PeerEdit, profile: &mut Profile| -> Result<(), String> {
                let mut peers = profile.peers();
                if let Some(peer) = peers.get_mut(index) {
                    edit(peer)?;
                }
                profile.set_peers(&peers);
                Ok(())
            };
            self.field(
                &group,
                &tr("Public key"),
                &peer.public_key,
                move |text, profile| {
                    let text = text.trim().to_owned();
                    change(
                        Box::new(move |peer| {
                            peer.public_key = text;
                            Ok(())
                        }),
                        profile,
                    )
                },
            );
            self.field(
                &group,
                &tr("Endpoint"),
                &peer.endpoint,
                move |text, profile| {
                    let endpoint = nmprofile::wireguard::parse_endpoint(text)?;
                    change(
                        Box::new(move |peer| {
                            peer.endpoint = endpoint;
                            Ok(())
                        }),
                        profile,
                    )
                },
            );
            self.field(
                &group,
                &tr("Allowed IPs"),
                &peer.allowed_ips.join(", "),
                move |text, profile| {
                    let networks = nmprofile::wireguard::parse_networks(text)?;
                    change(
                        Box::new(move |peer| {
                            peer.allowed_ips = networks;
                            Ok(())
                        }),
                        profile,
                    )
                },
            );
            self.secret_field(
                &group,
                &tr("Preshared key (optional)"),
                &peer.preshared_key,
                peer.is_preshared_key_agent_owned(),
                move |text, profile| {
                    let key = text.trim().to_owned();
                    change(
                        Box::new(move |peer| {
                            peer.set_preshared_key(key, are_secrets_read);
                            Ok(())
                        }),
                        profile,
                    )
                },
            );
            self.spin(
                &group,
                "timer",
                &tr("Keepalive in seconds (0 is off)"),
                (0, KEEPALIVE_MAX),
                peer.keepalive as i64,
                move |profile, seconds| {
                    let _ = change(
                        Box::new(move |peer| {
                            peer.keepalive = seconds as u32;
                            Ok(())
                        }),
                        profile,
                    );
                },
            );
            let (remove_button, _) = icon_button(&self.theme, "delete", false, &tr("Remove peer"));
            remove_button.set_halign(gtk4::Align::Start);
            remove_button.connect_clicked({
                let editor = Rc::downgrade(self);
                move |_| {
                    if let Some(editor) = editor.upgrade() {
                        let _ = editor.edit(|profile| {
                            let mut peers = profile.peers();
                            if index < peers.len() {
                                peers.remove(index);
                            }
                            profile.set_peers(&peers);
                            Ok(())
                        });
                        editor.errors.borrow_mut().clear();
                        editor.queue_rebuild();
                    }
                }
            });
            group.append(&remove_button);
        }
        let (add_button, _) = icon_button(&self.theme, "add", true, &tr("Add peer"));
        add_button.set_halign(gtk4::Align::Start);
        add_button.connect_clicked({
            let editor = Rc::downgrade(self);
            move |_| {
                if let Some(editor) = editor.upgrade() {
                    let _ = editor.edit(|profile| {
                        let mut peers = profile.peers();
                        peers.push(Peer::default());
                        profile.set_peers(&peers);
                        Ok(())
                    });
                    editor.queue_rebuild();
                }
            }
        });
        section.append(&add_button);
    }
}
