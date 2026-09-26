use material_colors::color::Argb;
use material_colors::hct::Hct;
use std::collections::{BTreeMap, HashMap};

const INDEX_BITS: usize = 5;
const INDEX_COUNT: usize = (1 << INDEX_BITS) + 1;
const TOTAL_SIZE: usize = INDEX_COUNT * INDEX_COUNT * INDEX_COUNT;
const MAX_COLORS: usize = 256;
const MAX_ITERATIONS: usize = 100;
const MIN_DELTA_E: f64 = 3.0;
const SEED: u32 = 42688;
const WHITE_POINT: [f64; 3] = [95.047, 100.0, 108.883];

pub fn celebi(pixels: &[u32], max_colors: usize) -> BTreeMap<u32, u32> {
    let max_colors = max_colors.min(MAX_COLORS);
    let starting = wu(pixels, max_colors);
    wsmeans(pixels, &starting, max_colors)
}

struct Glibc {
    state: Vec<u32>,
}

impl Glibc {
    fn new(seed: u32) -> Self {
        let mut state = vec![0u32; 34];
        state[0] = seed;
        for index in 1..31 {
            let previous = state[index - 1] as i32 as i64;
            let mut next = (16807 * previous) % 2147483647;
            if next < 0 {
                next += 2147483647;
            }
            state[index] = next as u32;
        }
        for index in 31..34 {
            state[index] = state[index - 31];
        }
        let mut generator = Glibc { state };
        for _ in 34..344 {
            generator.step();
        }
        generator
    }

    fn step(&mut self) -> u32 {
        let length = self.state.len();
        let next = self.state[length - 31].wrapping_add(self.state[length - 3]);
        self.state.push(next);
        next
    }

    fn rand(&mut self) -> i32 {
        (self.step() >> 1) as i32
    }
}

fn red(argb: u32) -> i32 {
    ((argb & 0x00ff0000) >> 16) as i32
}

fn green(argb: u32) -> i32 {
    ((argb & 0x0000ff00) >> 8) as i32
}

fn blue(argb: u32) -> i32 {
    (argb & 0x000000ff) as i32
}

fn argb_from_rgb(red: i32, green: i32, blue: i32) -> u32 {
    0xFF000000
        | (((red & 0xff) as u32) << 16)
        | (((green & 0xff) as u32) << 8)
        | (blue & 0xff) as u32
}

fn linearized(component: i32) -> f64 {
    let normalized = component as f64 / 255.0;
    if normalized <= 0.040449936 {
        normalized / 12.92 * 100.0
    } else {
        ((normalized + 0.055) / 1.055).powf(2.4) * 100.0
    }
}

fn delinearized(component: f64) -> i32 {
    let normalized = component / 100.0;
    let delinearized = if normalized <= 0.0031308 {
        normalized * 12.92
    } else {
        1.055 * normalized.powf(1.0 / 2.4) - 0.055
    };
    ((delinearized * 255.0).round() as i32).clamp(0, 255)
}

#[derive(Clone, Copy, Default)]
struct Lab {
    l: f64,
    a: f64,
    b: f64,
}

impl Lab {
    fn delta_e(&self, other: &Lab) -> f64 {
        let (l, a, b) = (self.l - other.l, self.a - other.a, self.b - other.b);
        l * l + a * a + b * b
    }
}

