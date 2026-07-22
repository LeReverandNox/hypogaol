use std::path::Path;
use std::process::Command;

use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// Real subprocess implementation of all three ports (AD-1).
pub struct ExecAdapter;

fn binary_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

/// Confirms LUKS2 FIDO2/hmac-secret support is actually usable, not just that
/// cryptsetup was built with token-plugin support in the abstract.
///
/// Verified empirically in the Nix devShell: `cryptsetup --help` reports "LUKS2
/// external token plugin support is enabled" and prints the exact directory it
/// will search for token plugins — but that directory can be empty. The
/// `libcryptsetup-token-systemd-fido2.so` plugin systemd-cryptenroll relies on
/// (AD-1) instead lives wherever the host's systemd package installs it (e.g.
/// /usr/lib/cryptsetup), which cryptsetup never searches by default. So text-only
/// "support is enabled" is not sufficient; this parses cryptsetup's own reported
/// plugin path and checks the plugin file actually exists there.
fn luks2_fido2_token_plugin_present() -> Result<(), String> {
    let output = Command::new("cryptsetup")
        .arg("--help")
        .output()
        .map_err(|_| "cryptsetup binary present but failed to execute --help".to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if !stdout.contains("LUKS2 external token plugin support is enabled.") {
        return Err("cryptsetup was built without LUKS2 external token plugin support".to_string());
    }

    let plugin_dir = stdout
        .lines()
        .find_map(|line| line.strip_prefix("LUKS2 external token plugin path: "))
        .map(|path| path.trim_end_matches('.'))
        .ok_or_else(|| "cryptsetup did not report an external token plugin path".to_string())?;

    if Path::new(plugin_dir)
        .join("libcryptsetup-token-systemd-fido2.so")
        .is_file()
    {
        Ok(())
    } else {
        Err(format!(
            "systemd-fido2 LUKS2 token plugin (libcryptsetup-token-systemd-fido2.so) not found in cryptsetup's external token plugin path: {plugin_dir}"
        ))
    }
}

impl LuksBackend for ExecAdapter {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        let mut missing = Vec::new();

        if !binary_on_path("cryptsetup") {
            missing.push("cryptsetup binary not found on PATH".to_string());
        } else if let Err(err) = luks2_fido2_token_plugin_present() {
            missing.push(err);
        }

        if !binary_on_path("systemd-cryptenroll") {
            missing.push("systemd-cryptenroll binary not found on PATH".to_string());
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

impl Fido2Backend for ExecAdapter {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        let mut missing = Vec::new();

        if !binary_on_path("fido2-token") {
            missing.push("fido2-token binary not found on PATH".to_string());
        }
        if !Path::new("/sys/class/hidraw").is_dir() {
            missing.push("kernel hidraw support not found (/sys/class/hidraw missing)".to_string());
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

impl FilesystemBackend for ExecAdapter {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        let mut missing = Vec::new();

        for binary in ["mkfs.ext4", "resize2fs", "blockdev"] {
            if !binary_on_path(binary) {
                missing.push(format!("{binary} binary not found on PATH"));
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}
