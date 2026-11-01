const MIN_VERSION: usize = 1;
const MAX_VERSION: usize = 40;
const MASK_COUNT: u8 = 8;
const MEDIUM_FORMAT_BITS: u32 = 0b00;
const FORMAT_GENERATOR: u32 = 0x537;
const FORMAT_MASK: u32 = 0x5412;
const VERSION_GENERATOR: u32 = 0x1F25;
const FIELD_POLYNOMIAL: u32 = 0x11D;
const BYTE_MODE: u32 = 0b0100;
const PAD_BYTES: [u8; 2] = [0xEC, 0x11];
const RUN_PENALTY: i64 = 3;
const BLOCK_PENALTY: i64 = 3;
const FINDER_PENALTY: i64 = 40;
const BALANCE_PENALTY: i64 = 10;

const ERROR_CODEWORDS_PER_BLOCK: [usize; MAX_VERSION] = [
    10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28,
    28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
];
const BLOCK_COUNTS: [usize; MAX_VERSION] = [
    1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25,
    26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49,
];

pub struct Code {
    size: usize,
    modules: Vec<bool>,
}

impl Code {
    pub fn size(&self) -> usize {
        self.size
    }

    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }
}

pub fn encode(data: &[u8]) -> Option<Code> {
    build(data, MIN_VERSION, None)
}

fn build(data: &[u8], min_version: usize, forced_mask: Option<u8>) -> Option<Code> {
    let version = (min_version..=MAX_VERSION).find(|&version| {
        4 + count_length_bits(version) + 8 * data.len() <= 8 * count_data_codewords(version)
    })?;
    let codewords = add_error_correction(&pack_data(data, version), version);
    let mut grid = Grid::new(version);
    grid.draw_function_patterns(version);
    grid.draw_codewords(&codewords);
    let mask = forced_mask.unwrap_or_else(|| pick_mask(&mut grid));
    grid.apply_mask(mask);
    grid.draw_format(mask);
    Some(Code {
        size: grid.size,
        modules: grid.modules,
    })
}

fn count_length_bits(version: usize) -> usize {
    if version < 10 { 8 } else { 16 }
}

fn count_raw_modules(version: usize) -> usize {
    let mut count = (16 * version + 128) * version + 64;
    if version >= 2 {
        let alignment_count = version / 7 + 2;
        count -= (25 * alignment_count - 10) * alignment_count - 55;
    }
    if version >= 7 {
        count -= 36;
    }
    count
}

fn count_data_codewords(version: usize) -> usize {
    count_raw_modules(version) / 8
        - ERROR_CODEWORDS_PER_BLOCK[version - 1] * BLOCK_COUNTS[version - 1]
}

fn list_alignment_positions(version: usize) -> Vec<usize> {
    if version == 1 {
        return Vec::new();
    }
    let count = version / 7 + 2;
    let size = version * 4 + 17;
    let step = if version == 32 {
        26
    } else {
        (version * 4 + count * 2 + 1) / (count * 2 - 2) * 2
    };
    let mut positions: Vec<usize> = (0..count - 1).map(|i| size - 7 - i * step).collect();
    positions.push(6);
    positions.reverse();
    positions
}

fn is_bit_set(value: u32, index: i32) -> bool {
    (value >> index) & 1 != 0
}

fn push_bits(bits: &mut Vec<bool>, value: u32, count: usize) {
    bits.extend(
        (0..count)
            .rev()
            .map(|index| is_bit_set(value, index as i32)),
    );
}

fn pack_data(data: &[u8], version: usize) -> Vec<u8> {
    let capacity = count_data_codewords(version);
    let mut bits = Vec::new();
    push_bits(&mut bits, BYTE_MODE, 4);
    push_bits(&mut bits, data.len() as u32, count_length_bits(version));
    for &byte in data {
        push_bits(&mut bits, byte.into(), 8);
    }
    let terminator_length = (capacity * 8 - bits.len()).min(4);
    bits.resize(bits.len() + terminator_length, false);
    bits.resize(bits.len().next_multiple_of(8), false);
    let mut bytes: Vec<u8> = bits
        .chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0, |byte, &is_set| byte << 1 | u8::from(is_set))
        })
        .collect();
    for &pad in PAD_BYTES.iter().cycle().take(capacity - bytes.len()) {
        bytes.push(pad);
    }
    bytes
}

fn multiply(x: u8, y: u8) -> u8 {
    let mut product = 0u32;
    for index in (0..8).rev() {
        product = (product << 1) ^ ((product >> 7) * FIELD_POLYNOMIAL);
        product ^= u32::from((y >> index) & 1) * u32::from(x);
    }
    product as u8
}