fn lab_from_int(argb: u32) -> Lab {
    let (red_l, green_l, blue_l) = (
        linearized(red(argb)),
        linearized(green(argb)),
        linearized(blue(argb)),
    );
    let x = 0.41233895 * red_l + 0.35762064 * green_l + 0.18051042 * blue_l;
    let y = 0.2126 * red_l + 0.7152 * green_l + 0.0722 * blue_l;
    let z = 0.01932141 * red_l + 0.11916382 * green_l + 0.95034478 * blue_l;
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    let f = |normalized: f64| {
        if normalized > e {
            normalized.powf(1.0 / 3.0)
        } else {
            (kappa * normalized + 16.0) / 116.0
        }
    };
    let fy = f(y / WHITE_POINT[1]);
    let fx = f(x / WHITE_POINT[0]);
    let fz = f(z / WHITE_POINT[2]);
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

fn int_from_lab(lab: Lab) -> u32 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    let ke = 8.0;
    let fy = (lab.l + 16.0) / 116.0;
    let fx = lab.a / 500.0 + fy;
    let fz = fy - lab.b / 200.0;
    let fx3 = fx * fx * fx;
    let x_normalized = if fx3 > e {
        fx3
    } else {
        (116.0 * fx - 16.0) / kappa
    };
    let y_normalized = if lab.l > ke {
        fy * fy * fy
    } else {
        lab.l / kappa
    };
    let fz3 = fz * fz * fz;
    let z_normalized = if fz3 > e {
        fz3
    } else {
        (116.0 * fz - 16.0) / kappa
    };
    let x = x_normalized * WHITE_POINT[0];
    let y = y_normalized * WHITE_POINT[1];
    let z = z_normalized * WHITE_POINT[2];
    let r = 3.2406 * x - 1.5372 * y - 0.4986 * z;
    let g = -0.9689 * x + 1.8758 * y + 0.0415 * z;
    let b = 0.0557 * x - 0.2040 * y + 1.0570 * z;
    argb_from_rgb(delinearized(r), delinearized(g), delinearized(b))
}

#[derive(Clone, Copy, Default)]
struct Cube {
    r0: usize,
    r1: usize,
    g0: usize,
    g1: usize,
    b0: usize,
    b1: usize,
    vol: i64,
}

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Red,
    Green,
    Blue,
}

fn index(r: usize, g: usize, b: usize) -> usize {
    (r << (INDEX_BITS * 2)) + (r << (INDEX_BITS + 1)) + (g << INDEX_BITS) + r + g + b
}

struct Moments {
    weights: Vec<i64>,
    red: Vec<i64>,
    green: Vec<i64>,
    blue: Vec<i64>,
    squares: Vec<f64>,
}

fn top(cube: &Cube, direction: Direction, position: usize, moment: &[i64]) -> i64 {
    match direction {
        Direction::Red => {
            moment[index(position, cube.g1, cube.b1)]
                - moment[index(position, cube.g1, cube.b0)]
                - moment[index(position, cube.g0, cube.b1)]
                + moment[index(position, cube.g0, cube.b0)]
        }
        Direction::Green => {
            moment[index(cube.r1, position, cube.b1)]
                - moment[index(cube.r1, position, cube.b0)]
                - moment[index(cube.r0, position, cube.b1)]
                + moment[index(cube.r0, position, cube.b0)]
        }
        Direction::Blue => {
            moment[index(cube.r1, cube.g1, position)]
                - moment[index(cube.r1, cube.g0, position)]
                - moment[index(cube.r0, cube.g1, position)]
                + moment[index(cube.r0, cube.g0, position)]
        }
    }
}

fn bottom(cube: &Cube, direction: Direction, moment: &[i64]) -> i64 {
    match direction {
        Direction::Red => {
            -moment[index(cube.r0, cube.g1, cube.b1)]
                + moment[index(cube.r0, cube.g1, cube.b0)]
                + moment[index(cube.r0, cube.g0, cube.b1)]
                - moment[index(cube.r0, cube.g0, cube.b0)]
        }
        Direction::Green => {
            -moment[index(cube.r1, cube.g0, cube.b1)]
                + moment[index(cube.r1, cube.g0, cube.b0)]
                + moment[index(cube.r0, cube.g0, cube.b1)]
                - moment[index(cube.r0, cube.g0, cube.b0)]
        }
        Direction::Blue => {
            -moment[index(cube.r1, cube.g1, cube.b0)]
                + moment[index(cube.r1, cube.g0, cube.b0)]
                + moment[index(cube.r0, cube.g1, cube.b0)]
                - moment[index(cube.r0, cube.g0, cube.b0)]
        }
    }
}

