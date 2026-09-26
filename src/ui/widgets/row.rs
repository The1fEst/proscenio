use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;

use crate::ui::widgets::centred::Centred;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Row {
        pub spacing: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Row {
        const NAME: &'static str = "ProscenioRow";
        type Type = super::Row;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Row {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Row {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let children = self.visible_children();
            if orientation == gtk4::Orientation::Horizontal {
                let (minimums, naturals, _, expanding) = self.widths(&children);
                let gaps = super::gaps(children.len(), self.spacing.get());
                let minimum = (0..children.len())
                    .map(|index| match expanding[index] {
                        true => minimums[index].min(naturals[index]),
                        false => naturals[index],
                    })
                    .sum::<i32>()
                    + gaps;
                return (minimum, naturals.iter().sum::<i32>() + gaps, -1, -1);
            }
            let size = children
                .iter()
                .map(|child| child.measure(orientation, -1).1)
                .max()
                .unwrap_or(0);
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            present_popovers(&*self.obj());
            let children = self.visible_children();
            let (minimums, naturals, exact, expanding) = self.widths(&children);
            let spacing = self.spacing.get();
            let lefts = super::lefts(&minimums, &exact, &expanding, spacing, width);
            for (index, child) in children.iter().enumerate() {
                let x = lefts[index];
                let child_width = match (expanding[index], lefts.get(index + 1)) {
                    (false, _) => naturals[index],
                    (true, Some(next)) => next - spacing - x,
                    (true, None) => (width - x).max(minimums[index]),
                };
                let child_height = match child.valign() {
                    gtk4::Align::Fill if child.compute_expand(gtk4::Orientation::Vertical) => {
                        height
                    }
                    _ => child
                        .measure(gtk4::Orientation::Vertical, child_width)
                        .1
                        .min(height),
                };
                let y = match child.valign() {
                    gtk4::Align::Start => 0.0,
                    gtk4::Align::End => (height - child_height) as f32,
                    _ => ((height - child_height) as f32 / 2.0).round(),
                };
                let place =
                    gtk4::gsk::Transform::new().translate(&gtk4::graphene::Point::new(x as f32, y));
                child.allocate(child_width, child_height, -1, Some(place));
            }
        }
    }

    impl Row {
        fn widths(&self, children: &[gtk4::Widget]) -> (Vec<i32>, Vec<i32>, Vec<f64>, Vec<bool>) {
            let (minimums, naturals): (Vec<i32>, Vec<i32>) = children
                .iter()
                .map(|child| {
                    let (minimum, natural, _, _) = child.measure(gtk4::Orientation::Horizontal, -1);
                    (minimum, natural)
                })
                .unzip();
            let exact = children
                .iter()
                .zip(&naturals)
                .map(|(child, &natural)| {
                    child
                        .downcast_ref::<Centred>()
                        .and_then(Centred::exact_width)
                        .filter(|exact| exact.ceil() as i32 == natural)
                        .unwrap_or(natural as f64)
                })
                .collect();
            let expanding = children
                .iter()
                .map(|child| child.compute_expand(gtk4::Orientation::Horizontal))
                .collect();
            (minimums, naturals, exact, expanding)
        }

        fn visible_children(&self) -> Vec<gtk4::Widget> {
            let mut children = Vec::new();
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                if widget.get_visible() && !widget.is::<gtk4::Popover>() {
                    children.push(widget.clone());
                }
                child = widget.next_sibling();
            }
            children
        }
    }
}

fn gaps(count: usize, spacing: i32) -> i32 {
    spacing * (count as i32 - 1).max(0)
}

fn floors(minimums: &[i32], naturals: &[f64], expanding: &[bool]) -> Vec<f64> {
    (0..naturals.len())
        .map(|index| {
            if expanding[index] {
                (minimums[index] as f64).min(naturals[index])
            } else {
                naturals[index]
            }
        })
        .collect()
}

