use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::glib;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const MTIME_KEY: &str = "Thumb::MTime";
const URI_KEY: &str = "Thumb::URI";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Normal,
    Large,
    XLarge,
    XXLarge,
}

impl Size {
    pub fn for_dimensions(width: f64, height: f64) -> Size {
        [Size::Normal, Size::Large, Size::XLarge]
            .into_iter()
            .find(|size| width <= size.pixels() as f64 && height <= size.pixels() as f64)
            .unwrap_or(Size::XXLarge)
    }

    pub fn pixels(self) -> i32 {
        match self {
            Size::Normal => 128,
            Size::Large => 256,
            Size::XLarge => 512,
            Size::XXLarge => 1024,
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Size::Normal => "normal",
            Size::Large => "large",
            Size::XLarge => "x-large",
            Size::XXLarge => "xx-large",
        }
    }
}

pub fn path(file: &Path, size: Size) -> Option<PathBuf> {
    let uri = glib::filename_to_uri(file, None).ok()?;
    let hash = glib::compute_checksum_for_string(glib::ChecksumType::Md5, &uri)?;
    Some(
        glib::user_cache_dir()
            .join("thumbnails")
            .join(size.folder())
            .join(format!("{hash}.png")),
    )
}

pub fn fresh(file: &Path, size: Size) -> bool {
    let (Some(thumbnail), Some(mtime)) = (path(file, size), modified(file)) else {
        return false;
    };
    text_chunk(&thumbnail, MTIME_KEY).is_some_and(|stored| stored == mtime.to_string())
}

pub fn generate(file: &Path, size: Size) -> bool {
    if fresh(file, size) {
        return false;
    }
    let (Some(target), Some(mtime)) = (path(file, size), modified(file)) else {
        return false;
    };
    let Ok(uri) = glib::filename_to_uri(file, None) else {
        return false;
    };
    let Some((_, width, height)) = Pixbuf::file_info(file) else {
        return false;
    };
    let limit = size.pixels();
    let pixbuf = if width <= limit && height <= limit {
        Pixbuf::from_file(file)
    } else {
        Pixbuf::from_file_at_scale(file, limit, limit, true)
    };
    let Ok(pixbuf) = pixbuf else {
        return false;
    };
    let Some(folder) = target.parent() else {
        return false;
    };
    let _ = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder);
    let staging = target.with_extension("png.part");
    let mtime = mtime.to_string();
    let saved = pixbuf.savev(
        &staging,
        "png",
        &[
            (&format!("tEXt::{URI_KEY}"), uri.as_str()),
            (&format!("tEXt::{MTIME_KEY}"), mtime.as_str()),
            ("tEXt::Software", "proscenio"),
        ],
    );
    if saved.is_err() {
        let _ = std::fs::remove_file(&staging);
        return false;
    }
    let _ = std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o600));
    std::fs::rename(&staging, &target).is_ok()
}

fn modified(file: &Path) -> Option<u64> {
    let time = std::fs::metadata(file).ok()?.modified().ok()?;
    Some(time.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

fn text_chunk(png: &Path, key: &str) -> Option<String> {
    let mut file = File::open(png).ok()?;
    let mut signature = [0u8; 8];
    file.read_exact(&mut signature).ok()?;
    if signature != PNG_SIGNATURE {
        return None;
    }
    loop {
        let mut header = [0u8; 8];
        file.read_exact(&mut header).ok()?;
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = &header[4..8];
        if kind == b"IDAT" || kind == b"IEND" {
            return None;
        }
        let mut data = vec![0u8; length + 4];
        file.read_exact(&mut data).ok()?;
        if kind != b"tEXt" {
            continue;
        }
        let body = &data[..length];
        let separator = body.iter().position(|byte| *byte == 0)?;
        if &body[..separator] == key.as_bytes() {
            return Some(String::from_utf8_lossy(&body[separator + 1..]).into_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_follow_the_shells_thresholds() {
        let cases = [
            ((128.0, 90.0), Size::Normal),
            ((129.0, 90.0), Size::Large),
            ((229.0, 164.0), Size::Large),
            ((300.0, 200.0), Size::XLarge),
            ((600.0, 400.0), Size::XXLarge),
            ((2000.0, 2000.0), Size::XXLarge),
        ];
        for ((width, height), expected) in cases {
            assert_eq!(Size::for_dimensions(width, height), expected);
        }
    }

    #[test]
    fn a_generated_thumbnail_is_fresh_until_the_image_changes() {
        let folder = std::env::temp_dir().join(format!("proscenio-thumbs-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&folder);
        let image = folder.join("image with space.png");
        let pixbuf = Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, false, 8, 600, 300).unwrap();
        pixbuf.fill(0x336699ff);
        pixbuf.savev(&image, "png", &[]).unwrap();

        let thumbnail = path(&image, Size::Large).unwrap();
        let _ = std::fs::remove_file(&thumbnail);
        assert!(!fresh(&image, Size::Large));
        assert!(generate(&image, Size::Large));
        assert!(fresh(&image, Size::Large));
        assert!(!generate(&image, Size::Large));
        let (_, width, height) = Pixbuf::file_info(&thumbnail).unwrap();
        assert_eq!((width, height), (256, 128));
        assert_eq!(
            text_chunk(&thumbnail, URI_KEY).as_deref(),
            glib::filename_to_uri(&image, None).ok().as_deref()
        );

        let _ = std::fs::remove_file(&thumbnail);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
