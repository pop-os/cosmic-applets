//! Support for NetworkManager VPN plugin "auth-dialog" helpers.
//!
//! Some VPN plugins (notably openconnect / Cisco AnyConnect) authenticate
//! through an external helper binary that owns the entire login UX — including
//! the WebKit browser window used for SAML. NetworkManager's reference agents
//! (nm-applet, GNOME Shell) spawn this helper instead of prompting for a
//! password. This module reimplements that protocol so the COSMIC applet can
//! support SAML-based VPNs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use zbus::zvariant::OwnedValue;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// VPN service-types routed through the plugin auth-dialog instead of the
/// applet's native password prompt.
///
/// openconnect decides password-vs-SAML at runtime by contacting the server,
/// so the helper must own the flow; we cannot know from static config whether
/// a given connection will need a browser. The single openconnect entry also
/// covers GlobalProtect and Pulse, which use the same NM service-type. Add
/// other interactive plugins here as needed.
pub const AUTH_DIALOG_SERVICE_TYPES: &[&str] = &["org.freedesktop.NetworkManager.openconnect"];

/// Whether a VPN of this service-type should be authenticated via the plugin
/// auth-dialog rather than the applet's built-in password field.
pub fn needs_auth_dialog(service_type: &str) -> bool {
    AUTH_DIALOG_SERVICE_TYPES.contains(&service_type)
}

/// Directories scanned for VPN plugin `.name` files, in priority order.
///
/// Mirrors libnm's search path (`/usr/lib/NetworkManager/VPN`,
/// `/etc/NetworkManager/VPN`), plus a colon-separated `$NM_VPN_PLUGIN_DIR`
/// override and the NixOS current-system location, where plugins live under a
/// `/nix/store` path rather than the FHS defaults.
pub fn plugin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(env_dirs) = std::env::var("NM_VPN_PLUGIN_DIR") {
        for dir in env_dirs.split(':').filter(|s| !s.is_empty()) {
            dirs.push(PathBuf::from(dir));
        }
    }
    for fixed in [
        "/etc/NetworkManager/VPN",
        "/usr/lib/NetworkManager/VPN",
        "/usr/lib64/NetworkManager/VPN",
        "/run/current-system/sw/lib/NetworkManager/VPN", // NixOS
        "/run/booted-system/sw/lib/NetworkManager/VPN",  // NixOS
    ] {
        dirs.push(PathBuf::from(fixed));
    }
    dirs
}

/// A parsed VPN plugin `.name` description file (GKeyFile / INI).
#[derive(Debug, Clone)]
pub struct NameEntry {
    /// `[VPN Connection] service` — the D-Bus service-type, matched against
    /// the connection's `vpn.service-type`.
    pub service: String,
    /// `[GNOME] auth-dialog` — absolute path to the helper binary.
    pub auth_dialog: Option<PathBuf>,
}

/// Parse a `.name` file, extracting the VPN service-type and auth-dialog path.
///
/// Only `[VPN Connection] service` and `[GNOME] auth-dialog` are read. A
/// `[GNOME] service` key (legacy/unused) is deliberately ignored.
pub fn parse_name_file(content: &str) -> Option<NameEntry> {
    let mut group = String::new();
    let mut service = None;
    let mut auth_dialog = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            group = name.to_string();
        } else if let Some((key, value)) = line.split_once('=') {
            let (key, value) = (key.trim(), value.trim());
            match (group.as_str(), key) {
                ("VPN Connection", "service") => service = Some(value.to_string()),
                ("GNOME", "auth-dialog") => auth_dialog = Some(PathBuf::from(value)),
                _ => {}
            }
        }
    }

    Some(NameEntry {
        service: service?,
        auth_dialog,
    })
}

/// Locate the auth-dialog binary for a VPN service-type by scanning the plugin
/// directories for a `.name` file whose `[VPN Connection] service` matches.
/// Returns the first match whose auth-dialog path exists on disk.
pub fn find_auth_dialog(service_type: &str) -> Option<PathBuf> {
    for dir in plugin_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("name") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some(parsed) = parse_name_file(&content) else {
                continue;
            };
            if parsed.service == service_type
                && let Some(auth_dialog) = parsed.auth_dialog
                && auth_dialog.exists()
            {
                return Some(auth_dialog);
            }
        }
    }
    None
}

/// Build the auth-dialog command-line arguments (openconnect option set:
/// `-u -n -s -i -r`). No `-t`/`--external-ui-mode`: the openconnect helper's
/// getopt table rejects them, and we only route openconnect through here.
pub fn build_argv(
    uuid: &str,
    id: &str,
    service_type: &str,
    allow_interaction: bool,
    request_new: bool,
) -> Vec<String> {
    let mut argv = vec![
        "-u".to_string(),
        uuid.to_string(),
        "-n".to_string(),
        id.to_string(),
        "-s".to_string(),
        service_type.to_string(),
    ];
    if allow_interaction {
        argv.push("-i".to_string());
    }
    if request_new {
        argv.push("-r".to_string());
    }
    argv
}

