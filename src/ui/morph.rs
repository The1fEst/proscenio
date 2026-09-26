use crate::ui::shapes::{Cubic, FeatureKind, Point, Polygon};

const DISTANCE_EPSILON: f64 = 1e-4;
const ANGLE_EPSILON: f64 = 1e-6;
const MEASURE_SEGMENTS: usize = 3;

pub struct Morph {
    pairs: Vec<(Cubic, Cubic)>,
}

impl Morph {
    pub fn new(start: &Polygon, end: &Polygon) -> Self {
        Morph {
            pairs: matched(start, end),
        }
    }

    pub fn cubics(&self, progress: f64) -> Vec<Cubic> {
        let mut result = Vec::with_capacity(self.pairs.len());
        let mut first: Option<Cubic> = None;
        let mut last: Option<Cubic> = None;
        for (from, to) in &self.pairs {
            let mut points = [0.0; 8];
            for (index, point) in points.iter_mut().enumerate() {
                *point = from.0[index] + (to.0[index] - from.0[index]) * progress;
            }
            let cubic = Cubic(points);
            if first.is_none() {
                first = Some(cubic);
            }
            if let Some(previous) = last {
                result.push(previous);
            }
            last = Some(cubic);
        }
        if let (Some(last), Some(first)) = (last, first) {
            let p = last.0;
            result.push(Cubic([
                p[0], p[1], p[2], p[3], p[4], p[5], first.0[0], first.0[1],
            ]));
        }
        result
    }
}

fn matched(start: &Polygon, end: &Polygon) -> Vec<(Cubic, Cubic)> {
    let measured1 = Measured::of(start);
    let measured2 = Measured::of(end);
    let mapper = feature_mapper(&measured1.features, &measured2.features);
    let cut_point = mapper.map(0.0);
    let bs1 = measured1;
    let bs2 = measured2.cut_and_shift(cut_point);

    let mut result = Vec::new();
    let (mut i1, mut i2) = (0, 0);
    let mut b1 = bs1.cubics.get(i1).cloned();
    i1 += 1;
    let mut b2 = bs2.cubics.get(i2).cloned();
    i2 += 1;
    while let (Some(c1), Some(c2)) = (b1.clone(), b2.clone()) {
        let b1a = if i1 == bs1.cubics.len() { 1.0 } else { c1.end };
        let b2a = if i2 == bs2.cubics.len() {
            1.0
        } else {
            mapper.map_back(positive_modulo(c2.end + cut_point, 1.0))
        };
        let smallest = b1a.min(b2a);
        let (segment1, next1) = if b1a > smallest + ANGLE_EPSILON {
            let (before, after) = c1.cut_at(smallest);
            (before, Some(after))
        } else {
            let next = bs1.cubics.get(i1).cloned();
            i1 += 1;
            (c1, next)
        };
        let (segment2, next2) = if b2a > smallest + ANGLE_EPSILON {
            let (before, after) = c2.cut_at(positive_modulo(mapper.map(smallest) - cut_point, 1.0));
            (before, Some(after))
        } else {
            let next = bs2.cubics.get(i2).cloned();
            i2 += 1;
            (c2, next)
        };
        result.push((segment1.cubic, segment2.cubic));
        b1 = next1;
        b2 = next2;
    }
    result
}

#[derive(Clone, Copy)]
struct Progressable {
    progress: f64,
    convex: bool,
    point: Point,
    index: usize,
}

#[derive(Clone)]
struct MeasuredCubic {
    cubic: Cubic,
    start: f64,
    end: f64,
    size: f64,
}

impl MeasuredCubic {
    fn new(cubic: Cubic, start: f64, end: f64) -> Self {
        MeasuredCubic {
            cubic,
            start,
            end,
            size: measure(&cubic),
        }
    }

    fn cut_at(&self, cut: f64) -> (MeasuredCubic, MeasuredCubic) {
        let bounded = cut.clamp(self.start.min(self.end), self.start.max(self.end));
        let span = self.end - self.start;
        let relative = (bounded - self.start) / span;
        let t = cut_point(&self.cubic, relative * self.size);
        let (first, second) = self.cubic.split(t);
        (
            MeasuredCubic::new(first, self.start, bounded),
            MeasuredCubic::new(second, bounded, self.end),
        )
    }
}

struct Measured {
    features: Vec<Progressable>,
    cubics: Vec<MeasuredCubic>,
}

