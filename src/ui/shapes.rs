use gtk4::cairo;
use std::f64::consts::PI;

const DISTANCE_EPSILON: f64 = 1e-4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    pub fn distance(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    fn dot(self, other: Point) -> f64 {
        self.x * other.x + self.y * other.y
    }

    fn cross(self, other: Point) -> f64 {
        self.x * other.y - self.y * other.x
    }

    fn direction(self) -> Point {
        self.div(self.distance())
    }

    pub fn minus(self, other: Point) -> Point {
        Point::new(self.x - other.x, self.y - other.y)
    }

    fn plus(self, other: Point) -> Point {
        Point::new(self.x + other.x, self.y + other.y)
    }

    fn times(self, operand: f64) -> Point {
        Point::new(self.x * operand, self.y * operand)
    }

    fn div(self, operand: f64) -> Point {
        Point::new(self.x / operand, self.y / operand)
    }

    fn rotate90(self) -> Point {
        Point::new(-self.y, self.x)
    }

    fn interpolate(start: Point, stop: Point, fraction: f64) -> Point {
        Point::new(
            start.x + (stop.x - start.x) * fraction,
            start.y + (stop.y - start.y) * fraction,
        )
    }

    fn rotate_degrees(self, angle: f64, center: Point) -> Point {
        let radians = angle * PI / 180.0;
        let offset = self.minus(center);
        let (sin, cos) = radians.sin_cos();
        Point::new(
            offset.x * cos - offset.y * sin,
            offset.x * sin + offset.y * cos,
        )
        .plus(center)
    }
}

fn interpolate(start: f64, stop: f64, fraction: f64) -> f64 {
    (1.0 - fraction) * start + fraction * stop
}

fn direction_vector(x: f64, y: f64) -> Point {
    let distance = (x * x + y * y).sqrt();
    Point::new(x / distance, y / distance)
}

