use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::core::{gsettings, process};
use crate::platform::system::appearance::with_ini_value;
use crate::platform::system::sessionenvironment;

const SCHEMA: &str = "org.gnome.system.proxy";
const KIO_SECTION: &str = "[Proxy Settings]";
const PROTOCOLS: [&str; 3] = ["http", "https", "socks"];
const APPLICATION: &str = "proscenio";

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
    pub user: String,
    pub password: String,
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

impl Endpoint {
    fn is_set(&self) -> bool {
        !self.host.trim().is_empty() && self.port > 0
    }

    fn url(&self, scheme: &str) -> String {
        let host = self.host.trim();
        let (scheme, host) = host.split_once("://").unwrap_or((scheme, host));
        let login = match (self.user.is_empty(), self.password.is_empty()) {
            (true, _) => String::new(),
            (false, true) => format!("{}@", encode(&self.user)),
            (false, false) => format!("{}:{}@", encode(&self.user), encode(&self.password)),
        };
        format!("{scheme}://{login}{host}:{}", self.port)
    }

    fn to_kio_value(&self, scheme: &str) -> String {
        if !self.is_set() {
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
    pub uses_same_proxy: bool,
    pub ignore_hosts: Vec<String>,
}

impl Proxy {
    fn https_endpoint(&self) -> &Endpoint {
        if self.uses_same_proxy {
            &self.http
        } else {
            &self.https
        }
    }

    fn endpoints(&self) -> [&Endpoint; 3] {
        [&self.http, &self.https, &self.socks]
    }

    pub fn endpoint_mut(&mut self, protocol: &str) -> &mut Endpoint {
        match protocol {
            "https" => &mut self.https,
            "socks" => &mut self.socks,
            _ => &mut self.http,
        }
    }

    pub fn environment(&self) -> Vec<(&'static str, String)> {
        let is_manual = self.mode == Mode::Manual;
        let to_url = |endpoint: &Endpoint, scheme: &str| {
            if is_manual && endpoint.is_set() {
                endpoint.url(scheme)
            } else {
                String::new()
            }
        };
        let http = to_url(&self.http, "http");
        let https = to_url(self.https_endpoint(), "http");
        let socks = to_url(&self.socks, "socks5");
        let no_proxy = if is_manual {
            self.ignore_hosts.join(",")
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
            ("no_proxy", no_proxy.clone()),
            ("NO_PROXY", no_proxy),
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
            ("httpProxy", self.http.to_kio_value("http")),
            ("httpsProxy", self.https_endpoint().to_kio_value("http")),
            ("socksProxy", self.socks.to_kio_value("socks")),
            ("NoProxyFor", self.ignore_hosts.join(",")),
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
        ..Endpoint::default()
    }
}

pub fn is_available() -> bool {
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
        uses_same_proxy: settings.boolean("use-same-proxy"),
        ignore_hosts: settings
            .strv("ignore-hosts")
            .iter()
            .map(|host| host.to_string())
            .collect(),
    }
}

fn secret_attributes(protocol: &str, field: &str) -> Vec<String> {
    [
        "application",
        APPLICATION,
        "proxy",
        protocol,
        "field",
        field,
    ]
    .iter()
    .map(|part| (*part).to_owned())
    .collect()
}

async fn lookup(protocol: &str, field: &str) -> String {
    let mut command = process::command(&["secret-tool", "lookup"]);
    command.args(secret_attributes(protocol, field));
    process::capture_text(command).await.unwrap_or_default()
}

pub async fn with_credentials(mut proxy: Proxy) -> Proxy {
    for protocol in PROTOCOLS {
        let user = lookup(protocol, "user").await;
        let password = lookup(protocol, "password").await;
        let endpoint = proxy.endpoint_mut(protocol);
        endpoint.user = user;
        endpoint.password = password;
    }
    proxy
}

fn store(protocol: &str, field: &str, value: &str) {
    let mut command = Command::new("secret-tool");
    if value.is_empty() {
        command.arg("clear");
    } else {
        command
            .arg("store")
            .arg(format!("--label=Proxy {field} ({protocol})"));
    }
    command
        .args(secret_attributes(protocol, field))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return;
    };
    if let Some(mut input) = child.stdin.take() {
        let _ = input.write_all(value.as_bytes());
    }
    let _ = child.wait();
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
    let _ = settings.set_boolean("use-same-proxy", proxy.uses_same_proxy);
    let ignore_hosts: Vec<&str> = proxy.ignore_hosts.iter().map(String::as_str).collect();
    let _ = settings.set_strv("ignore-hosts", ignore_hosts.as_slice());
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
    if let Some(http) = gsettings::open(&format!("{SCHEMA}.http")) {
        let _ = http.set_boolean("use-authentication", !proxy.http.user.is_empty());
        let _ = http.set_string("authentication-user", &proxy.http.user);
        let _ = http.set_string("authentication-password", &proxy.http.password);
    }
    gio::Settings::sync();
}

pub fn apply_environment(proxy: &Proxy) {
    sessionenvironment::apply(&proxy.environment());
}

pub fn write(proxy: &Proxy, previous: &Proxy) {
    write_gsettings(proxy);
    apply_environment(proxy);
    let secret_changes: Vec<(&'static str, &'static str, String)> = PROTOCOLS
        .iter()
        .zip(proxy.endpoints().into_iter().zip(previous.endpoints()))
        .flat_map(|(protocol, (current_endpoint, previous_endpoint))| {
            let user_change = (current_endpoint.user != previous_endpoint.user)
                .then(|| (*protocol, "user", current_endpoint.user.clone()));
            let password_change = (current_endpoint.password != previous_endpoint.password)
                .then(|| (*protocol, "password", current_endpoint.password.clone()));
            user_change.into_iter().chain(password_change)
        })
        .collect();
    let proxy = proxy.clone();
    gio::spawn_blocking(move || {
        let kio = glib::user_config_dir().join("kioslaverc");
        let text = std::fs::read_to_string(&kio).unwrap_or_default();
        let _ = std::fs::write(&kio, proxy.with_kio(&text));
        for (protocol, field, value) in secret_changes {
            store(protocol, field, &value);
        }
    });
}

pub fn apply_at_start() {
    let proxy = read();
    if proxy.mode != Mode::Manual {
        return;
    }
    glib::spawn_future_local(async move {
        apply_environment(&with_credentials(proxy).await);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::hyprland::hyprconfig::lua_string;

    fn endpoint(host: &str, port: u16) -> Endpoint {
        Endpoint {
            host: host.to_owned(),
            port,
            ..Endpoint::default()
        }
    }

    fn manual_proxy() -> Proxy {
        Proxy {
            mode: Mode::Manual,
            http: endpoint("proxy.lan", 3128),
            https: endpoint("secure.lan", 443),
            socks: endpoint("socks.lan", 1080),
            ignore_hosts: vec!["localhost".to_owned(), "10.0.0.0/8".to_owned()],
            ..Proxy::default()
        }
    }

    fn value(environment: &[(&str, String)], name: &str) -> String {
        environment
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    }

    #[test]
    fn manual_proxies_become_the_usual_variables_and_others_clear_them() {
        let mut proxy = manual_proxy();
        let environment = proxy.environment();
        assert_eq!(value(&environment, "http_proxy"), "http://proxy.lan:3128");
        assert_eq!(value(&environment, "HTTPS_PROXY"), "http://secure.lan:443");
        assert_eq!(value(&environment, "all_proxy"), "socks5://socks.lan:1080");
        assert_eq!(value(&environment, "no_proxy"), "localhost,10.0.0.0/8");
        proxy.uses_same_proxy = true;
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
    fn credentials_go_into_the_url_escaped() {
        let mut proxy = manual_proxy();
        proxy.http.user = "fest".to_owned();
        proxy.http.password = "p@ss:w/rd".to_owned();
        proxy.socks.user = "only".to_owned();
        let environment = proxy.environment();
        assert_eq!(
            value(&environment, "http_proxy"),
            "http://fest:p%40ss%3Aw%2Frd@proxy.lan:3128"
        );
        assert_eq!(
            value(&environment, "all_proxy"),
            "socks5://only@socks.lan:1080"
        );
        assert_eq!(lua_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(lua_string("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn kde_reads_the_same_proxy_from_its_own_section() {
        let text = "[General]\nfoo=bar\n";
        let kio_text = manual_proxy().with_kio(text);
        assert_eq!(
            kio_text,
            "[General]\nfoo=bar\n\n[Proxy Settings]\nProxyType=1\nProxy Config Script=\nhttpProxy=http://proxy.lan 3128\nhttpsProxy=http://secure.lan 443\nsocksProxy=socks://socks.lan 1080\nNoProxyFor=localhost,10.0.0.0/8\n"
        );
        let automatic_proxy = Proxy {
            mode: Mode::Automatic,
            ..Proxy::default()
        };
        assert!(
            automatic_proxy
                .with_kio(&kio_text)
                .contains("ProxyType=3\n")
        );
        assert!(
            Proxy::default()
                .with_kio("")
                .starts_with("[Proxy Settings]\nProxyType=0\n")
        );
    }
}