impl Measured {
    fn new(features: Vec<Progressable>, cubics: &[Cubic], outline: &[f64]) -> Self {
        let mut measured = Vec::new();
        let mut start = 0.0;
        for (index, cubic) in cubics.iter().enumerate() {
            if outline[index + 1] - outline[index] > DISTANCE_EPSILON {
                measured.push(MeasuredCubic::new(*cubic, start, outline[index + 1]));
                start = outline[index + 1];
            }
        }
        if let Some(last) = measured.last_mut() {
            last.end = 1.0;
        }
        Measured {
            features,
            cubics: measured,
        }
    }

    fn of(polygon: &Polygon) -> Self {
        let mut cubics = Vec::new();
        let mut corners = Vec::new();
        for feature in &polygon.features {
            for (index, cubic) in feature.cubics.iter().enumerate() {
                if let FeatureKind::Corner { convex } = feature.kind
                    && index == feature.cubics.len() / 2
                {
                    let first = feature.cubics[0].anchor0();
                    let last = feature.cubics[feature.cubics.len() - 1].anchor1();
                    let point = Point::new((first.x + last.x) / 2.0, (first.y + last.y) / 2.0);
                    corners.push((cubics.len(), convex, point));
                }
                cubics.push(*cubic);
            }
        }
        let mut measures = vec![0.0];
        for cubic in &cubics {
            let last = measures[measures.len() - 1];
            measures.push(last + measure(cubic));
        }
        let total = measures[measures.len() - 1];
        let outline: Vec<f64> = measures.iter().map(|measure| measure / total).collect();
        let features = corners
            .into_iter()
            .enumerate()
            .map(|(index, (at, convex, point))| Progressable {
                progress: positive_modulo((outline[at] + outline[at + 1]) / 2.0, 1.0),
                convex,
                point,
                index,
            })
            .collect();
        Measured::new(features, &cubics, &outline)
    }

    fn cut_and_shift(&self, cut: f64) -> Measured {
        if cut < DISTANCE_EPSILON {
            return Measured {
                features: self.features.clone(),
                cubics: self.cubics.clone(),
            };
        }
        let Some(target) = self
            .cubics
            .iter()
            .position(|cubic| cut >= cubic.start && cut <= cubic.end)
        else {
            return Measured {
                features: self.features.clone(),
                cubics: self.cubics.clone(),
            };
        };
        let (before, after) = self.cubics[target].cut_at(cut);
        let count = self.cubics.len();
        let mut cubics = vec![after.cubic];
        for index in 1..count {
            cubics.push(self.cubics[(index + target) % count].cubic);
        }
        cubics.push(before.cubic);
        let mut outline = vec![0.0];
        for index in 1..=count {
            let cubic = &self.cubics[(target + index - 1) % count];
            outline.push(positive_modulo(cubic.end - cut, 1.0));
        }
        outline.push(1.0);
        let features = self
            .features
            .iter()
            .map(|feature| Progressable {
                progress: positive_modulo(feature.progress - cut, 1.0),
                ..*feature
            })
            .collect();
        Measured::new(features, &cubics, &outline)
    }
}

fn closest_progress(cubic: &Cubic, threshold: f64) -> (f64, f64) {
    let mut total = 0.0;
    let mut remainder = threshold;
    let mut previous = cubic.anchor0();
    for index in 1..MEASURE_SEGMENTS {
        let progress = index as f64 / MEASURE_SEGMENTS as f64;
        let point = cubic.point_on_curve(progress);
        let segment = point.minus(previous).distance();
        if segment >= remainder {
            return (
                progress - (1.0 - remainder / segment) / MEASURE_SEGMENTS as f64,
                threshold,
            );
        }
        remainder -= segment;
        total += segment;
        previous = point;
    }
    (1.0, total)
}

fn measure(cubic: &Cubic) -> f64 {
    closest_progress(cubic, f64::INFINITY).1
}

fn cut_point(cubic: &Cubic, length: f64) -> f64 {
    closest_progress(cubic, length).0
}

struct Mapper {
    source: Vec<f64>,
    target: Vec<f64>,
}

impl Mapper {
    fn new(pairs: &[(f64, f64)]) -> Self {
        Mapper {
            source: pairs.iter().map(|pair| pair.0).collect(),
            target: pairs.iter().map(|pair| pair.1).collect(),
        }
    }

    fn map(&self, x: f64) -> f64 {
        linear_map(&self.source, &self.target, x)
    }

    fn map_back(&self, x: f64) -> f64 {
        linear_map(&self.target, &self.source, x)
    }
}