fn volume(cube: &Cube, moment: &[i64]) -> i64 {
    moment[index(cube.r1, cube.g1, cube.b1)]
        - moment[index(cube.r1, cube.g1, cube.b0)]
        - moment[index(cube.r1, cube.g0, cube.b1)]
        + moment[index(cube.r1, cube.g0, cube.b0)]
        - moment[index(cube.r0, cube.g1, cube.b1)]
        + moment[index(cube.r0, cube.g1, cube.b0)]
        + moment[index(cube.r0, cube.g0, cube.b1)]
        - moment[index(cube.r0, cube.g0, cube.b0)]
}

fn variance(cube: &Cube, moments: &Moments) -> f64 {
    let dr = volume(cube, &moments.red) as f64;
    let dg = volume(cube, &moments.green) as f64;
    let db = volume(cube, &moments.blue) as f64;
    let m = &moments.squares;
    let xx = m[index(cube.r1, cube.g1, cube.b1)]
        - m[index(cube.r1, cube.g1, cube.b0)]
        - m[index(cube.r1, cube.g0, cube.b1)]
        + m[index(cube.r1, cube.g0, cube.b0)]
        - m[index(cube.r0, cube.g1, cube.b1)]
        + m[index(cube.r0, cube.g1, cube.b0)]
        + m[index(cube.r0, cube.g0, cube.b1)]
        - m[index(cube.r0, cube.g0, cube.b0)];
    let hypotenuse = dr * dr + dg * dg + db * db;
    xx - hypotenuse / volume(cube, &moments.weights) as f64
}

fn maximize(
    cube: &Cube,
    direction: Direction,
    first: usize,
    last: usize,
    whole: [i64; 4],
    moments: &Moments,
) -> (f64, i64) {
    let bottom_r = bottom(cube, direction, &moments.red);
    let bottom_g = bottom(cube, direction, &moments.green);
    let bottom_b = bottom(cube, direction, &moments.blue);
    let bottom_w = bottom(cube, direction, &moments.weights);
    let [whole_w, whole_r, whole_g, whole_b] = whole;
    let mut max = 0.0;
    let mut cut = -1;
    for position in first..last {
        let mut half_r = bottom_r + top(cube, direction, position, &moments.red);
        let mut half_g = bottom_g + top(cube, direction, position, &moments.green);
        let mut half_b = bottom_b + top(cube, direction, position, &moments.blue);
        let mut half_w = bottom_w + top(cube, direction, position, &moments.weights);
        if half_w == 0 {
            continue;
        }
        let mut temp = (half_r as f64 * half_r as f64
            + half_g as f64 * half_g as f64
            + half_b as f64 * half_b as f64)
            / half_w as f64;
        half_r = whole_r - half_r;
        half_g = whole_g - half_g;
        half_b = whole_b - half_b;
        half_w = whole_w - half_w;
        if half_w == 0 {
            continue;
        }
        temp += (half_r as f64 * half_r as f64
            + half_g as f64 * half_g as f64
            + half_b as f64 * half_b as f64)
            / half_w as f64;
        if temp > max {
            max = temp;
            cut = position as i64;
        }
    }
    (max, cut)
}

