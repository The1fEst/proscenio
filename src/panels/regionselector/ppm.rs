use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use super::stitch::Frame;

const CHANNELS: usize = 3;

pub fn parse(mut bytes: Vec<u8>) -> Option<Frame> {
    if !bytes.starts_with(b"P6") {
        return None;
    }
    let mut position = 2;
    let mut numbers = [0usize; 3];
    for number in &mut numbers {
        loop {
            match bytes.get(position)? {
                b'#' => {
                    while *bytes.get(position)? != b'\n' {
                        position += 1;
                    }
                }
                byte if byte.is_ascii_whitespace() => position += 1,
                _ => break,
            }
        }
        let start = position;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            position += 1;
        }
        *number = std::str::from_utf8(&bytes[start..position])
            .ok()?
            .parse()
            .ok()?;
    }
    let [width, height, max_value] = numbers;
    let start = position + 1;
    let length = width.checked_mul(height)?.checked_mul(CHANNELS)?;
    if max_value != 255 || bytes.len() < start.checked_add(length)? {
        return None;
    }
    bytes.drain(..start);
    bytes.truncate(length);
    Some(Frame {
        width,
        height,
        pixels: bytes,
    })
}

pub fn write(path: &Path, frame: &Frame) -> io::Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    write!(writer, "P6\n{} {}\n255\n", frame.width, frame.height)?;
    writer.write_all(&frame.pixels)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_frame_parses_back_unchanged() {
        let folder = std::env::temp_dir().join(format!("proscenio-ppm-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("frame");
        let frame = Frame {
            width: 2,
            height: 3,
            pixels: (0..18).collect(),
        };
        write(&path, &frame).unwrap();
        let parsed = parse(std::fs::read(&path).unwrap());
        let _ = std::fs::remove_dir_all(&folder);
        let parsed = parsed.unwrap();
        assert_eq!(
            (parsed.width, parsed.height, parsed.pixels),
            (2, 3, frame.pixels)
        );
    }

    #[test]
    fn a_header_with_comments_and_a_short_or_foreign_file_are_handled() {
        let with_comment = [b"P6\n# grim\n1 2\n255\n".as_slice(), &[1, 2, 3, 4, 5, 6]].concat();
        let parsed = parse(with_comment).unwrap();
        assert_eq!(
            (parsed.width, parsed.height, parsed.pixels),
            (1, 2, vec![1, 2, 3, 4, 5, 6])
        );
        assert!(parse([b"P6\n1 2\n255\n".as_slice(), &[1, 2, 3]].concat()).is_none());
        assert!(parse(b"P5\n1 1\n255\n\0".to_vec()).is_none());
        assert!(parse(b"P6\n1 1\n65535\n\0\0\0\0\0\0".to_vec()).is_none());
        assert!(parse(b"P6\n1".to_vec()).is_none());
        assert!(parse(Vec::new()).is_none());
    }
}
