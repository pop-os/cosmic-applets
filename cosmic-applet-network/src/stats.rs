// SPDX-License-Identifier: GPL-3.0-or-later

use std::{collections::VecDeque, fs, io, net::Ipv4Addr, path::Path, time::Instant};

use crate::fl;

pub const RATE_HISTORY: usize = 40;
const PING_WINDOW: usize = 10;
const PING_BINS: &[&str] = &["/usr/bin/ping", "/bin/ping"];
const RESOLVECTL_BIN: &str = "/usr/bin/resolvectl";

#[derive(Debug, Default)]
pub struct LiveStats {
    pub interface: Option<String>,
    pub gateway: Option<String>,
    pub ip: Option<String>,
    pub dns_servers: Vec<String>,
    pub dns_provider: Option<&'static str>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub rx_history: VecDeque<f64>,
    pub tx_history: VecDeque<f64>,
    pub router_ping_ms: Option<f64>,
    pub ping_results: VecDeque<bool>,
    pub internet_ping: InternetPing,
    pub has_rates: bool,
    prev_rx: Option<u64>,
    prev_tx: Option<u64>,
    prev_at: Option<Instant>,
    prev_iface: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LinkProbe {
    pub generation: u64,
    pub router_ms: Option<f64>,
    pub dns_servers: Option<Vec<String>>,
}

const INTERNET_PING_HOST: &str = "1.1.1.1";
const INTERNET_PING_COUNT: u32 = 3;

#[derive(Debug, Clone, Default)]
pub enum InternetPing {
    #[default]
    Idle,
    Running,
    Done {
        avg_ms: Option<f64>,
        loss: Option<u8>,
    },
}

#[derive(Debug, Clone)]
pub struct InternetPingResult {
    pub generation: u64,
    pub avg_ms: Option<f64>,
    pub loss: Option<u8>,
}

impl LiveStats {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn packet_loss_percent(&self) -> Option<u8> {
        if self.ping_results.is_empty() {
            return None;
        }
        let lost = self.ping_results.iter().filter(|ok| !*ok).count();
        Some(((lost as f64 / self.ping_results.len() as f64) * 100.0).round() as u8)
    }

    /// Refresh counters from sysfs / routing. Returns `true` when the default
    /// interface changed so callers can re-run `resolvectl`.
    pub fn poll_counters(&mut self) -> bool {
        let Some((iface, gateway)) = default_route() else {
            if self.interface.is_some() {
                self.reset();
            }
            return false;
        };

        let iface_changed = self.prev_iface.as_deref() != Some(iface.as_str());
        if iface_changed {
            self.rx_history.clear();
            self.tx_history.clear();
            self.prev_rx = None;
            self.prev_tx = None;
            self.prev_at = None;
            self.has_rates = false;
            self.router_ping_ms = None;
            self.ping_results.clear();
        }

        self.interface = Some(iface.clone());
        self.gateway = gateway;

        let from_files = dns_from_files();
        if !from_files.is_empty() {
            self.dns_servers = from_files;
            self.dns_provider = dns_provider_name(&self.dns_servers);
        }

        let rx = read_sysfs_u64(&format!("/sys/class/net/{iface}/statistics/rx_bytes"));
        let tx = read_sysfs_u64(&format!("/sys/class/net/{iface}/statistics/tx_bytes"));
        let now = Instant::now();

        if let (Some(rx), Some(tx)) = (rx, tx) {
            self.rx_bytes = rx;
            self.tx_bytes = tx;
            if let (Some(prev_rx), Some(prev_tx), Some(prev_at)) =
                (self.prev_rx, self.prev_tx, self.prev_at)
            {
                let dt = now.saturating_duration_since(prev_at).as_secs_f64();
                if dt > 0.2 {
                    self.rx_rate = rx.saturating_sub(prev_rx) as f64 / dt;
                    self.tx_rate = tx.saturating_sub(prev_tx) as f64 / dt;
                    self.has_rates = true;
                    push_history(&mut self.rx_history, self.rx_rate);
                    push_history(&mut self.tx_history, self.tx_rate);
                }
            }
            self.prev_rx = Some(rx);
            self.prev_tx = Some(tx);
            self.prev_at = Some(now);
        }

        self.prev_iface = Some(iface);
        iface_changed
    }