fn cut(first: &mut Cube, second: &mut Cube, moments: &Moments) -> bool {
    let whole = [
        volume(first, &moments.weights),
        volume(first, &moments.red),
        volume(first, &moments.green),
        volume(first, &moments.blue),
    ];
    let (max_r, cut_r) = maximize(
        first,
        Direction::Red,
        first.r0 + 1,
        first.r1,
        whole,
        moments,
    );
    let (max_g, cut_g) = maximize(
        first,
        Direction::Green,
        first.g0 + 1,
        first.g1,
        whole,
        moments,
    );
    let (max_b, cut_b) = maximize(
        first,
        Direction::Blue,
        first.b0 + 1,
        first.b1,
        whole,
        moments,
    );
    let direction = if max_r >= max_g && max_r >= max_b {
        if cut_r < 0 {
            return false;
        }
        Direction::Red
    } else if max_g >= max_r && max_g >= max_b {
        Direction::Green
    } else {
        Direction::Blue
    };
    second.r1 = first.r1;
    second.g1 = first.g1;
    second.b1 = first.b1;
    match direction {
        Direction::Red => {
            first.r1 = cut_r as usize;
            second.r0 = first.r1;
            second.g0 = first.g0;
            second.b0 = first.b0;
        }
        Direction::Green => {
            second.r0 = first.r0;
            first.g1 = cut_g as usize;
            second.g0 = first.g1;
            second.b0 = first.b0;
        }
        Direction::Blue => {
            second.r0 = first.r0;
            second.g0 = first.g0;
            first.b1 = cut_b as usize;
            second.b0 = first.b1;
        }
    }
    let size =
        |cube: &Cube| ((cube.r1 - cube.r0) * (cube.g1 - cube.g0) * (cube.b1 - cube.b0)) as i64;
    first.vol = size(first);
    second.vol = size(second);
    true
}

fn wu(pixels: &[u32], max_colors: usize) -> Vec<u32> {
    if max_colors == 0 || pixels.is_empty() {
        return Vec::new();
    }
    let mut moments = Moments {
        weights: vec![0; TOTAL_SIZE],
        red: vec![0; TOTAL_SIZE],
        green: vec![0; TOTAL_SIZE],
        blue: vec![0; TOTAL_SIZE],
        squares: vec![0.0; TOTAL_SIZE],
    };
    let shift = 8 - INDEX_BITS;
    for &pixel in pixels {
        let (r, g, b) = (red(pixel), green(pixel), blue(pixel));
        let at = index(
            (r as usize >> shift) + 1,
            (g as usize >> shift) + 1,
            (b as usize >> shift) + 1,
        );
        moments.weights[at] += 1;
        moments.red[at] += r as i64;
        moments.green[at] += g as i64;
        moments.blue[at] += b as i64;
        moments.squares[at] += (r * r + g * g + b * b) as f64;
    }
    for r in 1..INDEX_COUNT {
        let mut area = [0i64; INDEX_COUNT];
        let mut area_r = [0i64; INDEX_COUNT];
        let mut area_g = [0i64; INDEX_COUNT];
        let mut area_b = [0i64; INDEX_COUNT];
        let mut area_2 = [0f64; INDEX_COUNT];
        for g in 1..INDEX_COUNT {
            let (mut line, mut line_r, mut line_g, mut line_b, mut line_2) =
                (0i64, 0i64, 0i64, 0i64, 0f64);
            for b in 1..INDEX_COUNT {
                let at = index(r, g, b);
                line += moments.weights[at];
                line_r += moments.red[at];
                line_g += moments.green[at];
                line_b += moments.blue[at];
                line_2 += moments.squares[at];
                area[b] += line;
                area_r[b] += line_r;
                area_g[b] += line_g;
                area_b[b] += line_b;
                area_2[b] += line_2;
                let previous = index(r - 1, g, b);
                moments.weights[at] = moments.weights[previous] + area[b];
                moments.red[at] = moments.red[previous] + area_r[b];
                moments.green[at] = moments.green[previous] + area_g[b];
                moments.blue[at] = moments.blue[previous] + area_b[b];
                moments.squares[at] = moments.squares[previous] + area_2[b];
            }
        }
    }

    let mut cubes = vec![Cube::default(); MAX_COLORS];
    cubes[0].r1 = INDEX_COUNT - 1;
    cubes[0].g1 = INDEX_COUNT - 1;
    cubes[0].b1 = INDEX_COUNT - 1;
    let mut volume_variance = vec![0.0; MAX_COLORS];
    let mut next = 0;
    let mut max_colors = max_colors;
    let mut i = 1;
    while i < max_colors {
        let (head, tail) = cubes.split_at_mut(i);
        if cut(&mut head[next], &mut tail[0], &moments) {
            volume_variance[next] = if head[next].vol > 1 {
                variance(&head[next], &moments)
            } else {
                0.0
            };
            volume_variance[i] = if tail[0].vol > 1 {
                variance(&tail[0], &moments)
            } else {
                0.0
            };
        } else {
            volume_variance[next] = 0.0;
            i -= 1;
        }
        next = 0;
        let mut temp = volume_variance[0];
        for (j, value) in volume_variance.iter().enumerate().take(i + 1).skip(1) {
            if *value > temp {
                temp = *value;
                next = j;
            }
        }
        if temp <= 0.0 {
            max_colors = i + 1;
            break;
        }
        i += 1;
    }

    let mut out = Vec::new();
    for cube in cubes.iter().take(max_colors) {
        let weight = volume(cube, &moments.weights);
        if weight > 0 {
            let r = (volume(cube, &moments.red) / weight) as i32;
            let g = (volume(cube, &moments.green) / weight) as i32;
            let b = (volume(cube, &moments.blue) / weight) as i32;
            out.push(argb_from_rgb(r, g, b));
        }
    }
    out
}

