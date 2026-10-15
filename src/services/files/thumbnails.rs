use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::glib;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const MTIME_KEY: &str = "Thumb::MTime";
const URI_KEY: &str = "Thumb::URI";
const MTIME_NANOSECONDS_KEY: &str = "X-Proscenio::MTime-Nanoseconds";

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
    path_in(&cache_folder(), file, size)
}

pub fn generate(file: &Path, size: Size) -> bool {
    generate_in(&cache_folder(), file, size)
}

fn cache_folder() -> PathBuf {
    glib::user_cache_dir().join("thumbnails")
}

fn path_in(folder: &Path, file: &Path, size: Size) -> Option<PathBuf> {
    let uri = glib::filename_to_uri(file, None).ok()?;
    let hash = glib::compute_checksum_for_string(glib::ChecksumType::Md5, &uri)?;
    Some(folder.join(size.folder()).join(format!("{hash}.png")))
}

fn is_fresh_in(folder: &Path, file: &Path, size: Size) -> bool {
    let (Some(thumbnail), Some(mtime)) = (path_in(folder, file, size), read_mtime(file)) else {
        return false;
    };
    text_chunk(&thumbnail, MTIME_KEY)
        .is_some_and(|stored_mtime| stored_mtime == mtime.as_secs().to_string())
        && text_chunk(&thumbnail, MTIME_NANOSECONDS_KEY)
            .is_none_or(|stored_nanoseconds| stored_nanoseconds == mtime.subsec_nanos().to_string())
}

fn generate_in(folder: &Path, file: &Path, size: Size) -> bool {
    if is_fresh_in(folder, file, size) {
        return false;
    }
    let (Some(target), Some(mtime)) = (path_in(folder, file, size), read_mtime(file)) else {
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
    let Some(size_folder) = target.parent() else {
        return false;
    };
    let _ = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(size_folder);
    let staging_path = target.with_extension("png.part");
    let mtime_seconds = mtime.as_secs().to_string();
    let mtime_nanoseconds = mtime.subsec_nanos().to_string();
    let save_result = pixbuf.savev(
        &staging_path,
        "png",
        &[
            (&format!("tEXt::{URI_KEY}"), uri.as_str()),
            (&format!("tEXt::{MTIME_KEY}"), mtime_seconds.as_str()),
            (
                &format!("tEXt::{MTIME_NANOSECONDS_KEY}"),
                mtime_nanoseconds.as_str(),
            ),
            ("tEXt::Software", "proscenio"),
        ],
    );
    if save_result.is_err() {
        let _ = std::fs::remove_file(&staging_path);
        return false;
    }
    let _ = std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o600));
    std::fs::rename(&staging_path, &target).is_ok()
}

fn read_mtime(file: &Path) -> Option<Duration> {
    let time = std::fs::metadata(file).ok()?.modified().ok()?;
    time.duration_since(UNIX_EPOCH).ok()
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
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::create_dir_all(&folder);
        let image = folder.join("image with space.png");
        let pixbuf = Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, false, 8, 600, 300).unwrap();
        pixbuf.fill(0x336699ff);
        pixbuf.savev(&image, "png", &[]).unwrap();
        let cache = folder.join("cache");

        let thumbnail = path_in(&cache, &image, Size::Large).unwrap();
        assert!(!is_fresh_in(&cache, &image, Size::Large));
        assert!(generate_in(&cache, &image, Size::Large));
        assert!(is_fresh_in(&cache, &image, Size::Large));
        assert!(!generate_in(&cache, &image, Size::Large));
        let (_, width, height) = Pixbuf::file_info(&thumbnail).unwrap();
        assert_eq!((width, height), (256, 128));
        assert_eq!(
            text_chunk(&thumbnail, URI_KEY).as_deref(),
            glib::filename_to_uri(&image, None).ok().as_deref()
        );

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_picture_rewritten_within_the_same_second_is_thumbnailed_again() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-thumbs-second-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::create_dir_all(&folder);
        let image = folder.join("image.png");
        let pixbuf = Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, false, 8, 64, 64).unwrap();
        pixbuf.savev(&image, "png", &[]).unwrap();
        let base_time = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let set_modified = |time| {
            File::options()
                .write(true)
                .open(&image)
                .unwrap()
                .set_modified(time)
                .unwrap()
        };
        set_modified(base_time + Duration::from_millis(100));
        let cache = folder.join("cache");
        assert!(generate_in(&cache, &image, Size::Normal));
        assert!(is_fresh_in(&cache, &image, Size::Normal));

        set_modified(base_time + Duration::from_millis(600));
        let is_generated_again = generate_in(&cache, &image, Size::Normal);

        let _ = std::fs::remove_dir_all(&folder);
        assert!(is_generated_again);
    }
}
