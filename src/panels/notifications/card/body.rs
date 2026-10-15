use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use std::path::PathBuf;

use crate::services::overlay::notifications::Notification;
use crate::services::overlay::notifications::images::{file_uri_to_path, is_file_uri};

const BODY_IMAGE_WIDTH: i32 = 300;
const BODY_IMAGE_HEIGHT: i32 = 200;

pub(super) fn body_label(notification: &Notification, should_wrap: bool) -> gtk4::Widget {
    let label = gtk4::Label::new(None);
    label.add_css_class("notif-body");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    let text = body_text(&notification.body, &notification.app_name);
    match markup(&text) {
        Some(markup) => label.set_markup(&markup),
        None => label.set_text(&text),
    }
    label.connect_activate_link(|_, address| {
        let _ =
            gtk4::gio::AppInfo::launch_default_for_uri(address, gtk4::gio::AppLaunchContext::NONE);
        glib::Propagation::Stop
    });
    if should_wrap {
        label.set_wrap(true);
        label.set_wrap_mode(pango::WrapMode::WordChar);
    } else {
        label.set_ellipsize(pango::EllipsizeMode::End);
        label.set_lines(1);
        label.set_single_line_mode(true);
    }
    label.upcast()
}

pub(super) fn body_text(body: &str, app_name: &str) -> String {
    const CHROMIUM: &[&str] = &[
        "brave",
        "chrome",
        "chromium",
        "vivaldi",
        "opera",
        "microsoft edge",
    ];
    let lowercase_name = app_name.to_lowercase();
    if CHROMIUM.iter().any(|name| lowercase_name.contains(name)) {
        let parts: Vec<&str> = body.split("\n\n").collect();
        if parts.len() > 1 && parts[0].starts_with("<a") {
            return parts[1..].join("\n\n");
        }
    }
    body.to_owned()
}

fn markup(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut probe_markup = String::with_capacity(text.len());
    let mut is_in_link = false;
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let plain = glib::markup_escape_text(&rest[..start]);
        out.push_str(&plain);
        probe_markup.push_str(&plain);
        let Some(end) = rest[start..].find('>') else {
            let tail = glib::markup_escape_text(&rest[start..]);
            out.push_str(&tail);
            probe_markup.push_str(&tail);
            return check(out, &probe_markup);
        };
        let tag = &rest[start + 1..start + end];
        let is_closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        match name.as_str() {
            "b" | "i" | "u" | "s" => {
                let tag_markup = format!("<{tag}>");
                out.push_str(&tag_markup);
                probe_markup.push_str(&tag_markup);
            }
            "a" if is_closing && is_in_link => {
                is_in_link = false;
                out.push_str("</a>");
                probe_markup.push_str("</span>");
            }
            "a" if !is_closing && !is_in_link => {
                if let Some(address) = attribute(tag, "href") {
                    is_in_link = true;
                    let address = glib::markup_escape_text(&unescape(&address));
                    out.push_str(&format!("<a href=\"{address}\">"));
                    probe_markup.push_str("<span>");
                }
            }
            "img" if !is_closing => {
                if let Some(alt) = attribute(tag, "alt") {
                    let alt = glib::markup_escape_text(&unescape(&alt));
                    out.push_str(&alt);
                    probe_markup.push_str(&alt);
                }
            }
            _ => {}
        }
        rest = &rest[start + end + 1..];
    }
    let tail = glib::markup_escape_text(rest);
    out.push_str(&tail);
    probe_markup.push_str(&tail);
    if is_in_link {
        out.push_str("</a>");
        probe_markup.push_str("</span>");
    }
    check(out, &probe_markup)
}

pub(super) fn body_images(text: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        let tag = &rest[start + 1..start + end];
        let is_image = tag
            .split_whitespace()
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("img"));
        if let Some(source) = attribute(tag, "src").filter(|_| is_image) {
            let source = unescape(&source);
            if is_file_uri(&source) {
                paths.extend(file_uri_to_path(&source));
            } else if source.starts_with('/') {
                paths.push(PathBuf::from(source));
            }
        }
        rest = &rest[start + end + 1..];
    }
    paths
}

