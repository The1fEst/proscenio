fn main() {
    println!("cargo:rerun-if-changed=protocols/hyprland-focus-grab-v1.xml");
    println!("cargo:rerun-if-changed=protocols/hyprland-global-shortcuts-v1.xml");
    println!("cargo:rerun-if-changed=protocols/hyprland-lock-notify-v1.xml");
    println!("cargo:rerun-if-changed=protocols/hyprland-toplevel-export-v1.xml");
    println!("cargo:rerun-if-changed=protocols/linux-dmabuf-v1.xml");
}
