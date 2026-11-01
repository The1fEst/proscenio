use std::cmp::Reverse;
use std::collections::HashMap;

pub const MAX_ROWS: usize = 16_384;
pub const MAX_BYTES: usize = 128 * 1024 * 1024;
const CHANNELS: usize = 3;
const MAX_MISSES: u32 = 8;
const MIN_FRAME_HEIGHT: usize = 32;
const MIN_MATCHED_ROWS: usize = 6;
const MIN_MATCH_PERCENT: usize = 60;
const MAX_REPEATS: usize = 4;
const MARGIN_PERCENT: usize = 8;
const MIN_NOISE_ROWS: usize = 4;
const NOISE_DIVISOR: usize = 16;
const PREVIEW_SAMPLES: usize = 3;

pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Start,
    Growth(usize),
    Revisit,
    Standstill,
    Mismatch,
    Limit,
}

struct Rows {
    hashes: Vec<u64>,
    positions: HashMap<u64, Vec<usize>>,
}

impl Rows {
    fn new(hashes: Vec<u64>) -> Self {
        let mut positions: HashMap<u64, Vec<usize>> = HashMap::new();
        for (index, hash) in hashes.iter().enumerate() {
            positions.entry(*hash).or_default().push(index);
        }
        Rows { hashes, positions }
    }
}

pub struct Stitcher {
    max_rows: usize,
    max_bytes: usize,
    width: usize,
    height: usize,
    row_limit: usize,
    canvas: Vec<u8>,
    previous_rows: Option<Rows>,
    anchor: isize,
    has_moved: bool,
    misses: u32,
}

impl Stitcher {
    pub fn new() -> Self {
        Self::with_limits(MAX_ROWS, MAX_BYTES)
    }

    pub fn with_limits(max_rows: usize, max_bytes: usize) -> Self {
        Stitcher {
            max_rows,
            max_bytes,
            width: 0,
            height: 0,
            row_limit: 0,
            canvas: Vec::new(),
            previous_rows: None,
            anchor: 0,
            has_moved: false,
            misses: 0,
        }
    }

    pub fn rows(&self) -> usize {
        self.canvas.len().checked_div(self.row_bytes()).unwrap_or(0)
    }

    pub fn has_moved(&self) -> bool {
        self.has_moved
    }

    pub fn has_lost_track(&self) -> bool {
        self.misses >= MAX_MISSES
    }

    pub fn is_full(&self) -> bool {
        self.previous_rows.is_some() && self.rows() >= self.row_limit
    }

    fn row_bytes(&self) -> usize {
        self.width * CHANNELS
    }

    pub fn reject(&mut self) -> Outcome {
        self.misses += 1;
        Outcome::Mismatch
    }

    pub fn push(&mut self, frame: Frame) -> Outcome {
        if self.is_full() {
            return Outcome::Limit;
        }
        if frame.width == 0
            || frame.height < MIN_FRAME_HEIGHT
            || frame.pixels.len() != frame.width * frame.height * CHANNELS
        {
            return self.reject();
        }
        let Some(previous_rows) = &self.previous_rows else {
            return self.start(frame);
        };
        if frame.width != self.width || frame.height != self.height {
            return self.reject();
        }
        let hashes = hash_rows(&frame);
        let changed_rows: Vec<usize> = (0..self.height)
            .filter(|&index| hashes[index] != previous_rows.hashes[index])
            .collect();
        if changed_rows.is_empty() {
            self.misses = 0;
            return Outcome::Standstill;
        }
        let footer_rows = hashes
            .iter()
            .zip(&previous_rows.hashes)
            .rev()
            .take_while(|(row_hash, previous_hash)| row_hash == previous_hash)
            .count();
        let Some(shift) = find_shift(&hashes, &changed_rows, previous_rows) else {
            if changed_rows.len() <= MIN_NOISE_ROWS.max(self.height / NOISE_DIVISOR) {
                self.misses = 0;
                return Outcome::Standstill;
            }
            return self.reject();
        };
        self.place(&frame, hashes, shift, footer_rows)
    }

    fn start(&mut self, frame: Frame) -> Outcome {
        self.width = frame.width;
        self.height = frame.height;
        self.row_limit = self.max_rows.min(self.max_bytes / self.row_bytes()).max(1);
        let count = self.height.min(self.row_limit);
        self.previous_rows = Some(Rows::new(hash_rows(&frame)));
        self.canvas = frame.pixels;
        self.canvas.truncate(count * self.row_bytes());
        self.misses = 0;
        if self.is_full() {
            Outcome::Limit
        } else {
            Outcome::Start
        }
    }