fn wsmeans(input: &[u32], starting: &[u32], max_colors: usize) -> BTreeMap<u32, u32> {
    if max_colors == 0 || input.is_empty() {
        return BTreeMap::new();
    }
    let mut counts: HashMap<u32, u32> = HashMap::new();
    let mut pixels = Vec::new();
    let mut points = Vec::new();
    for &pixel in input {
        match counts.get_mut(&pixel) {
            Some(count) => *count += 1,
            None => {
                pixels.push(pixel);
                points.push(lab_from_int(pixel));
                counts.insert(pixel, 1);
            }
        }
    }
    let mut cluster_count = max_colors.min(points.len());
    if !starting.is_empty() {
        cluster_count = cluster_count.min(starting.len());
    }
    let mut clusters: Vec<Lab> = starting.iter().map(|&argb| lab_from_int(argb)).collect();
    let mut random = Glibc::new(SEED);
    if starting.is_empty() && cluster_count > clusters.len() {
        let max = i32::MAX as f64;
        for _ in clusters.len()..cluster_count {
            let l = random.rand() as f64 / max * 100.0;
            let a = random.rand() as f64 / max * 200.0 - 100.0;
            let b = random.rand() as f64 / max * 200.0 - 100.0;
            clusters.push(Lab { l, a, b });
        }
    }
    let mut random = Glibc::new(SEED);
    let mut indices: Vec<usize> = (0..points.len())
        .map(|_| random.rand() as usize % cluster_count)
        .collect();
    let mut distances = vec![vec![(0.0f64, 0usize); cluster_count]; cluster_count];
    let mut sums = vec![0u32; MAX_COLORS];
    for iteration in 0..MAX_ITERATIONS {
        for i in 0..cluster_count {
            distances[i][i] = (0.0, i);
            for j in i + 1..cluster_count {
                let distance = clusters[i].delta_e(&clusters[j]);
                distances[j][i] = (distance, i);
                distances[i][j] = (distance, j);
            }
        }
        let mut moved = false;
        for (i, point) in points.iter().enumerate() {
            let previous_index = indices[i];
            let previous_distance = point.delta_e(&clusters[previous_index]);
            let mut minimum = previous_distance;
            let mut new_index: Option<usize> = None;
            for j in 0..cluster_count {
                if distances[previous_index][j].0 >= 4.0 * previous_distance {
                    continue;
                }
                let distance = point.delta_e(&clusters[j]);
                if distance < minimum {
                    minimum = distance;
                    new_index = Some(j);
                }
            }
            if let Some(new_index) = new_index {
                let change = ((minimum.sqrt() - previous_distance.sqrt()) as i32).abs() as f64;
                if change > MIN_DELTA_E {
                    moved = true;
                    indices[i] = new_index;
                }
            }
        }
        if !moved && iteration != 0 {
            break;
        }
        let mut a_sums = vec![0.0; MAX_COLORS];
        let mut b_sums = vec![0.0; MAX_COLORS];
        let mut c_sums = vec![0.0; MAX_COLORS];
        for sum in sums.iter_mut().take(cluster_count) {
            *sum = 0;
        }
        for (i, point) in points.iter().enumerate() {
            let cluster = indices[i];
            let count = counts[&pixels[i]];
            sums[cluster] += count;
            a_sums[cluster] += point.l * count as f64;
            b_sums[cluster] += point.a * count as f64;
            c_sums[cluster] += point.b * count as f64;
        }
        for i in 0..cluster_count {
            let count = sums[i];
            if count == 0 {
                clusters[i] = Lab::default();
                continue;
            }
            clusters[i] = Lab {
                l: a_sums[i] / count as f64,
                a: b_sums[i] / count as f64,
                b: c_sums[i] / count as f64,
            };
        }
    }
    let mut swatches: Vec<(u32, u32)> = Vec::new();
    for i in 0..cluster_count {
        let argb = int_from_lab(clusters[i]);
        let count = sums[i];
        if count == 0 {
            continue;
        }
        match swatches.iter_mut().find(|(known, _)| *known == argb) {
            Some((_, population)) => *population += count,
            None => swatches.push((argb, count)),
        }
    }
    swatches.into_iter().collect()
}

