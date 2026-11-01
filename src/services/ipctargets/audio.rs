use super::parse_percent;
use crate::platform::dbus::ipc::Ipc;
use crate::services::media::audio::Audio;

const STEP: f64 = 0.02;
const FINE_STEP: f64 = 0.01;
const FINE_STEP_LIMIT: f64 = 0.1;

fn find_next_volume(level: f64, should_raise: bool) -> Option<f64> {
    let step = if level < FINE_STEP_LIMIT {
        FINE_STEP
    } else {
        STEP
    };
    if should_raise {
        (level < 1.0).then(|| (level + step).min(1.0))
    } else {
        (level > 0.0).then(|| (level - step).max(0.0))
    }
}

fn step_volume(audio: &Audio, should_raise: bool) {
    if let Some(volume) = find_next_volume(audio.sink_volume.get(), should_raise) {
        audio.set_sink_volume(volume);
    }
}

pub fn register(ipc: &Ipc, audio: &Option<Audio>) {
    let add = |name: &'static str, act: fn(&Audio)| {
        let audio = audio.clone();
        ipc.add("audio", name, move || {
            if let Some(audio) = &audio {
                act(audio);
            }
        });
    };
    add("increment", |audio| step_volume(audio, true));
    add("decrement", |audio| step_volume(audio, false));
    add("toggleMute", |audio| audio.toggle_sink_mute());
    add("toggleMicrophoneMute", |audio| audio.toggle_source_mute());
    ipc.add_with("audio", "setVolume", &["percent: string"], {
        let audio = audio.clone();
        move |arguments| {
            if let (Some(audio), Some(percent)) = (&audio, parse_percent(&arguments[0])) {
                audio.set_sink_volume(f64::from(percent) / 100.0);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_is_finer_below_10_percent_and_stops_at_the_ends() {
        let cases = [
            (0.5, true, Some(52)),
            (0.5, false, Some(48)),
            (0.05, true, Some(6)),
            (0.05, false, Some(4)),
            (0.995, true, Some(100)),
            (0.005, false, Some(0)),
            (1.0, true, None),
            (1.3, true, None),
            (0.0, false, None),
        ];
        for (level, should_raise, expected) in cases {
            let percent =
                find_next_volume(level, should_raise).map(|volume| (volume * 100.0).round());
            assert_eq!(percent, expected.map(f64::from), "{level} {should_raise}");
        }
    }
}