    fn place(
        &mut self,
        frame: &Frame,
        hashes: Vec<u64>,
        shift: isize,
        footer_rows: usize,
    ) -> Outcome {
        let content_end = self.height - footer_rows;
        let canvas_rows = if self.has_moved {
            self.rows()
        } else {
            content_end
        };
        let new_top = self.anchor + shift;
        let Ok(start) = usize::try_from(canvas_rows as isize - new_top) else {
            return self.reject();
        };
        let row_bytes = self.row_bytes();
        if !self.has_moved {
            self.canvas.truncate(content_end * row_bytes);
            self.has_moved = true;
        }
        let room = self.row_limit - self.rows();
        let count = content_end.saturating_sub(start).min(room);
        if count > 0 {
            self.canvas.reserve_exact(count * row_bytes);
            self.canvas
                .extend_from_slice(&frame.pixels[start * row_bytes..(start + count) * row_bytes]);
        }
        self.anchor = new_top;
        self.previous_rows = Some(Rows::new(hashes));
        self.misses = 0;
        match (self.is_full(), count) {
            (true, _) => Outcome::Limit,
            (false, 0) => Outcome::Revisit,
            (false, count) => Outcome::Growth(count),
        }
    }

    pub fn build_preview(&self, max_width: usize, max_height: usize) -> Option<Frame> {
        let rows = self.rows();
        if rows == 0 || max_width == 0 || max_height == 0 {
            return None;
        }
        let width = self.width;
        let out_width = width.min(max_width);
        let scale = out_width as f64 / width as f64;
        let shown_rows = rows.min((max_height as f64 / scale) as usize).max(1);
        let out_height = ((shown_rows as f64 * scale).round() as usize).max(1);
        let first_row = rows - shown_rows;
        let mut pixels = Vec::with_capacity(out_width * out_height * CHANNELS);
        for out_y in 0..out_height {
            let top = first_row + out_y * shown_rows / out_height;
            let bottom = (first_row + (out_y + 1) * shown_rows / out_height).max(top + 1);
            for out_x in 0..out_width {
                let left = out_x * width / out_width;
                let right = ((out_x + 1) * width / out_width).max(left + 1);
                for channel in 0..CHANNELS {
                    let mut sum = 0usize;
                    for sample_y in 0..PREVIEW_SAMPLES {
                        let y = top + (bottom - top) * sample_y / PREVIEW_SAMPLES;
                        for sample_x in 0..PREVIEW_SAMPLES {
                            let x = left + (right - left) * sample_x / PREVIEW_SAMPLES;
                            sum += self.canvas[(y * width + x) * CHANNELS + channel] as usize;
                        }
                    }
                    pixels.push((sum / (PREVIEW_SAMPLES * PREVIEW_SAMPLES)) as u8);
                }
            }
        }
        Some(Frame {
            width: out_width,
            height: out_height,
            pixels,
        })
    }

    pub fn into_frame(self) -> Option<Frame> {
        let height = self.rows();
        (height > 0).then_some(Frame {
            width: self.width,
            height,
            pixels: self.canvas,
        })
    }
}

fn hash_rows(frame: &Frame) -> Vec<u64> {
    let margin = frame.width * MARGIN_PERCENT / 100;
    let (start_byte, end_byte) = (margin * CHANNELS, (frame.width - margin) * CHANNELS);
    let row_bytes = frame.width * CHANNELS;
    frame
        .pixels
        .chunks_exact(row_bytes)
        .map(|row| hash_row(&row[start_byte..end_byte]))
        .collect()
}

fn hash_row(row: &[u8]) -> u64 {
    const MULTIPLIER: u64 = 0x517c_c1b7_2722_0a95;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let (words, remainder) = row.as_chunks::<8>();
    for word in words {
        let value = u64::from_le_bytes(*word);
        hash = (hash.rotate_left(5) ^ value).wrapping_mul(MULTIPLIER);
    }
    for byte in remainder {
        hash = (hash.rotate_left(5) ^ u64::from(*byte)).wrapping_mul(MULTIPLIER);
    }
    hash
}

