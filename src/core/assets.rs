macro_rules! asset {
    ($path:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/", $path))
    };
}

macro_rules! text_asset {
    ($path:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/", $path))
    };
}

pub const CAVA: &str = text_asset!("cava.conf");
pub const KITTY_THEME: &str = text_asset!("terminal/kitty-theme.conf");
pub const TERMINAL_SEQUENCES: &str = text_asset!("terminal/sequences.txt");
pub const TERMINAL_SCHEME: &str = text_asset!("terminal/scheme-base.json");
pub const LOGO: &str = text_asset!("icons/proscenio.svg");

const ICONS: [(&str, &[u8]); 13] = [
    ("arch-symbolic", asset!("icons/arch-symbolic.svg")),
    ("cachyos-symbolic", asset!("icons/cachyos-symbolic.svg")),
    (
        "cloudflare-dns-symbolic",
        asset!("icons/cloudflare-dns-symbolic.svg"),
    ),
    ("debian-symbolic", asset!("icons/debian-symbolic.svg")),
    (
        "endeavouros-symbolic",
        asset!("icons/endeavouros-symbolic.svg"),
    ),
    ("fedora-symbolic", asset!("icons/fedora-symbolic.svg")),
    ("gentoo-symbolic", asset!("icons/gentoo-symbolic.svg")),
    ("linux-symbolic", asset!("icons/linux-symbolic.svg")),
    ("manjaro-symbolic", asset!("icons/manjaro-symbolic.svg")),
    ("nixos-symbolic", asset!("icons/nixos-symbolic.svg")),
    ("nyarch-symbolic", asset!("icons/nyarch-symbolic.svg")),
    ("ubuntu-symbolic", asset!("icons/ubuntu-symbolic.svg")),
    ("wireguard-symbolic", asset!("icons/wireguard-symbolic.svg")),
];

const MICROPHONE: &[u8] = asset!("icons/micgate.png");
const DEFAULT_WALLPAPER: &[u8] = asset!("images/default_wallpaper.png");

pub fn default_wallpaper() -> Option<std::path::PathBuf> {
    let directory = gtk4::glib::user_data_dir().join("proscenio");
    let path = directory.join("default_wallpaper.png");
    if !path.exists() {
        std::fs::create_dir_all(&directory).ok()?;
        std::fs::write(&path, DEFAULT_WALLPAPER).ok()?;
    }
    Some(path)
}

pub fn microphone_icon() -> Option<std::path::PathBuf> {
    let path = crate::core::paths::runtime().join("micgate.png");
    if !path.exists() {
        std::fs::create_dir_all(crate::core::paths::runtime()).ok()?;
        std::fs::write(&path, MICROPHONE).ok()?;
    }
    Some(path)
}

pub fn icon(name: &str) -> Option<&'static [u8]> {
    ICONS
        .iter()
        .find(|(icon, _)| *icon == name)
        .map(|(_, bytes)| *bytes)
}