/// Parse the auth-dialog's stdout into a secrets map.
///
/// The helper writes alternating `key\nvalue\n` lines followed by a blank line.
/// We split on `\n` and consume pairs, stopping at the first empty key line
/// (the terminator) — matching nm-applet's `process_child_response`.
pub fn parse_stdout(output: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut lines = output.split('\n');
    while let Some(key) = lines.next() {
        if key.is_empty() {
            break;
        }
        let Some(value) = lines.next() else { break };
        map.insert(key.to_string(), value.to_string());
    }
    map
}

/// Serialize the connection's VPN `data` and existing `secrets` into the exact
/// byte format the auth-dialog reads from stdin. Values have embedded newlines
/// replaced with spaces (matching nm-applet's `connection_to_data`). The buffer
/// ends with the literal `DONE\n\nQUIT\n\n` terminator; the caller writes it in
/// one shot and closes stdin.
pub fn serialize_stdin(data: &[(String, String)], secrets: &[(String, String)]) -> String {
    fn sanitize(value: &str) -> String {
        value.replace('\n', " ")
    }
    let mut buf = String::new();
    for (key, value) in data {
        buf.push_str("DATA_KEY=");
        buf.push_str(&sanitize(key));
        buf.push('\n');
        buf.push_str("DATA_VAL=");
        buf.push_str(&sanitize(value));
        buf.push('\n');
    }
    for (key, value) in secrets {
        buf.push_str("SECRET_KEY=");
        buf.push_str(&sanitize(key));
        buf.push('\n');
        buf.push_str("SECRET_VAL=");
        buf.push_str(&sanitize(value));
        buf.push('\n');
    }
    buf.push_str("DONE\n\nQUIT\n\n");
    buf
}

/// Failure modes when driving a VPN auth-dialog helper.
#[derive(Debug)]
pub enum AuthDialogError {
    /// The helper binary could not be spawned.
    Spawn(std::io::Error),
    /// An I/O error occurred writing stdin or reading stdout.
    Io(std::io::Error),
    /// The helper exited non-zero — treated as user-canceled.
    Canceled,
    /// The child's stdin/stdout pipe was unexpectedly absent.
    MissingPipe,
}

impl std::fmt::Display for AuthDialogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthDialogError::Spawn(e) => write!(f, "failed to spawn auth-dialog: {e}"),
            AuthDialogError::Io(e) => write!(f, "auth-dialog I/O error: {e}"),
            AuthDialogError::Canceled => write!(f, "auth-dialog canceled or failed"),
            AuthDialogError::MissingPipe => write!(f, "auth-dialog pipe missing"),
        }
    }
}

impl std::error::Error for AuthDialogError {}

