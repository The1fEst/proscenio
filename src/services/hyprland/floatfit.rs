use serde_json::Value;

use crate::platform::hyprland::hypr::{self, Events};

const OVERSIZE_PERCENT: f64 = 85.0;
const FALLBACK_PERCENT: f64 = 70.0;

pub fn start(events: &Events) {
    events
        .subscribe(|event, data| {
            if event != "changefloatingmode" {
                return;
            }
            if let Some(address) = parse_floated_address(data) {
                fit_window(&address);
            }
        })
        .detach();
}

fn parse_floated_address(data: &str) -> Option<String> {
    let (address, is_floating) = data.split_once(',')?;
    (is_floating.trim() == "1" && !address.is_empty())
        .then(|| format!("0x{}", address.trim_start_matches("0x")))
}

fn fit_length(length: f64, work_length: f64) -> f64 {
    if length * 100.0 > work_length * OVERSIZE_PERCENT {
        return (work_length * FALLBACK_PERCENT / 100.0).floor();
    }
    length
}

fn fit_size(size: (f64, f64), work_size: (f64, f64)) -> (f64, f64) {
    (
        fit_length(size.0, work_size.0),
        fit_length(size.1, work_size.1),
    )
}

fn read_number_pair(value: &Value, key: &str) -> Option<(f64, f64)> {
    let list = value.get(key)?.as_array()?;
    Some((list.first()?.as_f64()?, list.get(1)?.as_f64()?))
}

fn compute_work_size(monitor: &Value) -> Option<(f64, f64)> {
    let scale = monitor.get("scale").and_then(Value::as_f64)?;
    let width = monitor.get("width").and_then(Value::as_f64)?;
    let height = monitor.get("height").and_then(Value::as_f64)?;
    let reserved: Vec<f64> = monitor
        .get("reserved")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|inset| inset.as_f64().unwrap_or(0.0))
                .collect()
        })
        .unwrap_or_default();
    let inset = |index: usize| reserved.get(index).copied().unwrap_or(0.0);
    Some((
        width / scale - inset(0) - inset(2),
        height / scale - inset(1) - inset(3),
    ))
}

fn fit_window(address: &str) {
    let Some(clients) = hypr::json("clients") else {
        return;
    };
    let Some(client) = clients.as_array().and_then(|list| {
        list.iter()
            .find(|entry| entry.get("address").and_then(Value::as_str) == Some(address))
    }) else {
        return;
    };
    let Some(size) = read_number_pair(client, "size") else {
        return;
    };
    let Some(monitor_id) = client.get("monitor").and_then(Value::as_i64) else {
        return;
    };
    let Some(monitors) = hypr::json("monitors") else {
        return;
    };
    let Some(work_size) = monitors
        .as_array()
        .and_then(|list| {
            list.iter()
                .find(|entry| entry.get("id").and_then(Value::as_i64) == Some(monitor_id))
        })
        .and_then(compute_work_size)
    else {
        return;
    };
    let fitted = fit_size(size, work_size);
    if fitted != size {
        hypr::resize_and_center_window(address, fitted.0, fitted.1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_window_that_became_floating_is_fitted() {
        let cases = [
            ("55d0c3a1b2c0,1", Some("0x55d0c3a1b2c0")),
            ("55d0c3a1b2c0,0", None),
            (",1", None),
            ("55d0c3a1b2c0", None),
            ("", None),
        ];
        for (data, expected) in cases {
            assert_eq!(parse_floated_address(data).as_deref(), expected, "{data}");
        }
    }

    #[test]
    fn an_oversized_axis_shrinks_to_seventy_percent_of_the_work_area() {
        let cases = [
            ((800.0, 600.0), (2560.0, 1400.0), (800.0, 600.0)),
            ((2560.0, 600.0), (2560.0, 1400.0), (1792.0, 600.0)),
            ((800.0, 1400.0), (2560.0, 1400.0), (800.0, 980.0)),
            ((2560.0, 1440.0), (2560.0, 1400.0), (1792.0, 980.0)),
            ((2176.0, 1190.0), (2560.0, 1400.0), (2176.0, 1190.0)),
            ((2177.0, 1191.0), (2560.0, 1400.0), (1792.0, 980.0)),
            ((1800.0, 700.0), (2048.0, 1152.0), (1433.0, 700.0)),
        ];
        for (size, work, expected) in cases {
            assert_eq!(fit_size(size, work), expected, "{size:?} in {work:?}");
        }
    }

    #[test]
    fn the_work_area_is_the_scaled_monitor_minus_the_reserved_insets() {
        let cases = [
            (
                json!({"width": 2560, "height": 1440, "scale": 1.0, "reserved": [0, 40, 0, 0]}),
                Some((2560.0, 1400.0)),
            ),
            (
                json!({"width": 2560, "height": 1440, "scale": 1.25, "reserved": [10, 40, 6, 20]}),
                Some((2032.0, 1092.0)),
            ),
            (
                json!({"width": 1920, "height": 1080, "scale": 1.0}),
                Some((1920.0, 1080.0)),
            ),
            (json!({"width": 1920, "height": 1080}), None),
        ];
        for (monitor, expected) in cases {
            assert_eq!(compute_work_size(&monitor), expected, "{monitor}");
        }
    }
}