pub fn score(colours: &BTreeMap<u32, u32>) -> Vec<u32> {
    const TARGET_CHROMA: f64 = 48.0;
    const WEIGHT_PROPORTION: f64 = 0.7;
    const WEIGHT_CHROMA_ABOVE: f64 = 0.3;
    const WEIGHT_CHROMA_BELOW: f64 = 0.1;
    const CUTOFF_CHROMA: f64 = 5.0;
    const CUTOFF_EXCITED_PROPORTION: f64 = 0.01;
    const DESIRED: usize = 4;
    const FALLBACK: u32 = 0xFF4285F4;
    let sanitize = |degrees: i64| degrees.rem_euclid(360) as usize;
    let mut hcts = Vec::new();
    let mut hue_population = [0u64; 360];
    let mut population_sum = 0u64;
    for (&argb, &population) in colours {
        let hct = Hct::new(Argb::from_u32(argb));
        hue_population[hct.get_hue() as usize] += population as u64;
        population_sum += population as u64;
        hcts.push(hct);
    }
    let mut excited = [0.0f64; 360];
    for hue in 0..360i64 {
        let proportion = hue_population[hue as usize] as f64 / population_sum as f64;
        for neighbour in hue - 14..hue + 16 {
            excited[sanitize(neighbour)] += proportion;
        }
    }
    let mut scored: Vec<(Hct, f64)> = Vec::new();
    for hct in hcts {
        let proportion = excited[sanitize(hct.get_hue().round_ties_even() as i64)];
        if hct.get_chroma() < CUTOFF_CHROMA || proportion <= CUTOFF_EXCITED_PROPORTION {
            continue;
        }
        let weight = if hct.get_chroma() < TARGET_CHROMA {
            WEIGHT_CHROMA_BELOW
        } else {
            WEIGHT_CHROMA_ABOVE
        };
        let score =
            proportion * 100.0 * WEIGHT_PROPORTION + (hct.get_chroma() - TARGET_CHROMA) * weight;
        scored.push((hct, score));
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let difference = |a: f64, b: f64| 180.0 - ((a - b).abs() - 180.0).abs();
    let mut chosen: Vec<Hct> = Vec::new();
    for degrees in (15..=90).rev() {
        chosen.clear();
        for (hct, _) in &scored {
            if !chosen
                .iter()
                .any(|other| difference(hct.get_hue(), other.get_hue()) < degrees as f64)
            {
                chosen.push(*hct);
            }
            if chosen.len() >= DESIRED {
                break;
            }
        }
        if chosen.len() >= DESIRED {
            break;
        }
    }
    if chosen.is_empty() {
        return vec![FALLBACK];
    }
    chosen
        .into_iter()
        .map(|hct| {
            let argb: Argb = hct.into();
            argb_from_rgb(argb.red as i32, argb.green as i32, argb.blue as i32)
        })
        .collect()
}