    pub fn apply_probe(&mut self, probe: LinkProbe) {
        self.router_ping_ms = probe.router_ms;
        self.ping_results.push_back(probe.router_ms.is_some());
        while self.ping_results.len() > PING_WINDOW {
            self.ping_results.pop_front();
        }
        if let Some(servers) = probe.dns_servers
            && !servers.is_empty()
        {
            self.dns_servers = servers;
            self.dns_provider = dns_provider_name(&self.dns_servers);
        }
    }

    pub fn apply_internet_ping(&mut self, result: InternetPingResult) {
        self.internet_ping = InternetPing::Done {
            avg_ms: result.avg_ms,
            loss: result.loss,
        };
    }
}

fn push_history(history: &mut VecDeque<f64>, value: f64) {
    history.push_back(value);
    while history.len() > RATE_HISTORY {
        history.pop_front();
    }
}

fn read_sysfs_u64(path: &str) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Returns `(interface, gateway)` for the lowest-metric IPv4 default route.
pub fn default_route() -> Option<(String, Option<String>)> {
    let contents = fs::read_to_string("/proc/net/route").ok()?;
    let mut best: Option<(u32, String, Option<String>)> = None;

    for line in contents.lines().skip(1) {
        let mut cols = line.split_whitespace();
        let iface = cols.next()?;
        let destination = cols.next()?;
        let gateway_hex = cols.next()?;
        let flags_hex = cols.next()?;
        let _refcnt = cols.next()?;
        let _use = cols.next()?;
        let metric = cols.next()?.parse::<u32>().unwrap_or(u32::MAX);

        if destination != "00000000" || iface == "lo" {
            continue;
        }
        let flags = u32::from_str_radix(flags_hex, 16).unwrap_or(0);
        // RTF_UP
        if flags & 0x0001 == 0 {
            continue;
        }

        let gateway = parse_route_ipv4(gateway_hex).filter(|ip| *ip != Ipv4Addr::UNSPECIFIED);
        let candidate = (metric, iface.to_string(), gateway.map(|ip| ip.to_string()));
        match &best {
            Some((best_metric, _, _)) if *best_metric <= metric => {}
            _ => best = Some(candidate),
        }
    }

    best.map(|(_, iface, gateway)| (iface, gateway))
}

fn parse_route_ipv4(hex: &str) -> Option<Ipv4Addr> {
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(Ipv4Addr::from(value.to_le_bytes()))
}

fn dns_from_files() -> Vec<String> {
    for path in ["/run/systemd/resolve/resolv.conf", "/etc/resolv.conf"] {
        let servers = parse_resolv_conf(path);
        if !servers.is_empty() {
            return servers;
        }
    }
    Vec::new()
}

fn resolvectl_dns(iface: &str) -> Option<Vec<String>> {
    let output = std::process::Command::new(RESOLVECTL_BIN)
        .args(["dns", iface])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_resolvectl_dns(&String::from_utf8_lossy(&output.stdout))
}

fn parse_resolvectl_dns(text: &str) -> Option<Vec<String>> {
    let mut servers = Vec::new();
    for token in text.split_whitespace() {
        let token = token.split('#').next().unwrap_or(token);
        if token.contains(':') && token.contains('.') {
            continue;
        }
        if token.parse::<std::net::IpAddr>().is_ok() {
            servers.push(token.to_string());
        }
    }
    if servers.is_empty() {
        None
    } else {
        Some(servers)
    }
}

fn parse_resolv_conf(path: impl AsRef<Path>) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("nameserver ")?.trim();
            if rest.is_empty() || rest == "127.0.0.53" || rest == "::1" {
                return None;
            }
            Some(rest.to_string())
        })
        .collect()
}