pub(super) fn body_image(path: PathBuf) -> gtk4::Widget {
    let picture = gtk4::Picture::new();
    picture.add_css_class("notif-body-image");
    picture.set_overflow(gtk4::Overflow::Hidden);
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk4::ContentFit::ScaleDown);
    picture.set_halign(gtk4::Align::Start);
    picture.set_visible(false);
    let scale = gdk::Display::default()
        .map(|display| {
            display
                .monitors()
                .iter::<gdk::Monitor>()
                .flatten()
                .map(|monitor| monitor.scale_factor())
                .max()
                .unwrap_or(1)
        })
        .unwrap_or(1);
    let size = (BODY_IMAGE_WIDTH * scale, BODY_IMAGE_HEIGHT * scale);
    let weak_picture = picture.downgrade();
    glib::spawn_future_local(async move {
        let texture = crate::ui::image::texture(path, size).await;
        if let (Some(picture), Some(texture)) = (weak_picture.upgrade(), texture) {
            picture.set_paintable(Some(&texture));
            picture.set_visible(true);
        }
    });
    picture.upcast()
}

fn attribute(tag: &str, attribute_name: &str) -> Option<String> {
    let mut rest = tag.split_once(char::is_whitespace)?.1;
    loop {
        let (name, after) = rest.split_once('=')?;
        let after = after.trim_start();
        let quote = after
            .chars()
            .next()
            .filter(|mark| matches!(mark, '"' | '\''))?;
        let (value, next) = after[1..].split_once(quote)?;
        if name.trim().eq_ignore_ascii_case(attribute_name) {
            return Some(value.to_owned());
        }
        rest = next;
    }
}

fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn check(markup: String, probe_markup: &str) -> Option<String> {
    pango::parse_markup(probe_markup, '\u{0}')
        .ok()
        .map(|_| markup)
}

#[cfg(test)]
mod tests {
    use super::{body_images, body_text, markup};
    use crate::services::overlay::notifications::server::CAPABILITIES;
    use std::path::PathBuf;

    #[test]
    fn chromium_notifications_lose_their_link_line() {
        let body = "<a href=\"https://example.invalid\">example.invalid</a>\n\nThe actual text";
        assert_eq!(body_text(body, "Brave"), "The actual text");
        assert_eq!(body_text(body, "Thunderbird"), body);
    }

    #[test]
    fn a_body_without_a_link_line_is_left_alone() {
        let body = "First paragraph\n\nSecond paragraph";
        assert_eq!(body_text(body, "chromium"), body);
    }

    #[test]
    fn markup_keeps_emphasis_and_links_and_drops_everything_else() {
        assert_eq!(
            markup("<b>bold</b> <a href=\"https://x.invalid\">link</a> <span>plain</span>"),
            Some("<b>bold</b> <a href=\"https://x.invalid\">link</a> plain".to_owned())
        );
    }

    #[test]
    fn a_link_keeps_only_its_address_escaped_once() {
        assert_eq!(
            markup("<a class='c' href='https://x.invalid/?a=1&amp;b=\"2\"'>link</a>"),
            Some("<a href=\"https://x.invalid/?a=1&amp;b=&quot;2&quot;\">link</a>".to_owned())
        );
    }

    #[test]
    fn a_link_without_an_address_becomes_its_text() {
        assert_eq!(
            markup("<a name=\"top\">link</a> text"),
            Some("link text".to_owned())
        );
    }

    #[test]
    fn links_and_images_are_advertised_only_if_the_body_keeps_them() {
        let link_markup = markup("<a href=\"x\">link</a>").unwrap_or_default();
        assert_eq!(
            CAPABILITIES.contains(&"body-hyperlinks"),
            link_markup.contains("<a")
        );
        assert_eq!(
            CAPABILITIES.contains(&"body-images"),
            !body_images("<img src=\"/tmp/picture.png\"/>").is_empty()
        );
    }

    #[test]
    fn an_image_reads_as_its_alt_text_in_the_line() {
        assert_eq!(
            markup("see <img src=\"/tmp/a.png\" alt=\"a cat\"/> and <img src=\"/tmp/b.png\"/>."),
            Some("see a cat and .".to_owned())
        );
    }

    #[test]
    fn body_images_are_local_files_only() {
        assert_eq!(
            body_images(
                "<img src=\"file:///tmp/a%20b.png\"/> <img alt='c' src='/tmp/c.png'> \
                 <img src=\"https://x.invalid/d.png\"/> <img src=\"e.png\"/> \
                 <img src=\"file://x.invalid/srv/f.png\"/> <img src=\"FILE:///tmp/g.png\"/>"
            ),
            vec![
                PathBuf::from("/tmp/a b.png"),
                PathBuf::from("/tmp/c.png"),
                PathBuf::from("/tmp/g.png")
            ]
        );
    }

    #[test]
    fn markup_escapes_bare_ampersands() {
        assert_eq!(
            markup("tea & biscuits"),
            Some("tea &amp; biscuits".to_owned())
        );
    }
}
