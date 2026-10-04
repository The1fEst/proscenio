---
title: Network settings
sidebar_label: Network
description: The Wi-Fi and Network pages and their subpages, from joining networks and sharing a connection to editing NetworkManager profiles, the system proxy and the firewall.
---

# Network settings

Two pages of the [settings window](settings.md) deal with networks. **Wi-Fi** joins wireless networks
and shares a connection as a hotspot; **Network** lists wired connections, VPNs and virtual
interfaces, edits any NetworkManager profile, and leads to the proxy and firewall settings.

Everything about connections goes through NetworkManager: `nmcli` lists, switches and deletes them,
and its D-Bus interface reads and writes whole profiles and their passwords. Without `nmcli` (from
the networkmanager package) or without NetworkManager on the system bus, Saved Networks, Hotspot and
the connection editor show only a notice saying which one is missing, and the Network page shows
that notice above its Firewall & Proxy links.

## Wi-Fi

| Control | Does |
|---|---|
| Wi-Fi | turns the radio on or off (`nmcli radio wifi on` or `off`) |
| Saved Networks | opens the subpage of saved Wi-Fi profiles |
| Connect to Hidden Network… | opens the subpage for joining a network that does not announce its name |
| Hotspot | opens the subpage that shares this computer's connection |

Without a Wi-Fi adapter the page says "No Wi-Fi Found", and with the radio off, "Wi-Fi Off".

While the radio is on, **Visible Networks** lists every network in range, with its signal strength
and a lock for a secured one. The page scans when it opens and every 15 seconds after that
(`nmcli device wifi list --rescan yes`), with a spinner beside the title while a scan runs.

- **Clicking a network** joins it. A saved or open network joins with `nmcli device wifi connect`.
  A secured network that is not saved first asks for its password in the row; the password goes to
  NetworkManager's `AddAndActivateConnection`, and once the network is joined an older profile with
  the same name is deleted. When NetworkManager refuses, the row asks for the password again.
- **An enterprise network** (802.1X) that is not saved asks for nothing in the row: the click opens
  the connection editor on a blank WPA & WPA2 Enterprise profile named after it.
- **Disconnect**, on the joined network, brings its connection down.
- **Forget**, on a saved network, deletes its profile.

The sidebar's Wi-Fi dialog shows the same list and behaves the same way ([Sidebar](sidebar.md)).

### Saved Networks

One card per saved Wi-Fi profile, or "No Saved Networks" when there are none. A card shows the
network's name over "Connected", "Joins on its own" or "Only when chosen", and has:

| Control | Does |
|---|---|
| Automatic | joins the network whenever it is in range (`nmcli connection modify … connection.autoconnect yes` or `no`) |
| Edit | opens the profile in the connection editor |
| Forget | deletes the profile |

### Connect to Hidden Network

A hidden network does not announce itself, so it has to be named in full.

| Control | Does |
|---|---|
| Network name | the network's exact name |
| Password | its password; empty for an open network |
| Connect | joins it; available while a name is typed and nothing is connecting, and reads "Connecting…" meanwhile |

Enter in either field connects too, and the password field is cleared after every attempt. With an
empty password and a saved profile of that name, the saved profile is brought up. Otherwise the
network goes to NetworkManager as a profile marked hidden, and an older profile with the same name is
deleted once it joins. Afterwards a line under the button reads "Connected" or NetworkManager's
error.

### Hotspot

Other devices join this network to use this computer's internet connection. The Wi-Fi adapter
cannot join another network while it shares, so the connection to share is usually a wired one.

| Control | Does |
|---|---|
| Network name | the name other devices see; the computer's host name until a hotspot profile exists |
| Password | 8 to 63 characters |
| Band | Automatic, 2.4 GHz or 5 GHz |
| Share the connection | starts or stops the hotspot |

Turning sharing on deletes the old profile named `Hotspot` and runs
`nmcli device wifi hotspot con-name Hotspot ssid <name> password <password>`, adding `band bg` or
`band a` for a chosen band. Turning it off brings `Hotspot` down. The switch shows whether that
profile is active, and NetworkManager's error appears under it when starting fails. The fields start
from the stored profile, read with `nmcli -s`.

## Network

Three sections list NetworkManager's connections, sorted by name:

| Section | Connection types | Add buttons |
|---|---|---|
| Wired | `802-3-ethernet`, except the ports of a bridge or bond | Add wired connection |
| VPN | `vpn`, `wireguard`, `tun`, `ip-tunnel` | Add WireGuard, Add OpenVPN, Import from a file… |
| Virtual interfaces | `vlan`, `bridge`, `bond` | Add VLAN, Add bridge, Add bond |

A card shows the connection's name over "Connected · *device*" or "Not connected", an **Edit**
button that opens the connection editor, and a switch that brings the connection up or down
(`nmcli connection up` or `down`) and then shows what NetworkManager reports. The lists follow
NetworkManager's changes, refreshed half a second after the last one.

Each add button opens the connection editor on a blank profile of that type.

- **Add OpenVPN** is disabled, with a tooltip saying why, unless NetworkManager's OpenVPN plugin
  (networkmanager-openvpn) is installed.
- **Import from a file…** asks `kdialog` for a WireGuard `.conf` or an OpenVPN `.ovpn` file and
  imports it with `nmcli connection import`, then opens the editor on the imported connection. An
  `.ovpn` file needs the OpenVPN plugin. NetworkManager's message appears under the buttons when the
  import fails. Without `kdialog` the button is disabled.

The page ends with **Firewall & Proxy**: a link row to the Firewall subpage, whose second line reads
On, Off or Not installed, and one to the Proxy subpage, whose second line names the proxy mode. This
section shows even when NetworkManager is not running.

### The connection editor

The Connection subpage edits one NetworkManager profile. It opens from a card's **Edit** button, the
add buttons, an import, or an enterprise Wi-Fi network; its header shows the profile's name. Opened
without a profile, as a search result does, it edits the first active connection, or says there is
nothing to edit.

A blank profile is named "Wired connection", "WireGuard", "OpenVPN", "VLAN", "Bridge" or "Bond", or
after the network for enterprise Wi-Fi, with " 2", " 3" and so on added when the name is taken.

**Reading.** The profile comes from NetworkManager's `GetSettings`, and its passwords and keys from
`GetSecrets` for each part of the profile that holds any, without asking for authorization. A secret
that could not be read that way shows an empty field and the line "Stored and hidden. Type to
replace it, or press the eye to show it". The eye button then asks again, letting polkit ask for a
password; once the secrets are read, the eye shows and hides the text.

**Editing and saving.** Changes go into a draft. Every text field checks its value as it is typed and
shows what is wrong under itself. The draft is kept while the editor or one of its subpages (IPv4,
IPv6, Authentication) is shown, so a change made on a subpage is still there back on the editor;
showing any other page, or closing the window, drops it.

| Button | Does |
|---|---|
| Save | available while the draft differs from the stored profile and nothing is wrong; writes the whole profile to disk with `Update2`, or with `AddConnection2` for a profile not stored yet. An active connection is then brought up again ("Saved and reconnected") |
| Revert | returns to the stored profile, and for a bridge or bond to its stored ports |
| Delete | asks first, then removes the profile, and for a bridge or bond its ports too, and goes back; shown on the editor itself for a stored profile |

When the draft holds a typed password but the stored ones were never read, saving reads them first,
letting polkit ask, so the others are kept. A line under the buttons shows "Saving…", the first
problem, or the outcome. Switching a method, security type or peer rebuilds the form at the same
scroll position; a field whose typed text was invalid then shows the last valid value again.

The form is built from these sections, depending on the profile's type:

**General**, for every profile:

| Control | Profile setting |
|---|---|
| Name | `connection.id` |
| Connect automatically | `connection.autoconnect` |
| Available to all users | off puts the current user into `connection.permissions`, keeping the connection to that account |
| Metered connection | Automatic, Yes or No (`connection.metered`); apps hold back on large downloads over a metered connection |

**Wired**: the device the profile is tied to ("Any device" or one Ethernet device), the cloned MAC
address (an address, or `preserve`, `permanent`, `random` or `stable`), the MTU (0 picks it), and
**802.1X security**, which signs in to the network port as office and campus networks ask and adds
an Authentication link row.

**Wi-Fi**: the network name (1 to 32 bytes), **Hidden network**, and **Security**: None, WPA & WPA2
Personal and WPA3 Personal with a password (for WPA & WPA2, 8 to 63 characters or 64 hexadecimal
digits), WEP with a key (5 or 13 characters, or 10 or 26 hexadecimal digits), or WPA & WPA2
Enterprise with an Authentication link row. A profile with any other kind of security keeps it and
shows a notice. The cloned MAC address and the MTU follow.