/// Spawn a VPN plugin auth-dialog helper, feed it the connection data/secrets,
/// and collect the secrets it returns.
///
/// The helper owns its own login UI (for openconnect SAML it opens a WebKit
/// browser window), so this future resolves only once the user completes or
/// cancels the login. `kill_on_drop` ensures the helper is torn down if the
/// surrounding task is dropped (e.g. NetworkManager cancels the request).
#[allow(clippy::too_many_arguments)]
pub async fn run_auth_dialog(
    auth_dialog: &Path,
    uuid: &str,
    id: &str,
    service_type: &str,
    allow_interaction: bool,
    request_new: bool,
    vpn_data: &[(String, String)],
    existing_secrets: &[(String, String)],
) -> Result<HashMap<String, String>, AuthDialogError> {
    let argv = build_argv(uuid, id, service_type, allow_interaction, request_new);

    let mut child = Command::new(auth_dialog)
        .args(&argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        // GTK debug output on stdout would corrupt the protocol stream.
        .env_remove("G_MESSAGES_DEBUG")
        // The auth-dialog is an independent system binary that spawns its own
        // WebKit subprocesses (which dlopen EGL/GL). Our LD_LIBRARY_PATH — set
        // when running an unpackaged build so the applet finds its own libs —
        // is wrong for the helper and makes those subprocesses load mismatched
        // graphics libraries and abort. Let it resolve its own libraries.
        // (No-op for a normally-packaged applet, where LD_LIBRARY_PATH is unset.)
        .env_remove("LD_LIBRARY_PATH")
        // openconnect's auth-dialog renders the SAML login in an embedded
        // WebKitGTK view. WebKitGTK's native-Wayland path fails to paint its
        // web content under COSMIC's compositor (the window and chrome show,
        // but the page area stays blank), so force the helper onto XWayland,
        // where WebKitGTK rendering is reliable. Scoped to this child only;
        // XWayland is always available and this applet only runs under COSMIC.
        .env("GDK_BACKEND", "x11")
        .kill_on_drop(true)
        .spawn()
        .map_err(AuthDialogError::Spawn)?;

    let stdin_buf = serialize_stdin(vpn_data, existing_secrets);
    {
        let mut stdin = child.stdin.take().ok_or(AuthDialogError::MissingPipe)?;
        stdin
            .write_all(stdin_buf.as_bytes())
            .await
            .map_err(AuthDialogError::Io)?;
        stdin.flush().await.map_err(AuthDialogError::Io)?;
        // `stdin` dropped here → EOF for the helper.
    }

    let mut stdout = child.stdout.take().ok_or(AuthDialogError::MissingPipe)?;
    let mut output = String::new();
    stdout
        .read_to_string(&mut output)
        .await
        .map_err(AuthDialogError::Io)?;

    let status = child.wait().await.map_err(AuthDialogError::Io)?;
    if !status.success() {
        return Err(AuthDialogError::Canceled);
    }

    Ok(parse_stdout(&output))
}

/// Fetch the connection's `vpn.data` (the non-secret plugin config, e.g.
/// `gateway`, `protocol`, `authtype`) via `Settings.Connection.GetSettings`.
///
/// The auth-dialog needs this to know which server to reach and how to
/// authenticate. Returns an empty vec on any error; the helper still runs and
/// can prompt for whatever it needs.
pub async fn fetch_vpn_data(
    conn: &zbus::Connection,
    connection_path: &str,
) -> Vec<(String, String)> {
    async fn inner(
        conn: &zbus::Connection,
        connection_path: &str,
    ) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
        let proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.NetworkManager",
            connection_path,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?;

        // Signature a{sa{sv}}: setting name -> (key -> value).
        let settings: HashMap<String, HashMap<String, OwnedValue>> =
            proxy.call("GetSettings", &()).await?;

        let mut out = Vec::new();
        if let Some(vpn) = settings.get("vpn")
            && let Some(data) = vpn.get("data")
        {
            // vpn.data is itself an a{ss} map wrapped in a variant.
            let map = <HashMap<String, String>>::try_from(data.clone())?;
            out.extend(map);
        }
        Ok(out)
    }

    match inner(conn, connection_path).await {
        Ok(data) => data,
        Err(e) => {
            tracing::warn!("failed to fetch vpn.data for {connection_path}: {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENCONNECT_NAME: &str = "\
[VPN Connection]
name=openconnect
service=org.freedesktop.NetworkManager.openconnect
program=/nix/store/abc/libexec/nm-openconnect-service
supports-multiple-connections=true

[libnm]
plugin=/nix/store/abc/lib/NetworkManager/libnm-vpn-plugin-openconnect.so

[GNOME]
auth-dialog=/nix/store/abc/libexec/nm-openconnect-auth-dialog
properties=/nix/store/abc/lib/NetworkManager/libnm-openconnect-properties
";

    #[test]
    fn parses_service_and_auth_dialog() {
        let entry = parse_name_file(OPENCONNECT_NAME).expect("parse");
        assert_eq!(entry.service, "org.freedesktop.NetworkManager.openconnect");
        assert_eq!(
            entry.auth_dialog.as_deref(),
            Some(std::path::Path::new(
                "/nix/store/abc/libexec/nm-openconnect-auth-dialog"
            ))
        );
    }

    #[test]
    fn ignores_gnome_service_key() {
        // A stray [GNOME] service= must NOT be treated as the VPN service.
        let content = "[VPN Connection]\nservice=real.svc\n[GNOME]\nservice=legacy.gui\nauth-dialog=/bin/x\n";
        let entry = parse_name_file(content).expect("parse");
        assert_eq!(entry.service, "real.svc");
    }

    #[test]
    fn needs_auth_dialog_matches_openconnect_only() {
        assert!(needs_auth_dialog("org.freedesktop.NetworkManager.openconnect"));
        assert!(!needs_auth_dialog("org.freedesktop.NetworkManager.openvpn"));
    }

    #[test]
    fn plugin_dirs_honors_env_override() {
        // SAFETY: single-threaded test; restore after.
        unsafe { std::env::set_var("NM_VPN_PLUGIN_DIR", "/custom/a:/custom/b") };
        let dirs = plugin_dirs();
        unsafe { std::env::remove_var("NM_VPN_PLUGIN_DIR") };
        assert_eq!(dirs[0], std::path::PathBuf::from("/custom/a"));
        assert_eq!(dirs[1], std::path::PathBuf::from("/custom/b"));
        assert!(dirs.contains(&std::path::PathBuf::from("/etc/NetworkManager/VPN")));
    }

    #[test]
    fn argv_openconnect_interactive() {
        let argv = build_argv(
            "uuid-1",
            "Work VPN",
            "org.freedesktop.NetworkManager.openconnect",
            true,
            false,
        );
        assert_eq!(
            argv,
            vec![
                "-u", "uuid-1",
                "-n", "Work VPN",
                "-s", "org.freedesktop.NetworkManager.openconnect",
                "-i",
            ]
        );
    }

    #[test]
    fn argv_adds_reprompt_flag() {
        let argv = build_argv("u", "n", "s", true, true);
        assert!(argv.ends_with(&["-i".to_string(), "-r".to_string()]));
    }

    #[test]
    fn serialize_stdin_exact_wire_format() {
        let data = vec![
            ("gateway".to_string(), "vpn.example.com".to_string()),
            ("protocol".to_string(), "anyconnect".to_string()),
        ];
        let secrets = vec![("cookie".to_string(), "old".to_string())];
        let out = serialize_stdin(&data, &secrets);
        assert_eq!(
            out,
            "DATA_KEY=gateway\nDATA_VAL=vpn.example.com\n\
             DATA_KEY=protocol\nDATA_VAL=anyconnect\n\
             SECRET_KEY=cookie\nSECRET_VAL=old\n\
             DONE\n\nQUIT\n\n"
        );
    }

    #[test]
    fn serialize_stdin_replaces_newlines_in_values() {
        let data = vec![("k".to_string(), "line1\nline2".to_string())];
        let out = serialize_stdin(&data, &[]);
        assert_eq!(out, "DATA_KEY=k\nDATA_VAL=line1 line2\nDONE\n\nQUIT\n\n");
    }

    #[test]
    fn parse_stdout_reads_key_value_pairs() {
        let out = "gateway\nhttps://vpn.example.com\ncookie\nSESSIONXYZ\n\n\n";
        let map = parse_stdout(out);
        assert_eq!(map.get("gateway").map(String::as_str), Some("https://vpn.example.com"));
        assert_eq!(map.get("cookie").map(String::as_str), Some("SESSIONXYZ"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn parse_stdout_stops_at_first_empty_key() {
        // Anything after the blank terminator line is ignored.
        let out = "cookie\nABC\n\ntrailing\ngarbage\n";
        let map = parse_stdout(out);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("cookie").map(String::as_str), Some("ABC"));
    }

    #[test]
    fn parse_stdout_allows_empty_value() {
        let out = "resolve\n\ncookie\nABC\n\n\n";
        let map = parse_stdout(out);
        // `resolve` has an empty value; parsing continues to `cookie`.
        assert_eq!(map.get("resolve").map(String::as_str), Some(""));
        assert_eq!(map.get("cookie").map(String::as_str), Some("ABC"));
    }

    #[test]
    fn parse_stdout_empty_input_is_empty_map() {
        assert!(parse_stdout("").is_empty());
    }

    #[tokio::test]
    async fn run_auth_dialog_round_trips_with_mock_helper() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        // Mock helper: consume stdin, then emit two secrets + blank terminator.
        let dir = std::env::temp_dir().join(format!("vpn_auth_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("mock-auth-dialog.sh");
        {
            let mut f = std::fs::File::create(&script).unwrap();
            writeln!(
                f,
                "#!/bin/sh\ncat >/dev/null\nprintf 'gateway\\nhttps://vpn.example.com\\ncookie\\nSESSIONXYZ\\n\\n\\n'"
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let data = vec![("gateway".to_string(), "vpn.example.com".to_string())];
        let secrets = run_auth_dialog(
            &script,
            "uuid-1",
            "Work VPN",
            "org.freedesktop.NetworkManager.openconnect",
            true,
            false,
            &data,
            &[],
        )
        .await
        .expect("helper should succeed");

        assert_eq!(secrets.get("cookie").map(String::as_str), Some("SESSIONXYZ"));
        assert_eq!(
            secrets.get("gateway").map(String::as_str),
            Some("https://vpn.example.com")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn run_auth_dialog_nonzero_exit_is_canceled() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("vpn_auth_cancel_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fail.sh");
        {
            let mut f = std::fs::File::create(&script).unwrap();
            writeln!(f, "#!/bin/sh\ncat >/dev/null\nexit 1").unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let err = run_auth_dialog(&script, "u", "n", "s", true, false, &[], &[])
            .await
            .unwrap_err();
        assert!(matches!(err, AuthDialogError::Canceled));
        std::fs::remove_dir_all(&dir).ok();
    }
}
