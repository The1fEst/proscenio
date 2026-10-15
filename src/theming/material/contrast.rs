use material_colors::hct::Hct;

pub(super) fn python_round(value: f64) -> f64 {
    value.round_ties_even()
}

fn lab_f(t: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    if t > e {
        t.powf(1.0 / 3.0)
    } else {
        (kappa * t + 16.0) / 116.0
    }
}

fn lab_invf(ft: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    let cubed = ft * ft * ft;
    if cubed > e {
        cubed
    } else {
        (116.0 * ft - 16.0) / kappa
    }
}

fn y_from_lstar(lstar: f64) -> f64 {
    100.0 * lab_invf((lstar + 16.0) / 116.0)
}

fn lstar_from_y(y: f64) -> f64 {
    lab_f(y / 100.0) * 116.0 - 16.0
}

fn ratio_of_ys(first: f64, second: f64) -> f64 {
    let lighter_y = if first > second { first } else { second };
    let darker_y = if lighter_y == second { first } else { second };
    (lighter_y + 5.0) / (darker_y + 5.0)
}

pub(super) fn ratio_of_tones(first: f64, second: f64) -> f64 {
    ratio_of_ys(
        y_from_lstar(first.clamp(0.0, 100.0)),
        y_from_lstar(second.clamp(0.0, 100.0)),
    )
}

pub(super) fn lighter(tone: f64, ratio: f64) -> f64 {
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }
    let dark_y = y_from_lstar(tone);
    let light_y = ratio * (dark_y + 5.0) - 5.0;
    let real_contrast = ratio_of_ys(light_y, dark_y);
    if real_contrast < ratio && (real_contrast - ratio).abs() > 0.04 {
        return -1.0;
    }
    let value = lstar_from_y(light_y) + 0.4;
    if !(0.0..=100.0).contains(&value) {
        return -1.0;
    }
    value
}

pub(super) fn darker(tone: f64, ratio: f64) -> f64 {
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }
    let light_y = y_from_lstar(tone);
    let dark_y = (light_y + 5.0) / ratio - 5.0;
    let real_contrast = ratio_of_ys(light_y, dark_y);
    if real_contrast < ratio && (real_contrast - ratio).abs() > 0.04 {
        return -1.0;
    }
    let value = lstar_from_y(dark_y) - 0.4;
    if !(0.0..=100.0).contains(&value) {
        return -1.0;
    }
    value
}

fn lighter_unsafe(tone: f64, ratio: f64) -> f64 {
    let safe = lighter(tone, ratio);
    if safe < 0.0 { 100.0 } else { safe }
}

fn darker_unsafe(tone: f64, ratio: f64) -> f64 {
    let safe = darker(tone, ratio);
    if safe < 0.0 { 0.0 } else { safe }
}

pub(super) fn prefers_light_foreground(tone: f64) -> bool {
    python_round(tone) < 60.0
}

pub(super) fn foreground_tone(background: f64, ratio: f64) -> f64 {
    let lighter_tone = lighter_unsafe(background, ratio);
    let darker_tone = darker_unsafe(background, ratio);
    let lighter_ratio = ratio_of_tones(lighter_tone, background);
    let darker_ratio = ratio_of_tones(darker_tone, background);
    if prefers_light_foreground(background) {
        let is_difference_negligible = (lighter_ratio - darker_ratio).abs() < 0.1
            && lighter_ratio < ratio
            && darker_ratio < ratio;
        if lighter_ratio >= ratio || lighter_ratio >= darker_ratio || is_difference_negligible {
            lighter_tone
        } else {
            darker_tone
        }
    } else if darker_ratio >= ratio || darker_ratio >= lighter_ratio {
        darker_tone
    } else {
        lighter_tone
    }
}

pub(super) fn curve([low, normal, medium, high]: [f64; 4], level: f64) -> f64 {
    let lerp = |start: f64, stop: f64, amount: f64| (1.0 - amount) * start + amount * stop;
    if level <= -1.0 {
        low
    } else if level < 0.0 {
        lerp(low, normal, level + 1.0)
    } else if level < 0.5 {
        lerp(normal, medium, level / 0.5)
    } else if level < 1.0 {
        lerp(medium, high, (level - 0.5) / 0.5)
    } else {
        high
    }
}

pub(super) fn find_desired_chroma_by_tone(
    hue: f64,
    chroma: f64,
    tone: f64,
    decreasing: bool,
) -> f64 {
    let mut answer = tone;
    let mut closest = Hct::from(hue, chroma, tone);
    if closest.get_chroma() < chroma {
        let mut peak = closest.get_chroma();
        while closest.get_chroma() < chroma {
            answer += if decreasing { -1.0 } else { 1.0 };
            let candidate = Hct::from(hue, chroma, answer);
            if peak > candidate.get_chroma() {
                break;
            }
            if (candidate.get_chroma() - chroma).abs() < 0.4 {
                break;
            }
            if (candidate.get_chroma() - chroma).abs() < (closest.get_chroma() - chroma).abs() {
                closest = candidate;
            }
            peak = peak.max(candidate.get_chroma());
        }
    }
    answer
}