fn radial_to_cartesian(radius: f64, angle: f64) -> Point {
    Point::new(angle.cos(), angle.sin()).times(radius)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cubic(pub [f64; 8]);

impl Cubic {
    fn create(anchor0: Point, control0: Point, control1: Point, anchor1: Point) -> Self {
        Cubic([
            anchor0.x, anchor0.y, control0.x, control0.y, control1.x, control1.y, anchor1.x,
            anchor1.y,
        ])
    }

    pub fn point_on_curve(&self, t: f64) -> Point {
        let p = &self.0;
        let u = 1.0 - t;
        Point::new(
            p[0] * (u * u * u)
                + p[2] * (3.0 * t * u * u)
                + p[4] * (3.0 * t * t * u)
                + p[6] * (t * t * t),
            p[1] * (u * u * u)
                + p[3] * (3.0 * t * u * u)
                + p[5] * (3.0 * t * t * u)
                + p[7] * (t * t * t),
        )
    }

    fn zero_length(&self) -> bool {
        (self.0[0] - self.0[6]).abs() < DISTANCE_EPSILON
            && (self.0[1] - self.0[7]).abs() < DISTANCE_EPSILON
    }

    fn approximate_bounds(&self) -> [f64; 4] {
        let p = &self.0;
        if self.zero_length() {
            return [p[0], p[1], p[0], p[1]];
        }
        [
            p[0].min(p[6]).min(p[2].min(p[4])),
            p[1].min(p[7]).min(p[3].min(p[5])),
            p[0].max(p[6]).max(p[2].max(p[4])),
            p[1].max(p[7]).max(p[3].max(p[5])),
        ]
    }

    pub fn split(&self, t: f64) -> (Cubic, Cubic) {
        let p = &self.0;
        let u = 1.0 - t;
        let on = self.point_on_curve(t);
        (
            Cubic([
                p[0],
                p[1],
                p[0] * u + p[2] * t,
                p[1] * u + p[3] * t,
                p[0] * (u * u) + p[2] * (2.0 * u * t) + p[4] * (t * t),
                p[1] * (u * u) + p[3] * (2.0 * u * t) + p[5] * (t * t),
                on.x,
                on.y,
            ]),
            Cubic([
                on.x,
                on.y,
                p[2] * (u * u) + p[4] * (2.0 * u * t) + p[6] * (t * t),
                p[3] * (u * u) + p[5] * (2.0 * u * t) + p[7] * (t * t),
                p[4] * u + p[6] * t,
                p[5] * u + p[7] * t,
                p[6],
                p[7],
            ]),
        )
    }

    fn reverse(&self) -> Cubic {
        let p = &self.0;
        Cubic([p[6], p[7], p[4], p[5], p[2], p[3], p[0], p[1]])
    }

    fn transformed(&self, f: &dyn Fn(Point) -> Point) -> Cubic {
        let mut points = self.0;
        for index in [0, 2, 4, 6] {
            let moved = f(Point::new(points[index], points[index + 1]));
            points[index] = moved.x;
            points[index + 1] = moved.y;
        }
        Cubic(points)
    }

    fn straight_line(x0: f64, y0: f64, x1: f64, y1: f64) -> Cubic {
        Cubic([
            x0,
            y0,
            interpolate(x0, x1, 1.0 / 3.0),
            interpolate(y0, y1, 1.0 / 3.0),
            interpolate(x0, x1, 2.0 / 3.0),
            interpolate(y0, y1, 2.0 / 3.0),
            x1,
            y1,
        ])
    }

    fn circular_arc(center_x: f64, center_y: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> Cubic {
        let p0d = direction_vector(x0 - center_x, y0 - center_y);
        let p1d = direction_vector(x1 - center_x, y1 - center_y);
        let rotated0 = p0d.rotate90();
        let rotated1 = p1d.rotate90();
        let clockwise = rotated0.dot(Point::new(x1 - center_x, y1 - center_y)) >= 0.0;
        let cosa = p0d.dot(p1d);
        if cosa > 0.999 {
            return Cubic::straight_line(x0, y0, x1, y1);
        }
        let k = Point::new(x0 - center_x, y0 - center_y).distance() * 4.0 / 3.0
            * ((2.0 * (1.0 - cosa)).sqrt() - (1.0 - cosa * cosa).sqrt())
            / (1.0 - cosa)
            * if clockwise { 1.0 } else { -1.0 };
        Cubic([
            x0,
            y0,
            x0 + rotated0.x * k,
            y0 + rotated0.y * k,
            x1 - rotated1.x * k,
            y1 - rotated1.y * k,
            x1,
            y1,
        ])
    }

    pub fn anchor0(&self) -> Point {
        Point::new(self.0[0], self.0[1])
    }

    pub fn anchor1(&self) -> Point {
        Point::new(self.0[6], self.0[7])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rounding {
    pub radius: f64,
    pub smoothing: f64,
}

impl Rounding {
    pub const NONE: Rounding = Rounding {
        radius: 0.0,
        smoothing: 0.0,
    };

    pub const fn new(radius: f64) -> Self {
        Rounding {
            radius,
            smoothing: 0.0,
        }
    }

    pub const fn smooth(radius: f64, smoothing: f64) -> Self {
        Rounding { radius, smoothing }
    }
}

struct RoundedCorner {
    p0: Point,
    p1: Point,
    p2: Point,
    d1: Point,
    d2: Point,
    corner_radius: f64,
    smoothing: f64,
    expected_round_cut: f64,
}

impl RoundedCorner {
    fn new(p0: Point, p1: Point, p2: Point, rounding: Rounding) -> Self {
        let v01 = p0.minus(p1);
        let v21 = p2.minus(p1);
        let (d01, d21) = (v01.distance(), v21.distance());
        if d01 > 0.0 && d21 > 0.0 {
            let d1 = v01.div(d01);
            let d2 = v21.div(d21);
            let cos_angle = d1.dot(d2);
            let sin_angle = (1.0 - cos_angle.powi(2)).sqrt();
            let expected_round_cut = if sin_angle > 1e-3 {
                rounding.radius * (cos_angle + 1.0) / sin_angle
            } else {
                0.0
            };
            RoundedCorner {
                p0,
                p1,
                p2,
                d1,
                d2,
                corner_radius: rounding.radius,
                smoothing: rounding.smoothing,
                expected_round_cut,
            }
        } else {
            RoundedCorner {
                p0,
                p1,
                p2,
                d1: Point::new(0.0, 0.0),
                d2: Point::new(0.0, 0.0),
                corner_radius: 0.0,
                smoothing: 0.0,
                expected_round_cut: 0.0,
            }
        }
    }

    fn expected_cut(&self) -> f64 {
        (1.0 + self.smoothing) * self.expected_round_cut
    }

    fn cubics(&self, allowed_cut0: f64, allowed_cut1: f64) -> Vec<Cubic> {
        let allowed_cut = allowed_cut0.min(allowed_cut1);
        if self.expected_round_cut < DISTANCE_EPSILON
            || allowed_cut < DISTANCE_EPSILON
            || self.corner_radius < DISTANCE_EPSILON
        {
            return vec![Cubic::straight_line(
                self.p1.x, self.p1.y, self.p1.x, self.p1.y,
            )];
        }
        let actual_round_cut = allowed_cut.min(self.expected_round_cut);
        let smoothing0 = self.actual_smoothing(allowed_cut0);
        let smoothing1 = self.actual_smoothing(allowed_cut1);
        let actual_radius = self.corner_radius * actual_round_cut / self.expected_round_cut;
        let center_distance = (actual_radius.powi(2) + actual_round_cut.powi(2)).sqrt();
        let center = self.p1.plus(
            self.d1
                .plus(self.d2)
                .div(2.0)
                .direction()
                .times(center_distance),
        );
        let intersection0 = self.p1.plus(self.d1.times(actual_round_cut));
        let intersection2 = self.p1.plus(self.d2.times(actual_round_cut));
        let flanking0 = self.flanking(
            actual_round_cut,
            smoothing0,
            self.p0,
            intersection0,
            intersection2,
            center,
            actual_radius,
        );
        let flanking2 = self
            .flanking(
                actual_round_cut,
                smoothing1,
                self.p2,
                intersection2,
                intersection0,
                center,
                actual_radius,
            )
            .reverse();
        vec![
            flanking0,
            Cubic::circular_arc(
                center.x,
                center.y,
                flanking0.0[6],
                flanking0.0[7],
                flanking2.0[0],
                flanking2.0[1],
            ),
            flanking2,
        ]
    }

    fn actual_smoothing(&self, allowed_cut: f64) -> f64 {
        if allowed_cut > self.expected_cut() {
            self.smoothing
        } else if allowed_cut > self.expected_round_cut {
            self.smoothing * (allowed_cut - self.expected_round_cut)
                / (self.expected_cut() - self.expected_round_cut)
        } else {
            0.0
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn flanking(
        &self,
        actual_round_cut: f64,
        smoothing: f64,
        side_start: Point,
        intersection: Point,
        other_intersection: Point,
        center: Point,
        radius: f64,
    ) -> Cubic {
        let corner = self.p1;
        let side_direction = side_start.minus(corner).direction();
        let curve_start = corner.plus(side_direction.times(actual_round_cut * (1.0 + smoothing)));
        let p = Point::interpolate(
            intersection,
            intersection.plus(other_intersection).div(2.0),
            smoothing,
        );
        let curve_end = center.plus(direction_vector(p.x - center.x, p.y - center.y).times(radius));
        let tangent = curve_end.minus(center).rotate90();
        let anchor_end = line_intersection(side_start, side_direction, curve_end, tangent)
            .unwrap_or(intersection);
        let anchor_start = curve_start.plus(anchor_end.times(2.0)).div(3.0);
        Cubic::create(curve_start, anchor_start, anchor_end, curve_end)
    }
}

fn line_intersection(p0: Point, d0: Point, p1: Point, d1: Point) -> Option<Point> {
    let rotated = d1.rotate90();
    let den = d0.dot(rotated);
    if den.abs() < DISTANCE_EPSILON {
        return None;
    }
    let num = p1.minus(p0).dot(rotated);
    if den.abs() < DISTANCE_EPSILON * num.abs() {
        return None;
    }
    Some(p0.plus(d0.times(num / den)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FeatureKind {
    Edge,
    Corner { convex: bool },
}

#[derive(Clone, Debug)]
pub struct Feature {
    pub cubics: Vec<Cubic>,
    pub kind: FeatureKind,
}

#[derive(Clone, Debug)]
pub struct Polygon {
    pub features: Vec<Feature>,
    center: Point,
    pub cubics: Vec<Cubic>,
}

impl Polygon {
    fn new(features: Vec<Feature>, center: Point) -> Self {
        let cubics = build_cubics(&features, center);
        Polygon {
            features,
            center,
            cubics,
        }
    }

    pub fn transformed(&self, f: &dyn Fn(Point) -> Point) -> Polygon {
        Polygon::new(
            self.features
                .iter()
                .map(|feature| Feature {
                    cubics: feature
                        .cubics
                        .iter()
                        .map(|cubic| cubic.transformed(f))
                        .collect(),
                    kind: feature.kind,
                })
                .collect(),
            f(self.center),
        )
    }

    pub fn bounds(&self) -> [f64; 4] {
        const SAFE: f64 = 9007199254740991.0;
        let mut result = [SAFE, SAFE, -SAFE, -SAFE];
        for cubic in &self.cubics {
            let bounds = cubic.approximate_bounds();
            result[0] = result[0].min(bounds[0]);
            result[1] = result[1].min(bounds[1]);
            result[2] = result[2].max(bounds[2]);
            result[3] = result[3].max(bounds[3]);
        }
        result
    }

    pub fn normalized(&self) -> Polygon {
        let bounds = self.bounds();
        let width = bounds[2] - bounds[0];
        let height = bounds[3] - bounds[1];
        let side = width.max(height);
        let offset_x = (side - width) / 2.0 - bounds[0];
        let offset_y = (side - height) / 2.0 - bounds[1];
        self.transformed(&|point| {
            Point::new((point.x + offset_x) / side, (point.y + offset_y) / side)
        })
    }

    fn from_vertices(
        vertices: &[f64],
        rounding: Rounding,
        per_vertex: Option<&[Rounding]>,
        center: Option<Point>,
    ) -> Polygon {
        let n = vertices.len() / 2;
        let vertex = |index: usize| Point::new(vertices[index * 2], vertices[index * 2 + 1]);
        let corners: Vec<RoundedCorner> = (0..n)
            .map(|index| {
                let own = per_vertex
                    .and_then(|list| list.get(index).copied())
                    .unwrap_or(rounding);
                RoundedCorner::new(
                    vertex((index + n - 1) % n),
                    vertex(index),
                    vertex((index + 1) % n),
                    own,
                )
            })
            .collect();

        let adjusts: Vec<(f64, f64)> = (0..n)
            .map(|index| {
                let next = (index + 1) % n;
                let expected_round_cut =
                    corners[index].expected_round_cut + corners[next].expected_round_cut;
                let expected_cut = corners[index].expected_cut() + corners[next].expected_cut();
                let side = vertex(index).minus(vertex(next)).distance();
                if expected_round_cut > side {
                    (side / expected_round_cut, 0.0)
                } else if expected_cut > side {
                    (
                        1.0,
                        (side - expected_round_cut) / (expected_cut - expected_round_cut),
                    )
                } else {
                    (1.0, 1.0)
                }
            })
            .collect();

        let cubics: Vec<Vec<Cubic>> = (0..n)
            .map(|index| {
                let allowed: Vec<f64> = [0, 1]
                    .iter()
                    .map(|delta| {
                        let (round_ratio, cut_ratio) = adjusts[(index + n - 1 + delta) % n];
                        let corner = &corners[index];
                        corner.expected_round_cut * round_ratio
                            + (corner.expected_cut() - corner.expected_round_cut) * cut_ratio
                    })
                    .collect();
                corners[index].cubics(allowed[0], allowed[1])
            })
            .collect();

        let mut features = Vec::new();
        for index in 0..n {
            let next = (index + 1) % n;
            let previous = vertex((index + n - 1) % n);
            let turn = vertex(index)
                .minus(previous)
                .cross(vertex(next).minus(vertex(index)));
            features.push(Feature {
                cubics: cubics[index].clone(),
                kind: FeatureKind::Corner { convex: turn > 0.0 },
            });
            let end = cubics[index][cubics[index].len() - 1].anchor1();
            let start = cubics[next][0].anchor0();
            features.push(Feature {
                cubics: vec![Cubic::straight_line(end.x, end.y, start.x, start.y)],
                kind: FeatureKind::Edge,
            });
        }

        let center = center.unwrap_or_else(|| {
            let (mut x, mut y) = (0.0, 0.0);
            for index in 0..n {
                x += vertices[index * 2];
                y += vertices[index * 2 + 1];
            }
            Point::new(x / n as f64, y / n as f64)
        });
        Polygon::new(features, center)
    }

    fn from_count(count: usize, radius: f64, center: Point, rounding: Rounding) -> Polygon {
        let mut vertices = Vec::new();
        for index in 0..count {
            let vertex =
                radial_to_cartesian(radius, PI / count as f64 * 2.0 * index as f64).plus(center);
            vertices.push(vertex.x);
            vertices.push(vertex.y);
        }
        Polygon::from_vertices(&vertices, rounding, None, Some(center))
    }

    fn circle(count: usize) -> Polygon {
        let theta = PI / count as f64;
        Polygon::from_count(
            count,
            1.0 / theta.cos(),
            Point::new(0.0, 0.0),
            Rounding::new(1.0),
        )
    }

    fn star(count: usize, radius: f64, inner: f64, rounding: Rounding) -> Polygon {
        let mut vertices = Vec::new();
        for index in 0..count {
            let outer = radial_to_cartesian(radius, PI / count as f64 * 2.0 * index as f64);
            vertices.push(outer.x);
            vertices.push(outer.y);
            let inside = radial_to_cartesian(inner, PI / count as f64 * (2.0 * index as f64 + 1.0));
            vertices.push(inside.x);
            vertices.push(inside.y);
        }
        Polygon::from_vertices(&vertices, rounding, None, Some(Point::new(0.0, 0.0)))
    }

    pub fn trace(&self, cr: &cairo::Context, x: f64, y: f64, size: f64) {
        trace(&self.cubics, cr, x, y, size);
    }
}

pub fn trace(cubics: &[Cubic], cr: &cairo::Context, x: f64, y: f64, size: f64) {
    let Some(first) = cubics.first() else {
        return;
    };
    cr.new_path();
    cr.move_to(x + first.0[0] * size, y + first.0[1] * size);
    for cubic in cubics {
        let p = &cubic.0;
        cr.curve_to(
            x + p[2] * size,
            y + p[3] * size,
            x + p[4] * size,
            y + p[5] * size,
            x + p[6] * size,
            y + p[7] * size,
        );
    }
    cr.close_path();
}

fn build_cubics(features: &[Feature], center: Point) -> Vec<Cubic> {
    let mut result = Vec::new();
    let mut first: Option<Cubic> = None;
    let mut last: Option<Cubic> = None;
    let mut split_start: Option<Vec<Cubic>> = None;
    let mut split_end: Option<Vec<Cubic>> = None;
    if let Some(feature) = features.first()
        && feature.cubics.len() == 3
    {
        let (start, end) = feature.cubics[1].split(0.5);
        split_start = Some(vec![feature.cubics[0], start]);
        split_end = Some(vec![end, feature.cubics[2]]);
    }
    for index in 0..=features.len() {
        let cubics: &[Cubic] = if index == 0 && split_end.is_some() {
            split_end.as_deref().unwrap_or_default()
        } else if index == features.len() {
            match split_start.as_deref() {
                Some(cubics) => cubics,
                None => break,
            }
        } else {
            &features[index].cubics
        };
        for cubic in cubics {
            if !cubic.zero_length() {
                if let Some(previous) = last {
                    result.push(previous);
                }
                last = Some(*cubic);
                if first.is_none() {
                    first = Some(*cubic);
                }
            } else if let Some(previous) = last.as_mut() {
                previous.0[6] = cubic.0[6];
                previous.0[7] = cubic.0[7];
            }
        }
    }
    match (last, first) {
        (Some(last), Some(first)) => {
            let p = last.0;
            result.push(Cubic([
                p[0], p[1], p[2], p[3], p[4], p[5], first.0[0], first.0[1],
            ]));
        }
        _ => result.push(Cubic([
            center.x, center.y, center.x, center.y, center.x, center.y, center.x, center.y,
        ])),
    }
    result
}

fn rotation(degrees: f64) -> impl Fn(Point) -> Point {
    let radians = degrees * PI / 180.0;
    let (sin, cos) = radians.sin_cos();
    move |point| Point::new(cos * point.x - sin * point.y, sin * point.x + cos * point.y)
}

fn scaling(x: f64, y: f64) -> impl Fn(Point) -> Point {
    move |point| Point::new(point.x * x, point.y * y)
}

struct Vertex(Point, Rounding);

fn at(x: f64, y: f64) -> Vertex {
    Vertex(Point::new(x, y), Rounding::NONE)
}

fn rounded(x: f64, y: f64, radius: f64) -> Vertex {
    Vertex(Point::new(x, y), Rounding::new(radius))
}

fn smoothed(x: f64, y: f64, radius: f64, smoothing: f64) -> Vertex {
    Vertex(Point::new(x, y), Rounding::smooth(radius, smoothing))
}

fn custom(points: &[Vertex], repeats: usize) -> Polygon {
    let center = Point::new(0.5, 0.5);
    let count = points.len();
    let mut vertices = Vec::new();
    let mut roundings = Vec::new();
    for index in 0..count * repeats {
        let source = &points[index % count];
        let point = source
            .0
            .rotate_degrees((index / count) as f64 * 360.0 / repeats as f64, center);
        vertices.push(point.x);
        vertices.push(point.y);
        roundings.push(source.1);
    }
    Polygon::from_vertices(&vertices, Rounding::NONE, Some(&roundings), Some(center))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    Circle,
    Arrow,
    Oval,
    Pill,
    Diamond,
    ClamShell,
    Pentagon,
    Gem,
    Sunny,
    VerySunny,
    Cookie4Sided,
    Cookie6Sided,
    Cookie7Sided,
    Cookie9Sided,
    Cookie12Sided,
    Ghostish,
    Clover4Leaf,
    SoftBurst,
    PuffyDiamond,
    PixelCircle,
}

thread_local! {
    static CACHE: std::cell::RefCell<std::collections::HashMap<Shape, std::rc::Rc<Polygon>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static COOKIES: std::cell::RefCell<std::collections::HashMap<i64, std::rc::Rc<Polygon>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

pub fn cookie(sides: i64) -> std::rc::Rc<Polygon> {
    match sides {
        0 | 1 => return polygon(Shape::Circle),
        4 => return polygon(Shape::Cookie4Sided),
        6 => return polygon(Shape::Cookie6Sided),
        7 => return polygon(Shape::Cookie7Sided),
        9 => return polygon(Shape::Cookie9Sided),
        12 => return polygon(Shape::Cookie12Sided),
        _ => {}
    }
    COOKIES.with(|cache| {
        cache
            .borrow_mut()
            .entry(sides)
            .or_insert_with(|| {
                let count = sides.max(1);
                let radius = if count < 17 { 1.5 } else { 1.1 } / count as f64;
                std::rc::Rc::new(
                    Polygon::star(count as usize, 1.0, 0.8, Rounding::new(radius))
                        .transformed(&rotation(30.0))
                        .normalized(),
                )
            })
            .clone()
    })
}

pub fn polygon(shape: Shape) -> std::rc::Rc<Polygon> {
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry(shape)
            .or_insert_with(|| std::rc::Rc::new(build(shape)))
            .clone()
    })
}

fn build(shape: Shape) -> Polygon {
    let r15 = Rounding::new(0.15);
    let r50 = Rounding::new(0.5);
    match shape {
        Shape::Circle => Polygon::circle(10)
            .transformed(&rotation(45.0))
            .normalized(),
        Shape::Arrow => custom(
            &[
                rounded(1.225, 1.060, 0.211),
                rounded(0.500, 0.892, 0.313),
                rounded(-0.216, 1.050, 0.207),
                smoothed(0.499, -0.160, 0.215, 1.000),
            ],
            1,
        )
        .normalized(),
        Shape::Oval => Polygon::circle(8)
            .transformed(&rotation(-90.0))
            .transformed(&scaling(1.0, 0.64))
            .transformed(&rotation(135.0))
            .normalized(),
        Shape::Pill => custom(
            &[
                rounded(0.428, -0.001, 0.426),
                rounded(0.961, 0.039, 0.426),
                at(1.001, 0.428),
                rounded(1.000, 0.609, 1.000),
            ],
            2,
        )
        .transformed(&rotation(180.0))
        .normalized(),
        Shape::Diamond => custom(
            &[
                smoothed(0.500, 1.096, 0.151, 0.524),
                rounded(0.040, 0.500, 0.159),
            ],
            2,
        )
        .normalized(),
        Shape::ClamShell => custom(
            &[
                rounded(0.829, 0.841, 0.159),
                rounded(0.171, 0.841, 0.159),
                rounded(-0.020, 0.500, 0.140),
            ],
            2,
        )
        .normalized(),
        Shape::Pentagon => custom(
            &[
                rounded(0.828, 0.970, 0.169),
                rounded(0.172, 0.970, 0.169),
                rounded(-0.030, 0.365, 0.164),
                rounded(0.500, -0.009, 0.172),
                rounded(1.030, 0.365, 0.164),
            ],
            1,
        )
        .normalized(),
        Shape::Gem => custom(
            &[
                rounded(1.005, 0.792, 0.208),
                smoothed(0.5, 1.023, 0.241, 0.778),
                rounded(-0.005, 0.792, 0.208),
                rounded(0.073, 0.258, 0.228),
                smoothed(0.5, 0.000, 0.241, 0.778),
                rounded(0.927, 0.258, 0.228),
            ],
            1,
        )
        .normalized(),
        Shape::Sunny => Polygon::star(8, 1.0, 0.8, r15)
            .transformed(&rotation(45.0))
            .normalized(),
        Shape::VerySunny => custom(
            &[rounded(0.500, 1.080, 0.085), rounded(0.358, 0.843, 0.085)],
            8,
        )
        .transformed(&rotation(-45.0))
        .normalized(),
        Shape::Cookie4Sided => custom(
            &[rounded(1.237, 1.236, 0.258), rounded(0.500, 0.918, 0.233)],
            4,
        )
        .normalized(),
        Shape::Cookie6Sided => custom(
            &[rounded(0.723, 0.884, 0.394), rounded(0.500, 1.099, 0.398)],
            6,
        )
        .normalized(),
        Shape::Cookie7Sided => {
            let mut polygon = Polygon::star(7, 1.0, 0.75, r50).normalized();
            for _ in 0..5 {
                polygon = polygon.transformed(&rotation(360.0 / 28.0));
            }
            polygon.normalized()
        }
        Shape::Cookie9Sided => Polygon::star(9, 1.0, 0.8, r50)
            .transformed(&rotation(30.0))
            .normalized(),
        Shape::Cookie12Sided => Polygon::star(12, 1.0, 0.8, r50)
            .transformed(&rotation(30.0))
            .normalized(),
        Shape::Ghostish => custom(
            &[
                smoothed(1.000, 1.140, 0.254, 0.106),
                rounded(0.575, 0.906, 0.253),
                rounded(0.425, 0.906, 0.253),
                smoothed(0.000, 1.140, 0.254, 0.106),
                rounded(0.000, 0.000, 1.0),
                rounded(0.500, 0.000, 1.0),
                rounded(1.000, 0.000, 1.0),
            ],
            1,
        )
        .normalized(),
        Shape::Clover4Leaf => custom(
            &[
                rounded(1.099, 0.725, 0.476),
                rounded(0.725, 1.099, 0.476),
                at(0.500, 0.926),
            ],
            4,
        )
        .normalized(),
        Shape::SoftBurst => custom(
            &[rounded(0.193, 0.277, 0.053), rounded(0.176, 0.055, 0.053)],
            10,
        )
        .transformed(&rotation(180.0))
        .normalized(),
        Shape::PuffyDiamond => custom(
            &[
                rounded(0.870, 0.130, 0.146),
                at(0.818, 0.357),
                rounded(1.000, 0.332, 0.853),
                rounded(1.000, 1.0 - 0.332, 0.853),
                at(0.818, 1.0 - 0.357),
            ],
            4,
        )
        .transformed(&rotation(90.0))
        .normalized(),
        Shape::PixelCircle => custom(
            &[
                at(1.000, 0.704),
                at(0.926, 0.704),
                at(0.926, 0.852),
                at(0.843, 0.852),
                at(0.843, 0.935),
                at(0.704, 0.935),
                at(0.704, 1.000),
                at(0.500, 1.000),
                at(1.0 - 0.704, 1.000),
                at(1.0 - 0.704, 0.935),
                at(1.0 - 0.843, 0.935),
                at(1.0 - 0.843, 0.852),
                at(1.0 - 0.926, 0.852),
                at(1.0 - 0.926, 0.704),
                at(0.000, 0.704),
            ],
            2,
        )
        .normalized(),
    }
}