**Authentication** (the `802-1x` setting, a subpage):

| Control | Notes |
|---|---|
| Authentication | Protected EAP (PEAP), Tunneled TLS (TTLS) or TLS |
| User name | required |
| Anonymous identity, Inner authentication, Password | PEAP and TTLS; the inner method is MSCHAPv2, GTC or MD5 for PEAP, and PAP, MSCHAPv2, MSCHAP or CHAP for TTLS |
| User certificate, Private key, Private key password | TLS; the certificate and key are required |
| CA certificate | empty trusts any server |
| Server domain | `domain-suffix-match` |

A certificate field takes a path, or one picked with the folder button through `kdialog`, and stores
it as a `file://` reference. A certificate stored inside the profile is kept until a path replaces
it. Opened for a profile without 802.1X settings, the subpage goes straight back to the editor.

**WireGuard**:

| Control | Notes |
|---|---|
| Interface name | required; a blank profile takes the first free `wgN` |
| Private key | a blank profile gets one from `wg genkey`; the public key derived from it with `wg pubkey` shows below, selectable |
| Generate a new key | replaces the private key; disabled without `wg` (wireguard-tools) |
| Listen port | 0 picks one |
| MTU | 0 picks it |

**Peers** has one group per peer: its public key, endpoint (`host:port`), allowed IPs, an optional
preshared key, keepalive in seconds (0 is off) and **Remove peer**. **Add peer** adds an empty one.

**OpenVPN**: the gateway, the port (0 is the default, 1194), **Use TCP**, and **Authentication**:

| Kind | Fields |
|---|---|
| Certificates | CA certificate, user certificate, private key and its password |
| Password | CA certificate, user name, password |
| Password and certificates | both of the above |
| Static key | the key file, its direction, and the remote and local tunnel addresses |

Every kind except the static key adds an optional TLS authentication key (the `tls-auth` line of an
OpenVPN config) with its direction. The fields are the plugin's `vpn.data` entries, with its other
entries kept as they are, and the passwords its `vpn.secrets`, stored by NetworkManager.

**VPN**, for other VPN plugins: the plugin's name and a field for each entry of its `vpn.data`.

**VLAN**: the parent device (a blank profile takes the first Ethernet device), the VLAN ID from 1 to
4094, and the interface name, which NetworkManager picks when it is empty.

**Bridge** and **Bond**: the interface name (a blank profile takes the first free `brN` or `bondN`);
**Spanning tree (STP)** for a bridge, on for a blank one; **Mode** for a bond: Active backup, Round
robin, XOR, Broadcast, 802.3ad (LACP), Adaptive transmit or Adaptive load balancing, with link
monitoring (`miimon=100`). **Ports** has a switch per Ethernet device. Saving brings the port
profiles in line with the switches: a joining device gets an Ethernet profile named
"*interface* port *device*", with the controller as its `connection.master` and no IP settings; a
device switched off loses its port profile; ports take the controller's **Connect automatically**;
renaming the interface makes the ports anew.

An interface name, wherever the editor asks for one, has at most 15 characters and no spaces or
slashes.

**IPv4** and **IPv6** are subpages, reached from link rows that show the current method:

| Control | Notes |
|---|---|
| Method | Automatic, Automatic DHCP only (IPv6), Manual, Link-local only, Shared with other computers, Disabled |
| Addresses, Gateway | Manual; addresses separated by commas, each with its prefix length, such as `192.168.1.10/24` |
| DNS | Automatic DNS (automatic methods), the DNS servers and the search domains |
| Routing | Automatic routes (automatic methods), and **Only for its own network** (`never-default`), which keeps the connection from becoming the default route |
| Routes | separated by commas and written like `ip route`: `10.0.0.0/8 via 192.168.1.1 metric 100` |
| Privacy extensions | automatic IPv6 only: Default, Off, Prefer the fixed address, Prefer a temporary address |

Link-local and Disabled show only the method. Addresses and routes are written as `address-data` and
`route-data`, and DNS servers as `dns`, or as `dns-data` for servers that are not plain addresses.

Parsing and writing profiles lives in `src/platform/nmprofile.rs`, the D-Bus calls in
`src/services/nmsettings.rs`.