pub fn dns_provider_name(servers: &[String]) -> Option<&'static str> {
    let matches = |needles: &[&str]| {
        servers.iter().any(|server| {
            needles
                .iter()
                .any(|needle| server == needle || server.starts_with(needle))
        })
    };

    if matches(&[
        "1.1.1.1",
        "1.0.0.1",
        "2606:4700:4700::1111",
        "2606:4700:4700::1001",
    ]) {
        Some("Cloudflare")
    } else if matches(&[
        "8.8.8.8",
        "8.8.4.4",
        "2001:4860:4860::8888",
        "2001:4860:4860::8844",
    ]) {
        Some("Google")
    } else if matches(&["9.9.9.9", "149.112.112.112", "2620:fe::fe", "2620:fe::9"]) {
        Some("Quad9")
    } else if servers.is_empty() {
        None
    } else {
        Some("DHCP")
    }
}

fn missing() -> String {
    fl!("no-value")
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub fn format_rate(bytes_per_sec: f64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec.max(0.0) as u64))
}

pub fn format_ping_ms(ms: Option<f64>) -> String {
    match ms {
        Some(ms) if ms < 10.0 => format!("{ms:.1} ms"),
        Some(ms) => format!("{ms:.0} ms"),
        None => missing(),
    }
}

pub fn format_percent(value: Option<u8>) -> String {
    match value {
        Some(v) => format!("{v}%"),
        None => missing(),
    }
}

pub fn format_dns(stats: &LiveStats) -> String {
    if stats.dns_servers.is_empty() {
        return missing();
    }
    let extra = stats.dns_servers.len().saturating_sub(2);
    let mut servers = stats
        .dns_servers
        .iter()
        .take(2)
        .cloned()
        .collect::<Vec<_>>();
    if extra > 0 {
        servers.push("…".to_string());
    }
    let servers = servers.join(", ");
    match stats.dns_provider {
        Some("DHCP") | None => servers,
        Some(name) => format!("{name} ({servers})"),
    }
}

pub async fn probe_link(
    generation: u64,
    gateway: Option<String>,
    iface: Option<String>,
    refresh_dns: bool,
) -> LinkProbe {
    let router = async {
        match gateway.as_deref() {
            Some(gw) => icmp_ping(gw).await,
            None => None,
        }
    };
    let dns = async {
        if !refresh_dns {
            return None;
        }
        let iface = iface?;
        tokio::task::spawn_blocking(move || resolvectl_dns(&iface))
            .await
            .ok()
            .flatten()
    };
    let (router_ms, dns_servers) = tokio::join!(router, dns);
    LinkProbe {
        generation,
        router_ms,
        dns_servers,
    }
}

pub async fn ping_internet(generation: u64) -> InternetPingResult {
    let stdout = match run_ping(INTERNET_PING_HOST, INTERNET_PING_COUNT).await {
        Some(stdout) => stdout,
        None => {
            return InternetPingResult {
                generation,
                avg_ms: None,
                loss: None,
            };
        }
    };
    let (avg_ms, loss) = parse_ping_stats(&stdout);
    InternetPingResult {
        generation,
        avg_ms,
        loss,
    }
}

async fn run_ping(host: &str, count: u32) -> Option<String> {
    let count = count.to_string();
    for bin in PING_BINS {
        match tokio::process::Command::new(bin)
            .args(["-n", "-c", &count, "-W", "2", host])
            .output()
            .await
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                if stdout.trim().is_empty() {
                    return None;
                }
                return Some(stdout);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return None,
        }
    }
    None
}

