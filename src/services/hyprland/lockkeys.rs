use crate::platform::hyprland::hypr;
use crate::platform::hyprland::hyprconfig::{self, Area};
use crate::platform::hyprland::lockkeys;

pub fn write_lock_keys() {
    let path = hyprconfig::path(Area::Binds);
    match hyprconfig::replace_if_changed(&path, lockkeys::with_lock_keys) {
        Ok(true) => {
            hypr::request("reload");
        }
        Ok(false) => {}
        Err(error) => eprintln!(
            "lock keys: {} could not be written: {error}",
            path.display()
        ),
    }
}