### Proxy

The Proxy subpage sets the system proxy that GTK apps, apps built on KIO, and programs reading the
usual proxy variables follow. It needs the `org.gnome.system.proxy` settings schema
(gsettings-desktop-schemas); without it the page is only a notice.

| Control | Does |
|---|---|
| Proxy | Off, Automatic or Manual |
| Configuration script | Automatic: the address of a proxy configuration script; empty finds the proxy on the network (WPAD) |
| HTTP proxy, HTTPS proxy, SOCKS proxy | Manual: each a host, a port (0 to 65535), a username and a password |
| Use the HTTP proxy for HTTPS too | Manual: hides the HTTPS proxy and uses the HTTP one for both |
| Not for these hosts | Manual: host names, domains such as `.example.org` and networks such as `10.0.0.0/8`, separated by commas |

The settings themselves live in `org.gnome.system.proxy`; the usernames and passwords live in the
Secret Service, one item each with the attributes `application proscenio`, `proxy` (`http`, `https`
or `socks`) and `field` (`user` or `password`), stored and read with `secret-tool`. The page fills in
once those are read.

A change is written 800 ms after the last one, or when the page closes, to every place apps look:

- `org.gnome.system.proxy` and its `http`, `https` and `socks` children. The HTTP username and
  password also go to `use-authentication`, `authentication-user` and `authentication-password`.
- `[Proxy Settings]` in `~/.config/kioslaverc`: `ProxyType` (0 off, 1 manual, 2 a script, 3 WPAD),
  `Proxy Config Script`, `httpProxy`, `httpsProxy`, `socksProxy` and `NoProxyFor`.
- The Secret Service items whose value changed, and only those, since storing into a locked keyring
  brings up its prompt.
- `http_proxy`, `https_proxy`, `all_proxy` (as `socks5://`) and `no_proxy`, in lower and upper case,
  as URLs with the username and password before the host. They are never written to a file:
  Hyprland gets them as `hl.env` calls through `hyprctl eval`, the systemd and D-Bus activation
  environment through `dbus-update-activation-environment --systemd`, and the shell its own
  environment, so whatever any of them starts next has them. Outside Manual they are set empty.

When the shell starts in Manual mode it reads the passwords and sets the variables again, since
nothing keeps them across sessions. The writer is `src/platform/proxy.rs`.

### Firewall

The Firewall subpage drives `ufw`; without it the page is only a notice. It reads the state without
root: `ENABLED` in `/etc/ufw/ufw.conf`, `DEFAULT_INPUT_POLICY` in `/etc/default/ufw`, and the rules
in `/etc/ufw/user.rules` and `user6.rules`.

| Control | Does |
|---|---|
| Firewall | turns `ufw` on (`ufw --force enable`, and `systemctl enable ufw.service` so it starts with the system) or off |
| Incoming connections no rule allows | Block drops them silently, Refuse tells the other side they were turned away, Allow lets them in (`ufw default <policy> incoming` with `deny`, `reject` or `allow`) |

**Rules** lists the incoming rules that apply to any local address, the IPv4 and IPv6 copies of a
rule once, as "Allow 22/tcp" over "from anywhere" or "from 10.0.0.0/8" and the rule's comment, each
with a remove button; "No rules yet" stands in for none. An application profile, or "every port",
stands in for the port where that is what the rule names.

**Add a rule**:

| Field | Notes |
|---|---|
| Action | Allow, Deny, Reject or Limit; Limit allows a connection but refuses an address that opens six or more within 30 seconds |
| Protocol | TCP and UDP, TCP, or UDP |
| Port | a port, a range or a list, such as `22`, `6000:6007` or `80,443` |
| From | where the connections come from, such as `192.168.1.0/24`; empty is anywhere |
| Comment | without single quotes or line breaks |

A rule needs a port or a source, and a range or a list needs TCP or UDP. A refused rule shows why; an
accepted one clears the fields.

Every change runs `pkexec proscenio firewall …` with `enable`, `disable`, `default POLICY`, or `add`
or `delete` and the rule. That command checks its arguments again and runs `ufw` with them directly,
never through a shell ([polkit](polkit.md)). The page reads the files again afterwards, and `ufw`'s
error, if any, appears under the switch and under the form. The code is in
`src/platform/firewall.rs`.