fn compute_reed_solomon_divisor(degree: usize) -> Vec<u8> {
    let mut divisor = vec![0; degree];
    divisor[degree - 1] = 1;
    let mut root = 1;
    for _ in 0..degree {
        for index in 0..degree {
            divisor[index] = multiply(divisor[index], root);
            if index + 1 < degree {
                divisor[index] ^= divisor[index + 1];
            }
        }
        root = multiply(root, 2);
    }
    divisor
}

fn compute_reed_solomon_remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    let mut remainder = vec![0; divisor.len()];
    for &byte in data {
        let factor = byte ^ remainder.remove(0);
        remainder.push(0);
        for (value, &coefficient) in remainder.iter_mut().zip(divisor) {
            *value ^= multiply(coefficient, factor);
        }
    }
    remainder
}

fn add_error_correction(data: &[u8], version: usize) -> Vec<u8> {
    let block_count = BLOCK_COUNTS[version - 1];
    let error_length = ERROR_CODEWORDS_PER_BLOCK[version - 1];
    let raw_codewords = count_raw_modules(version) / 8;
    let short_block_count = block_count - raw_codewords % block_count;
    let short_data_length = raw_codewords / block_count - error_length;
    let divisor = compute_reed_solomon_divisor(error_length);
    let mut data_tail = data;
    let mut blocks = Vec::with_capacity(block_count);
    for index in 0..block_count {
        let length = short_data_length + usize::from(index >= short_block_count);
        let (block_data, rest) = data_tail.split_at(length);
        data_tail = rest;
        blocks.push((
            block_data,
            compute_reed_solomon_remainder(block_data, &divisor),
        ));
    }
    let mut codewords = Vec::with_capacity(raw_codewords);
    for index in 0..=short_data_length {
        codewords.extend(blocks.iter().filter_map(|(block, _)| block.get(index)));
    }
    for index in 0..error_length {
        codewords.extend(blocks.iter().map(|(_, parity)| parity[index]));
    }
    codewords
}

struct Grid {
    size: usize,
    modules: Vec<bool>,
    is_function: Vec<bool>,
}

impl Grid {
    fn new(version: usize) -> Self {
        let size = version * 4 + 17;
        Grid {
            size,
            modules: vec![false; size * size],
            is_function: vec![false; size * size],
        }
    }

    fn get(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }

    fn set_function(&mut self, x: i32, y: i32, is_dark: bool) {
        let size = self.size as i32;
        if !(0..size).contains(&x) || !(0..size).contains(&y) {
            return;
        }
        let index = y as usize * self.size + x as usize;
        self.modules[index] = is_dark;
        self.is_function[index] = true;
    }

    fn draw_function_patterns(&mut self, version: usize) {
        let size = self.size as i32;
        for index in 0..size {
            self.set_function(6, index, index % 2 == 0);
            self.set_function(index, 6, index % 2 == 0);
        }
        for (x, y) in [(3, 3), (size - 4, 3), (3, size - 4)] {
            self.draw_finder(x, y);
        }
        let positions = list_alignment_positions(version);
        let last_index = positions.len().saturating_sub(1);
        for (row, &y) in positions.iter().enumerate() {
            for (column, &x) in positions.iter().enumerate() {
                let is_on_finder = (row == 0 && column == 0)
                    || (row == 0 && column == last_index)
                    || (row == last_index && column == 0);
                if !is_on_finder {
                    self.draw_alignment(x as i32, y as i32);
                }
            }
        }
        self.draw_format(0);
        self.draw_version(version);
    }

    fn draw_finder(&mut self, center_x: i32, center_y: i32) {
        for dy in -4..=4i32 {
            for dx in -4..=4i32 {
                let distance = dx.abs().max(dy.abs());
                self.set_function(center_x + dx, center_y + dy, distance != 2 && distance != 4);
            }
        }
    }

    fn draw_alignment(&mut self, center_x: i32, center_y: i32) {
        for dy in -2..=2i32 {
            for dx in -2..=2i32 {
                let distance = dx.abs().max(dy.abs());
                self.set_function(center_x + dx, center_y + dy, distance != 1);
            }
        }
    }