fn grown(naturals: &[f64], expanding: &[bool], extra: f64) -> Vec<f64> {
    let any_expanding = expanding.contains(&true);
    let factor = |index: usize| match (expanding[index] || !any_expanding, naturals[index]) {
        (false, _) => 0.0,
        (true, natural) if natural == 0.0 => 1.0,
        (true, natural) => natural,
    };
    let factors: f64 = (0..naturals.len()).map(factor).sum();
    (0..naturals.len())
        .map(|index| {
            let share = if factors > 0.0 {
                extra * factor(index) / factors
            } else {
                0.0
            };
            naturals[index] + share
        })
        .collect()
}

fn shrunk(floors: &[f64], naturals: &[f64], available: f64) -> Vec<f64> {
    let desired: Vec<f64> = (0..naturals.len())
        .map(|index| naturals[index] - floors[index])
        .collect();
    let sum_desired: f64 = desired.iter().sum();
    if available <= 0.0 || sum_desired <= 0.0 {
        return floors.to_vec();
    }
    let factors: Vec<f64> = desired
        .iter()
        .map(|&wanted| wanted * (available / sum_desired).powf(wanted / sum_desired))
        .collect();
    let sum_factors: f64 = factors.iter().sum();
    (0..naturals.len())
        .map(|index| floors[index] + available * factors[index] / sum_factors)
        .collect()
}

fn lefts(
    minimums: &[i32],
    naturals: &[f64],
    expanding: &[bool],
    spacing: i32,
    width: i32,
) -> Vec<i32> {
    let gaps = gaps(naturals.len(), spacing) as f64;
    let used = naturals.iter().sum::<f64>() + gaps;
    let width = width as f64;
    let sizes = if width >= used {
        grown(naturals, expanding, width - used)
    } else {
        let floors = floors(minimums, naturals, expanding);
        let available = width - floors.iter().sum::<f64>() - gaps;
        shrunk(&floors, naturals, available)
    };
    let mut left = 0.0;
    let mut lefts = Vec::with_capacity(naturals.len());
    for size in sizes {
        lefts.push((left + 0.5_f64).floor() as i32);
        left += size + spacing as f64;
    }
    lefts
}

pub fn present_popovers(widget: &impl IsA<gtk4::Widget>) {
    let mut child = widget.as_ref().first_child();
    while let Some(current) = child {
        if let Some(popover) = current.downcast_ref::<gtk4::Popover>() {
            popover.present();
        }
        child = current.next_sibling();
    }
}

glib::wrapper! {
    pub struct Row(ObjectSubclass<imp::Row>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Row {
    pub fn new(spacing: i32) -> Self {
        let row: Row = glib::Object::new();
        row.imp().spacing.set(spacing);
        row
    }

    pub fn append(&self, child: &impl IsA<gtk4::Widget>) {
        child.set_parent(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_width_goes_to_expanding_children_by_their_natural_width() {
        assert_eq!(
            lefts(&[0, 0], &[278.0, 198.0], &[true, true], 4, 600),
            [0, 352]
        );
        assert_eq!(
            lefts(
                &[0, 0, 0],
                &[20.0, 100.0, 30.0],
                &[false, true, false],
                8,
                300
            ),
            [0, 28, 270]
        );
        assert_eq!(
            lefts(&[0, 0], &[40.0, 60.0], &[false, false], 4, 304),
            [0, 124]
        );
        assert_eq!(
            lefts(&[0, 0], &[75.4, 100.0], &[false, true], 4, 600),
            [0, 79]
        );
    }

    #[test]
    fn missing_width_is_taken_from_expanding_children_down_to_their_minimum() {
        assert_eq!(
            lefts(&[100, 80], &[300.0, 250.0], &[true, true], 4, 500),
            [0, 274]
        );
        assert_eq!(
            lefts(&[100, 80], &[300.0, 250.0], &[false, true], 4, 400),
            [0, 304]
        );
        assert_eq!(
            lefts(&[100, 80], &[300.0, 250.0], &[true, true], 4, 100),
            [0, 104]
        );
    }
}
