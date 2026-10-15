use std::path::PathBuf;

use crate::core::paths;

pub const FILTERS: [&str; 5] = [
    "grayscale",
    "inverted",
    "deuteranopia",
    "protanopia",
    "tritanopia",
];

const HEADER: &str = "#version 300 es
precision mediump float;
in vec2 v_texcoord;
layout(location = 0) out vec4 fragColor;
uniform sampler2D tex;
";

const DALTONIZE: &str = "
vec3 daltonize(vec3 color) {
    float l = dot(color, vec3(17.8824, 43.5161, 4.11935));
    float m = dot(color, vec3(3.45565, 27.1554, 3.86714));
    float s = dot(color, vec3(0.0299566, 0.184309, 1.46709));
    vec3 lms = simulate(vec3(l, m, s));
    vec3 seen = vec3(
        dot(lms, vec3(0.0809444479, -0.130504409, 0.116721066)),
        dot(lms, vec3(-0.0102485335, 0.0540193266, -0.113614708)),
        dot(lms, vec3(-0.000365296938, -0.00412161469, 0.693511405)));
    vec3 lost = color - seen;
    vec3 shift = vec3(0.0, 0.7 * lost.r + lost.g, 0.7 * lost.r + lost.b);
    return clamp(color + shift, 0.0, 1.0);
}

void main() {
    vec4 color = texture(tex, v_texcoord);
    fragColor = vec4(daltonize(color.rgb), color.a);
}
";

pub fn source(filter: &str) -> Option<String> {
    let body = match filter {
        "grayscale" => "
void main() {
    vec4 color = texture(tex, v_texcoord);
    float gray = dot(color.rgb, vec3(0.2126, 0.7152, 0.0722));
    fragColor = vec4(vec3(gray), color.a);
}
"
        .to_owned(),
        "inverted" => "
void main() {
    vec4 color = texture(tex, v_texcoord);
    fragColor = vec4(1.0 - color.rgb, color.a);
}
"
        .to_owned(),
        "deuteranopia" => format!(
            "vec3 simulate(vec3 lms) {{ return vec3(lms.x, 0.494207 * lms.x + 1.24827 * lms.z, lms.z); }}\n{DALTONIZE}"
        ),
        "protanopia" => format!(
            "vec3 simulate(vec3 lms) {{ return vec3(2.02344 * lms.y - 2.52581 * lms.z, lms.y, lms.z); }}\n{DALTONIZE}"
        ),
        "tritanopia" => format!(
            "vec3 simulate(vec3 lms) {{ return vec3(lms.x, lms.y, -0.395913 * lms.x + 0.801109 * lms.y); }}\n{DALTONIZE}"
        ),
        _ => return None,
    };
    Some(format!("{HEADER}{body}"))
}

fn directory() -> PathBuf {
    paths::state().join("shaders")
}

pub fn install(filter: &str) -> Option<String> {
    let text = source(filter)?;
    let path = directory().join(format!("{filter}.frag"));
    std::fs::create_dir_all(directory()).ok()?;
    std::fs::write(&path, text).ok()?;
    Some(path.to_string_lossy().into_owned())
}

pub fn filter_of(shader: &str) -> String {
    let path = PathBuf::from(shader);
    if shader.is_empty() || path.parent() != Some(directory().as_path()) {
        return if shader.is_empty() {
            String::new()
        } else {
            "custom".to_owned()
        };
    }
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| FILTERS.contains(&stem.as_str()))
        .unwrap_or_else(|| "custom".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_filter_is_a_whole_shader_and_paths_map_back() {
        for filter in FILTERS {
            let text = source(filter).unwrap();
            assert!(text.starts_with("#version 300 es"), "{filter}");
            assert!(text.contains("void main()"), "{filter}");
        }
        assert_eq!(source("sepia"), None);
        let ours = directory().join("protanopia.frag");
        assert_eq!(filter_of(&ours.to_string_lossy()), "protanopia");
        assert_eq!(filter_of(""), "");
        assert_eq!(filter_of("/home/someone/blue.frag"), "custom");
    }
}