    fn draw_format(&mut self, mask: u8) {
        let data = MEDIUM_FORMAT_BITS << 3 | u32::from(mask);
        let mut remainder = data;
        for _ in 0..10 {
            remainder = (remainder << 1) ^ ((remainder >> 9) * FORMAT_GENERATOR);
        }
        let bits = (data << 10 | remainder) ^ FORMAT_MASK;
        let size = self.size as i32;
        for index in 0..6 {
            self.set_function(8, index, is_bit_set(bits, index));
        }
        self.set_function(8, 7, is_bit_set(bits, 6));
        self.set_function(8, 8, is_bit_set(bits, 7));
        self.set_function(7, 8, is_bit_set(bits, 8));
        for index in 9..15 {
            self.set_function(14 - index, 8, is_bit_set(bits, index));
        }
        for index in 0..8 {
            self.set_function(size - 1 - index, 8, is_bit_set(bits, index));
        }
        for index in 8..15 {
            self.set_function(8, size - 15 + index, is_bit_set(bits, index));
        }
        self.set_function(8, size - 8, true);
    }

    fn draw_version(&mut self, version: usize) {
        if version < 7 {
            return;
        }
        let mut remainder = version as u32;
        for _ in 0..12 {
            remainder = (remainder << 1) ^ ((remainder >> 11) * VERSION_GENERATOR);
        }
        let bits = (version as u32) << 12 | remainder;
        let size = self.size as i32;
        for index in 0..18 {
            let is_dark = is_bit_set(bits, index);
            let corner = size - 11 + index % 3;
            let offset = index / 3;
            self.set_function(corner, offset, is_dark);
            self.set_function(offset, corner, is_dark);
        }
    }

    fn draw_codewords(&mut self, codewords: &[u8]) {
        let size = self.size;
        let mut bit_index = 0;
        let mut right = size as i32 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            let is_upward = (right + 1) & 2 == 0;
            for step in 0..size {
                let y = if is_upward { size - 1 - step } else { step };
                for column in [right, right - 1] {
                    let index = y * size + column as usize;
                    if !self.is_function[index] && bit_index < codewords.len() * 8 {
                        self.modules[index] =
                            is_bit_set(codewords[bit_index / 8].into(), 7 - (bit_index % 8) as i32);
                        bit_index += 1;
                    }
                }
            }
            right -= 2;
        }
    }

    fn apply_mask(&mut self, mask: u8) {
        for y in 0..self.size {
            for x in 0..self.size {
                let index = y * self.size + x;
                if !self.is_function[index] && is_inverted(mask, x, y) {
                    self.modules[index] = !self.modules[index];
                }
            }
        }
    }
}

fn is_inverted(mask: u8, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y).is_multiple_of(2),
        1 => y.is_multiple_of(2),
        2 => x.is_multiple_of(3),
        3 => (x + y).is_multiple_of(3),
        4 => (x / 3 + y / 2).is_multiple_of(2),
        5 => x * y % 2 + x * y % 3 == 0,
        6 => (x * y % 2 + x * y % 3).is_multiple_of(2),
        _ => ((x + y) % 2 + x * y % 3).is_multiple_of(2),
    }
}

fn pick_mask(grid: &mut Grid) -> u8 {
    let mut best_mask = 0;
    let mut lowest_score = i64::MAX;
    for mask in 0..MASK_COUNT {
        grid.apply_mask(mask);
        grid.draw_format(mask);
        let score = compute_penalty(grid);
        if score < lowest_score {
            best_mask = mask;
            lowest_score = score;
        }
        grid.apply_mask(mask);
    }
    best_mask
}

fn compute_penalty(grid: &Grid) -> i64 {
    let size = grid.size;
    let mut total = 0;
    for index in 0..size {
        total += compute_line_penalty((0..size).map(|x| grid.get(x, index)), size);
        total += compute_line_penalty((0..size).map(|y| grid.get(index, y)), size);
    }
    for y in 0..size - 1 {
        for x in 0..size - 1 {
            let color = grid.get(x, y);
            if color == grid.get(x + 1, y)
                && color == grid.get(x, y + 1)
                && color == grid.get(x + 1, y + 1)
            {
                total += BLOCK_PENALTY;
            }
        }
    }
    let dark_count = grid.modules.iter().filter(|&&is_dark| is_dark).count() as i64;
    let module_count = (size * size) as i64;
    let deviation = (dark_count * 20 - module_count * 10).abs();
    total + ((deviation + module_count - 1) / module_count - 1) * BALANCE_PENALTY
}

