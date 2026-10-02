use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::core::{gsettings, process};
use crate::platform::appearance::{set_lua_env, with_ini_value};
use crate::platform::hypr;

const SCHEMA: &str = "org.gnome.system.proxy";
const KIO_SECTION: &str = "[Proxy Settings]";
const PROTOCOLS: [&str; 3] = ["http", "https", "socks"];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Mode {
    #[default]
    Off,
    Automatic,
    Manual,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    fn set(&self) -> bool {
        !self.host.trim().is_empty() && self.port > 0
    }

    fn url(&self, scheme: &str) -> String {
        let host = self.host.trim();
        if host.contains("://") {
            format!("{host}:{}", self.port)
        } else {
            format!("{scheme}://{host}:{}", self.port)
        }
    }

    fn kio(&self, scheme: &str) -> String {
        if !self.set() {
            return String::new();
        }
        let host = self.host.trim();
        let host = if host.contains("://") {
            host.to_owned()
        } else {
            format!("{scheme}://{host}")
        };
        format!("{host} {}", self.port)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Proxy {
    pub mode: Mode,
    pub script: String,
    pub http: Endpoint,
    pub https: Endpoint,
    pub socks: Endpoint,
    pub same: bool,
    pub ignore: Vec<String>,
}

impl Proxy {
    fn https_endpoint(&self) -> &Endpoint {
        if self.same { &self.http } else { &self.https }
    }

    pub fn environment(&self) -> Vec<(&'static str, String)> {
        let manual = self.mode == Mode::Manual;
        let value = |endpoint: &Endpoint, scheme: &str| {
            if manual && endpoint.set() {
                endpoint.url(scheme)
            } else {
                String::new()
            }
        };
        let http = value(&self.http, "http");
        let https = value(self.https_endpoint(), "http");
        let socks = value(&self.socks, "socks5");
        let ignore = if manual {
            self.ignore.join(",")
        } else {
            String::new()
        };
        vec![
            ("http_proxy", http.clone()),
            ("HTTP_PROXY", http),
            ("https_proxy", https.clone()),
            ("HTTPS_PROXY", https),
            ("all_proxy", socks.clone()),
            ("ALL_PROXY", socks),
            ("no_proxy", ignore.clone()),
            ("NO_PROXY", ignore),
        ]
    }

    pub fn with_kio(&self, text: &str) -> String {
        let kind = match self.mode {
            Mode::Off => "0",
            Mode::Manual => "1",
            Mode::Automatic if self.script.trim().is_empty() => "3",
            Mode::Automatic => "2",
        };
        let entries = [
            ("ProxyType", kind.to_owned()),
            ("Proxy Config Script", self.script.trim().to_owned()),
            ("httpProxy", self.http.kio("http")),
            ("httpsProxy", self.https_endpoint().kio("http")),
            ("socksProxy", self.socks.kio("socks")),
            ("NoProxyFor", self.ignore.join(",")),
        ];
        let text = if text.contains(KIO_SECTION) {
            text.to_owned()
        } else {
            format!("{}\n\n{KIO_SECTION}\n", text.trim_end_matches('\n'))
                .trim_start()
                .to_owned()
        };
        entries.iter().fold(text, |text, (key, value)| {
            with_ini_value(&text, key, value, KIO_SECTION)
        })
    }
}

fn endpoint(schema: &str) -> Endpoint {
    let Some(settings) = gsettings::open(&format!("{SCHEMA}.{schema}")) else {
        return Endpoint::default();
    };
    Endpoint {
        host: settings.string("host").to_string(),
        port: u16::try_from(settings.int("port")).unwrap_or(0),
    }
}

pub fn available() -> bool {
    gsettings::open(SCHEMA).is_some()
}

pub fn read() -> Proxy {
    let Some(settings) = gsettings::open(SCHEMA) else {
        return Proxy::default();
    };
    Proxy {
        mode: match settings.string("mode").as_str() {
            "manual" => Mode::Manual,
            "auto" => Mode::Automatic,
            _ => Mode::Off,
        },
        script: settings.string("autoconfig-url").to_string(),
        http: endpoint("http"),
        https: endpoint("https"),
        socks: endpoint("socks"),
        same: settings.boolean("use-same-proxy"),
        ignore: settings
            .strv("ignore-hosts")
            .iter()
            .map(|host| host.to_string())
            .collect(),
    }
}

fn write_gsettings(proxy: &Proxy) {
    let Some(settings) = gsettings::open(SCHEMA) else {
        return;
    };
    let mode = match proxy.mode {
        Mode::Off => "none",
        Mode::Automatic => "auto",
        Mode::Manual => "manual",
    };
    let _ = settings.set_string("mode", mode);
    let _ = settings.set_string("autoconfig-url", proxy.script.trim());
    let _ = settings.set_boolean("use-same-proxy", proxy.same);
    let ignore: Vec<&str> = proxy.ignore.iter().map(String::as_str).collect();
    let _ = settings.set_strv("ignore-hosts", ignore.as_slice());
    for (protocol, endpoint) in
        PROTOCOLS
            .iter()
            .zip([&proxy.http, proxy.https_endpoint(), &proxy.socks])
    {
        if let Some(settings) = gsettings::open(&format!("{SCHEMA}.{protocol}")) {
            let _ = settings.set_string("host", endpoint.host.trim());
            let _ = settings.set_int("port", i32::from(endpoint.port));
        }
    }
    gio::Settings::sync();
}

pub fn write(proxy: &Proxy) {
    write_gsettings(proxy);
    let kio = glib::user_config_dir().join("kioslaverc");
    let text = std::fs::read_to_string(&kio).unwrap_or_default();
    let _ = std::fs::write(&kio, proxy.with_kio(&text));
    let environment = proxy.environment();
    let mut update = vec![
        "dbus-update-activation-environment".to_owned(),
        "--systemd".to_owned(),
    ];
    for (name, value) in &environment {
        set_lua_env(name, value);
        update.push(format!("{name}={value}"));
        unsafe {
            if value.is_empty() {
                std::env::remove_var(name);
            } else {
                std::env::set_var(name, value);
            }
        }
    }
    process::start(process::quiet(&update));
    hypr::request("reload");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual() -> Proxy {
        Proxy {
            mode: Mode::Manual,
            http: Endpoint {
                host: "proxy.lan".to_owned(),
                port: 3128,
            },
            https: Endpoint {
                host: "secure.lan".to_owned(),
                port: 443,
            },
            socks: Endpoint {
                host: "socks.lan".to_owned(),
                port: 1080,
            },
            ignore: vec!["localhost".to_owned(), "10.0.0.0/8".to_owned()],
            ..Proxy::default()
        }
    }

    #[test]
    fn manual_proxies_become_the_usual_variables_and_others_clear_them() {
        let mut proxy = manual();
        let environment = proxy.environment();
        let value = |environment: &[(&str, String)], name: &str| {
            environment
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        assert_eq!(value(&environment, "http_proxy"), "http://proxy.lan:3128");
        assert_eq!(value(&environment, "HTTPS_PROXY"), "http://secure.lan:443");
        assert_eq!(value(&environment, "all_proxy"), "socks5://socks.lan:1080");
        assert_eq!(value(&environment, "no_proxy"), "localhost,10.0.0.0/8");
        proxy.same = true;
        assert_eq!(
            value(&proxy.environment(), "https_proxy"),
            "http://proxy.lan:3128"
        );
        proxy.mode = Mode::Automatic;
        assert!(
            proxy
                .environment()
                .iter()
                .all(|(_, value)| value.is_empty())
        );
    }

    #[test]
    fn kde_reads_the_same_proxy_from_its_own_section() {
        let text = "[General]\nfoo=bar\n";
        let written = manual().with_kio(text);
        assert_eq!(
            written,
            "[General]\nfoo=bar\n\n[Proxy Settings]\nProxyType=1\nProxy Config Script=\nhttpProxy=http://proxy.lan 3128\nhttpsProxy=http://secure.lan 443\nsocksProxy=socks://socks.lan 1080\nNoProxyFor=localhost,10.0.0.0/8\n"
        );
        let automatic = Proxy {
            mode: Mode::Automatic,
            ..Proxy::default()
        };
        assert!(automatic.with_kio(&written).contains("ProxyType=3\n"));
        assert!(
            Proxy::default()
                .with_kio("")
                .starts_with("[Proxy Settings]\nProxyType=0\n")
        );
    }
}