fn find_shift(hashes: &[u64], changed_rows: &[usize], previous_rows: &Rows) -> Option<isize> {
    let mut votes: HashMap<isize, usize> = HashMap::new();
    for &index in changed_rows {
        let Some(positions) = previous_rows.positions.get(&hashes[index]) else {
            continue;
        };
        if positions.len() > MAX_REPEATS {
            continue;
        }
        for &position in positions {
            *votes.entry(position as isize - index as isize).or_default() += 1;
        }
    }
    let (shift, _) = votes
        .into_iter()
        .max_by_key(|&(shift, count)| (count, Reverse(shift.unsigned_abs()), shift))?;
    let (mut matched_count, mut compared_count) = (0, 0);
    for &index in changed_rows {
        let Some(&previous_hash) = usize::try_from(index as isize + shift)
            .ok()
            .and_then(|position| previous_rows.hashes.get(position))
        else {
            continue;
        };
        compared_count += 1;
        matched_count += usize::from(previous_hash == hashes[index]);
    }
    (matched_count >= MIN_MATCHED_ROWS && matched_count * 100 >= compared_count * MIN_MATCH_PERCENT)
        .then_some(shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: usize = 40;
    const FRAME_HEIGHT: usize = 100;

    fn generate_row(seed: u64) -> Vec<u8> {
        let mut state = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (0..WIDTH * CHANNELS)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (state >> 56) as u8
            })
            .collect()
    }

    fn generate_rows(seed: u64, count: usize) -> Vec<Vec<u8>> {
        (0..count as u64)
            .map(|index| generate_row(seed + index))
            .collect()
    }

    fn build_frame(rows: &[Vec<u8>]) -> Frame {
        Frame {
            width: WIDTH,
            height: rows.len(),
            pixels: rows.concat(),
        }
    }

    fn capture(page: &[Vec<u8>], offset: usize) -> Frame {
        build_frame(&page[offset..offset + FRAME_HEIGHT])
    }

    fn read_canvas(stitcher: Stitcher) -> Vec<Vec<u8>> {
        let frame = stitcher.into_frame().unwrap();
        frame
            .pixels
            .chunks(WIDTH * CHANNELS)
            .map(<[u8]>::to_vec)
            .collect()
    }

    #[test]
    fn scrolling_down_rebuilds_the_page_from_overlapping_frames() {
        let mut page = generate_rows(1, 300);
        for blank in &mut page[120..160] {
            *blank = vec![255; WIDTH * CHANNELS];
        }
        let mut stitcher = Stitcher::new();
        let outcomes: Vec<Outcome> = [0, 30, 70, 150, 200]
            .into_iter()
            .map(|offset| stitcher.push(capture(&page, offset)))
            .collect();
        assert_eq!(
            outcomes,
            [
                Outcome::Start,
                Outcome::Growth(30),
                Outcome::Growth(40),
                Outcome::Growth(80),
                Outcome::Growth(50),
            ]
        );
        assert!(read_canvas(stitcher) == page);
    }

    #[test]
    fn a_header_and_a_footer_that_stay_in_place_are_not_repeated() {
        let page = generate_rows(1, 300);
        let header = generate_rows(5000, 8);
        let footer = generate_rows(6000, 8);
        let capture_with_bars = |offset: usize| {
            build_frame(&[&header[..], &page[offset..offset + 84], &footer[..]].concat())
        };
        let mut stitcher = Stitcher::new();
        for offset in [0, 30, 60, 100] {
            assert_ne!(
                stitcher.push(capture_with_bars(offset)),
                Outcome::Mismatch,
                "{offset}"
            );
        }
        let expected_rows = [&header[..], &page[..184]].concat();
        assert!(read_canvas(stitcher) == expected_rows);
    }

    #[test]
    fn a_frame_without_movement_adds_nothing() {
        let page = generate_rows(1, 200);
        let mut stitcher = Stitcher::new();
        stitcher.push(capture(&page, 0));
        assert_eq!(stitcher.push(capture(&page, 0)), Outcome::Standstill);
        let mut blinking_rows = page[..FRAME_HEIGHT].to_vec();
        blinking_rows[40] = generate_row(9000);
        blinking_rows[41] = generate_row(9001);
        assert_eq!(
            stitcher.push(build_frame(&blinking_rows)),
            Outcome::Standstill
        );
        assert_eq!(stitcher.rows(), FRAME_HEIGHT);
        assert!(!stitcher.has_moved());
    }

    #[test]
    fn a_frame_that_matches_nothing_is_rejected_and_the_next_match_carries_on() {
        let page = generate_rows(1, 200);
        let other = generate_rows(70_000, 200);
        let mut stitcher = Stitcher::new();
        stitcher.push(capture(&page, 0));
        assert_eq!(stitcher.push(capture(&other, 0)), Outcome::Mismatch);
        assert_eq!(stitcher.rows(), FRAME_HEIGHT);
        assert_eq!(stitcher.push(capture(&page, 40)), Outcome::Growth(40));
        assert_eq!(stitcher.rows(), 140);
    }

    #[test]
    fn the_stitcher_gives_up_after_consecutive_misses_and_a_match_resets_the_count() {
        let page = generate_rows(1, 200);
        let other = generate_rows(70_000, 200);
        let mut stitcher = Stitcher::new();
        stitcher.push(capture(&page, 0));
        for _ in 0..MAX_MISSES - 1 {
            stitcher.push(capture(&other, 0));
        }
        assert!(!stitcher.has_lost_track());
        stitcher.push(capture(&page, 20));
        for _ in 0..MAX_MISSES - 1 {
            stitcher.push(capture(&other, 0));
        }
        assert!(!stitcher.has_lost_track());
        stitcher.push(capture(&other, 0));
        assert!(stitcher.has_lost_track());
    }

    #[test]
    fn scrolling_back_does_not_repeat_rows() {
        let page = generate_rows(1, 300);
        let mut stitcher = Stitcher::new();
        let outcomes: Vec<Outcome> = [0, 60, 20, 100]
            .into_iter()
            .map(|offset| stitcher.push(capture(&page, offset)))
            .collect();
        assert_eq!(
            outcomes,
            [
                Outcome::Start,
                Outcome::Growth(60),
                Outcome::Revisit,
                Outcome::Growth(40),
            ]
        );
        assert!(read_canvas(stitcher) == page[..200]);
    }

    #[test]
    fn the_stitched_height_stops_at_the_row_cap_and_at_the_byte_cap() {
        let page = generate_rows(1, 300);
        let row_bytes = WIDTH * CHANNELS;
        for mut stitcher in [
            Stitcher::with_limits(150, usize::MAX),
            Stitcher::with_limits(usize::MAX, 150 * row_bytes),
        ] {
            assert_eq!(stitcher.push(capture(&page, 0)), Outcome::Start);
            assert_eq!(stitcher.push(capture(&page, 60)), Outcome::Limit);
            assert_eq!(stitcher.rows(), 150);
            assert!(stitcher.is_full());
            assert_eq!(stitcher.push(capture(&page, 100)), Outcome::Limit);
            assert!(read_canvas(stitcher) == page[..150]);
        }
    }

    #[test]
    fn a_frame_of_another_size_is_rejected() {
        let page = generate_rows(1, 200);
        let mut stitcher = Stitcher::new();
        stitcher.push(capture(&page, 0));
        assert_eq!(stitcher.push(build_frame(&page[..60])), Outcome::Mismatch);
        assert_eq!(stitcher.push(build_frame(&page[..10])), Outcome::Mismatch);
        assert_eq!(stitcher.rows(), FRAME_HEIGHT);
    }

    #[test]
    fn the_preview_shows_the_last_rows_and_never_enlarges() {
        let rows: Vec<Vec<u8>> = (0..FRAME_HEIGHT)
            .map(|index| vec![index as u8 * 2; WIDTH * CHANNELS])
            .collect();
        let mut stitcher = Stitcher::new();
        assert!(stitcher.build_preview(20, 10).is_none());
        stitcher.push(build_frame(&rows));
        let full_preview = stitcher.build_preview(1000, 1000).unwrap();
        assert_eq!(
            (full_preview.width, full_preview.height),
            (WIDTH, FRAME_HEIGHT)
        );
        let small_preview = stitcher.build_preview(20, 10).unwrap();
        assert_eq!((small_preview.width, small_preview.height), (20, 10));
        assert!(
            small_preview.pixels[0] >= 160,
            "{}",
            small_preview.pixels[0]
        );
        let bottom_value = small_preview.pixels[(9 * 20) * CHANNELS];
        assert!((190..=198).contains(&bottom_value), "{bottom_value}");
    }
}