fn compute_line_penalty(line: impl Iterator<Item = bool>, size: usize) -> i64 {
    let mut total = 0;
    let mut color = false;
    let mut run = 0;
    let mut history = [0usize; 7];
    for module in line {
        if module == color {
            run += 1;
            if run == 5 {
                total += RUN_PENALTY;
            } else if run > 5 {
                total += 1;
            }
        } else {
            push_run(&mut history, run, size);
            if !color {
                total += count_finders(&history) * FINDER_PENALTY;
            }
            color = module;
            run = 1;
        }
    }
    if color {
        push_run(&mut history, run, size);
        run = 0;
    }
    push_run(&mut history, run + size, size);
    total + count_finders(&history) * FINDER_PENALTY
}

fn push_run(history: &mut [usize; 7], run: usize, size: usize) {
    let run = if history[0] == 0 { run + size } else { run };
    history.rotate_right(1);
    history[0] = run;
}

fn count_finders(history: &[usize; 7]) -> i64 {
    let unit = history[1];
    let has_core = unit > 0
        && history[2] == unit
        && history[3] == unit * 3
        && history[4] == unit
        && history[5] == unit;
    i64::from(has_core && history[0] >= unit * 4 && history[6] >= unit)
        + i64::from(has_core && history[6] >= unit * 4 && history[0] >= unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_rows(code: &Code) -> Vec<String> {
        (0..code.size())
            .map(|y| {
                (0..code.size())
                    .map(|x| if code.is_dark(x, y) { '#' } else { '.' })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_iso_example_gets_its_error_correction_codewords() {
        let data = [
            0x10, 0x20, 0x0C, 0x56, 0x61, 0x80, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11,
            0xEC, 0x11,
        ];
        assert_eq!(
            compute_reed_solomon_remainder(&data, &compute_reed_solomon_divisor(10)),
            [0xA5, 0x24, 0xD4, 0xC1, 0xED, 0x36, 0xC7, 0x87, 0x2C, 0x55]
        );
    }

    #[test]
    fn the_field_multiplies_modulo_the_qr_polynomial() {
        assert_eq!(multiply(0x57, 0x83), 0x31);
        assert_eq!(multiply(0x80, 2), 0x1D);
        assert_eq!(multiply(0, 0xFF), 0);
    }

    #[test]
    fn data_is_packed_with_header_terminator_and_pad_bytes() {
        assert_eq!(
            pack_data(b"01234567", 1),
            [
                0x40, 0x83, 0x03, 0x13, 0x23, 0x33, 0x43, 0x53, 0x63, 0x70, 0xEC, 0x11, 0xEC, 0x11,
                0xEC, 0x11
            ]
        );
    }

    #[test]
    fn matrices_equal_the_reference_encoder() {
        let reference = include_str!("reference.txt");
        let cases: Vec<&str> = reference
            .split("\n\n")
            .filter(|case| !case.is_empty())
            .collect();
        assert_eq!(cases.len(), 16);
        for case in cases {
            let (header, expected) = case.split_once('\n').unwrap();
            let mut fields = header.splitn(4, ' ');
            let _level = fields.next().unwrap();
            let min_version = fields.next().unwrap().parse().unwrap();
            let mask = fields.next().unwrap().parse().ok();
            let text = fields.next().unwrap();
            let code = build(text.as_bytes(), min_version, mask).unwrap();
            assert_eq!(render_rows(&code).join("\n"), expected, "{header}");
        }
    }

    #[test]
    fn every_version_equals_the_reference_encoder() {
        let mut count = 0;
        for line in include_str!("checksums.txt").lines() {
            let fields: Vec<&str> = line.split(' ').collect();
            let version: usize = fields[0].parse().unwrap();
            let code = build(b"Wi-Fi", version, None).unwrap();
            let mut hash = 14695981039346656037u64;
            for y in 0..code.size() {
                for x in 0..code.size() {
                    hash = (hash ^ u64::from(code.is_dark(x, y))).wrapping_mul(1099511628211);
                }
            }
            let dark_count = (0..code.size() * code.size())
                .filter(|&index| code.is_dark(index % code.size(), index / code.size()))
                .count();
            assert_eq!(code.size(), version * 4 + 17);
            assert_eq!(dark_count.to_string(), fields[1], "version {version}");
            assert_eq!(format!("{hash:016x}"), fields[2], "version {version}");
            count += 1;
        }
        assert_eq!(count, MAX_VERSION);
    }

    #[test]
    fn the_smallest_fitting_version_is_picked() {
        for (length, version) in [(0, 1), (14, 1), (15, 2), (213, 10), (214, 11), (2331, 40)] {
            let code = encode(&vec![b'a'; length]).unwrap();
            assert_eq!(code.size(), version * 4 + 17, "{length} bytes");
        }
        assert!(encode(&[b'a'; 2332]).is_none());
    }
}