fn linear_map(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let Some(start) =
        (0..xs.len()).find(|&index| in_range(x, xs[index], xs[(index + 1) % xs.len()]))
    else {
        return x;
    };
    let end = (start + 1) % xs.len();
    let span_x = positive_modulo(xs[end] - xs[start], 1.0);
    let span_y = positive_modulo(ys[end] - ys[start], 1.0);
    let position = if span_x < 0.001 {
        0.5
    } else {
        positive_modulo(x - xs[start], 1.0) / span_x
    };
    positive_modulo(ys[start] + span_y * position, 1.0)
}

fn in_range(progress: f64, from: f64, to: f64) -> bool {
    if to >= from {
        progress >= from && progress <= to
    } else {
        progress >= from || progress <= to
    }
}

fn feature_mapper(features1: &[Progressable], features2: &[Progressable]) -> Mapper {
    let mut distances = Vec::new();
    for f1 in features1 {
        for f2 in features2 {
            if f1.convex == f2.convex {
                let offset = f1.point.minus(f2.point);
                distances.push((offset.x * offset.x + offset.y * offset.y, *f1, *f2));
            }
        }
    }
    distances.sort_by(|a, b| a.0.total_cmp(&b.0));
    match distances.as_slice() {
        [] => Mapper::new(&[(0.0, 0.0), (0.5, 0.5)]),
        [(_, f1, f2)] => Mapper::new(&[
            (f1.progress, f2.progress),
            ((f1.progress + 0.5) % 1.0, (f2.progress + 0.5) % 1.0),
        ]),
        _ => {
            let mut mapping: Vec<(f64, f64)> = Vec::new();
            let mut used1 = Vec::new();
            let mut used2 = Vec::new();
            for (_, f1, f2) in &distances {
                if used1.contains(&f1.index) || used2.contains(&f2.index) {
                    continue;
                }
                let Err(insertion) =
                    mapping.binary_search_by(|pair| pair.0.total_cmp(&f1.progress))
                else {
                    continue;
                };
                let count = mapping.len();
                if count >= 1 {
                    let (before1, before2) = mapping[(insertion + count - 1) % count];
                    let (after1, after2) = mapping[insertion % count];
                    if progress_distance(f1.progress, before1) < DISTANCE_EPSILON
                        || progress_distance(f1.progress, after1) < DISTANCE_EPSILON
                        || progress_distance(f2.progress, before2) < DISTANCE_EPSILON
                        || progress_distance(f2.progress, after2) < DISTANCE_EPSILON
                    {
                        continue;
                    }
                    if count > 1 && !in_range(f2.progress, before2, after2) {
                        continue;
                    }
                }
                mapping.insert(insertion, (f1.progress, f2.progress));
                used1.push(f1.index);
                used2.push(f2.index);
            }
            Mapper::new(&mapping)
        }
    }
}

fn progress_distance(p1: f64, p2: f64) -> f64 {
    let it = (p1 - p2).abs();
    it.min(1.0 - it)
}

fn positive_modulo(value: f64, modulus: f64) -> f64 {
    ((value % modulus) + modulus) % modulus
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::shapes::{Shape, polygon};

    fn farthest_from(outline: &Polygon, cubics: &[Cubic]) -> f64 {
        let samples: Vec<Point> = outline
            .cubics
            .iter()
            .flat_map(|cubic| (0..=64).map(move |step| cubic.point_on_curve(step as f64 / 64.0)))
            .collect();
        cubics
            .iter()
            .map(|cubic| {
                let middle = cubic.point_on_curve(0.5);
                samples
                    .iter()
                    .map(|point| point.minus(middle).distance())
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max)
    }

    #[test]
    fn a_morph_starts_and_ends_on_its_shapes_outlines() {
        let start = polygon(Shape::Pentagon);
        let end = polygon(Shape::Cookie9Sided);
        let morph = Morph::new(&start, &end);
        let at_start = morph.cubics(0.0);
        let at_end = morph.cubics(1.0);
        assert!(farthest_from(&start, &at_start) < 0.01);
        assert!(farthest_from(&end, &at_end) < 0.01);
        assert_eq!(at_end.last().unwrap().anchor1(), at_end[0].anchor0());
    }

    #[test]
    fn features_map_around_the_circle_in_order() {
        let mapper = Mapper::new(&[(0.1, 0.2), (0.4, 0.5), (0.8, 0.9)]);
        assert!((mapper.map(0.1) - 0.2).abs() < 1e-9);
        assert!((mapper.map(0.25) - 0.35).abs() < 1e-9);
        assert!((mapper.map(0.95) - 0.05).abs() < 1e-9);
        assert!((mapper.map_back(0.05) - 0.95).abs() < 1e-9);
    }
}