pub fn parse_ping_stats(stdout: &str) -> (Option<f64>, Option<u8>) {
    let mut avg_ms = None;
    let mut loss = None;
    for line in stdout.lines() {
        if loss.is_none()
            && let Some(percent) = line.find('%')
        {
            let prefix = &line[..percent];
            if let Some(num) = prefix
                .rsplit(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
            {
                if let Ok(value) = num.parse::<u8>() {
                    loss = Some(value);
                }
            }
        }
        if avg_ms.is_none() && (line.contains("min/avg/max") || line.contains("round-trip")) {
            if let Some(stats) = line.split('=').nth(1).or_else(|| line.split(':').nth(1)) {
                if let Some(avg) = stats.split('/').nth(1) {
                    let avg = avg.trim().trim_end_matches("ms").trim();
                    avg_ms = avg.parse().ok();
                }
            }
        }
    }
    (avg_ms, loss)
}

async fn icmp_ping(host: &str) -> Option<f64> {
    for bin in PING_BINS {
        match tokio::process::Command::new(bin)
            .args(["-n", "-c", "1", "-W", "1", host])
            .output()
            .await
        {
            Ok(output) if output.status.success() => {
                return parse_ping_ms(&String::from_utf8_lossy(&output.stdout));
            }
            Ok(_) => return None,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return None,
        }
    }
    None
}

fn parse_ping_ms(stdout: &str) -> Option<f64> {
    let idx = stdout.find("time=")?;
    let rest = stdout.get(idx + 5..)?;
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    num.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_ip_is_little_endian() {
        assert_eq!(
            parse_route_ipv4("0101A8C0"),
            Some(Ipv4Addr::new(192, 168, 1, 1))
        );
    }

    #[test]
    fn ping_parser_reads_iputils() {
        let out = "64 bytes from 1.1.1.1: icmp_seq=1 ttl=58 time=12.348 ms";
        assert_eq!(parse_ping_ms(out), Some(12.348));
    }

    #[test]
    fn bytes_and_rates_format() {
        assert_eq!(format_bytes(800), "800 B");
        assert_eq!(format_bytes(12_400), "12.4 KB");
        assert_eq!(format_rate(1_250_000.0), "1.25 MB/s");
    }

    #[test]
    fn dns_provider_detection() {
        assert_eq!(dns_provider_name(&["1.1.1.1".into()]), Some("Cloudflare"));
        assert_eq!(dns_provider_name(&["8.8.8.8".into()]), Some("Google"));
        assert_eq!(dns_provider_name(&["192.168.1.1".into()]), Some("DHCP"));
    }

    #[test]
    fn dns_list_is_capped() {
        let stats = LiveStats {
            dns_servers: vec![
                "1.1.1.1".into(),
                "1.0.0.1".into(),
                "2606:4700:4700::1111".into(),
                "2606:4700:4700::1001".into(),
            ],
            dns_provider: Some("Cloudflare"),
            ..LiveStats::default()
        };
        let formatted = format_dns(&stats);
        assert!(formatted.contains("1.1.1.1"));
        assert!(formatted.contains("…"));
        assert!(!formatted.contains("2606:4700:4700::1001"));
    }

    #[test]
    fn resolvectl_skips_link_labels() {
        let parsed = parse_resolvectl_dns("Link 3 (wlan0): 1.1.1.1#53 8.8.8.8");
        assert_eq!(parsed, Some(vec!["1.1.1.1".into(), "8.8.8.8".into()]));
    }

    #[test]
    fn ping_c3_stats_from_iputils() {
        let out = "\
PING 1.1.1.1 (1.1.1.1) 56(84) bytes of data.
64 bytes from 1.1.1.1: icmp_seq=1 ttl=58 time=12.348 ms
64 bytes from 1.1.1.1: icmp_seq=2 ttl=58 time=13.001 ms
64 bytes from 1.1.1.1: icmp_seq=3 ttl=58 time=11.500 ms

--- 1.1.1.1 ping statistics ---
3 packets transmitted, 3 received, 0% packet loss, time 2004ms
rtt min/avg/max/mdev = 11.500/12.283/13.001/0.612 ms
";
        assert_eq!(parse_ping_stats(out), (Some(12.283), Some(0)));
    }
}
