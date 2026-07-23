use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use zeroize::Zeroizing;

use crate::domain::errors::DomainError;
use crate::domain::types::{Filesystem, KeyMetadata, KeyslotInfo, KeyslotRef, MapperHandle};
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// Real subprocess implementation of all three ports (AD-1).
///
/// Caches the transient bootstrap passphrase (AD-3/AD-9) between
/// `bootstrap_format_and_open` and `enroll_fido2_key` — `systemd-cryptenroll`
/// can only add a keyslot by authenticating with a still-valid existing
/// credential, so the passphrase must survive across those two port calls.
/// It never crosses into `domain`, which only ever sees `&dyn LuksBackend`.
#[derive(Default)]
pub struct ExecAdapter {
    transient_passphrase: RefCell<Option<Zeroizing<String>>>,
}

fn binary_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

/// A `Command` for `program`, run under `sudo`. Used only for the handful of
/// operations that genuinely need root (device-mapper: `luksOpen`, `close`,
/// `mkfs` on the resulting mapper device) — everything else in this adapter
/// (file allocation, `luksFormat`, FIDO2 enrollment, token/keyslot cleanup)
/// operates on the LUKS2 header file directly and needs no elevation.
/// `sudo` prompts interactively via the controlling terminal exactly when
/// reached, rather than requiring the whole process to run as root.
fn privileged(program: &str) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg(program);
    cmd
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

/// Fills a fresh 32-byte buffer from the OS CSPRNG and hex-encodes it in
/// place, so the raw secret bytes never exist outside a `Zeroizing` wrapper.
/// Hex (rather than the raw bytes) sidesteps any ambiguity in how cryptsetup
/// reads a `--key-file -` payload containing embedded newlines/binary data.
fn generate_transient_passphrase() -> Result<Zeroizing<String>, String> {
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

    let mut bytes = Zeroizing::new([0u8; 32]);
    getrandom::fill(&mut *bytes)
        .map_err(|e| format!("failed to generate transient bootstrap passphrase: {e}"))?;

    let mut hex = Zeroizing::new(String::with_capacity(bytes.len() * 2));
    for &byte in bytes.iter() {
        hex.push(HEX_DIGITS[(byte >> 4) as usize] as char);
        hex.push(HEX_DIGITS[(byte & 0x0f) as usize] as char);
    }
    Ok(hex)
}

/// A temporary file holding the transient bootstrap passphrase, for
/// `systemd-cryptenroll --unlock-key-file`. Created with mode 0600, in
/// `/dev/shm` when available (tmpfs — never touches a disk-backed
/// filesystem), falling back to the system temp dir otherwise. Its `Drop`
/// impl best-effort-overwrites the file before unlinking it, so the
/// passphrase's on-disk lifetime is as short as this struct's scope.
struct TempKeyFile {
    path: PathBuf,
}

impl TempKeyFile {
    fn create(passphrase: &[u8]) -> Result<Self, String> {
        use std::os::unix::fs::OpenOptionsExt;

        let dir = if Path::new("/dev/shm").is_dir() {
            PathBuf::from("/dev/shm")
        } else {
            std::env::temp_dir()
        };

        let mut suffix = [0u8; 8];
        getrandom::fill(&mut suffix)
            .map_err(|e| format!("failed to generate temporary key file name: {e}"))?;
        let suffix_hex = suffix
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();

        let path = dir.join(format!(".tomb-fido2-bootstrap-{suffix_hex}"));

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| {
                format!(
                    "failed to create temporary key file {}: {e}",
                    path.display()
                )
            })?;

        file.write_all(passphrase)
            .map_err(|e| format!("failed to write temporary key file {}: {e}", path.display()))?;

        Ok(Self { path })
    }
}

impl Drop for TempKeyFile {
    fn drop(&mut self) {
        if let Ok(metadata) = std::fs::metadata(&self.path) {
            if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(&self.path) {
                let zeros = vec![0u8; metadata.len() as usize];
                let _ = file.write_all(&zeros);
                let _ = file.sync_all();
            }
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Runs `cmd` piping `input` to its stdin, capturing stdout/stderr (only
/// surfaced in the returned `Err`). Used for calls with no interactive
/// component — `input` may be secret (the transient passphrase) or not (a
/// token JSON payload); either way it is never logged or echoed.
fn run_piping_stdin(cmd: &mut Command, input: &[u8]) -> Result<(), String> {
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn {cmd:?}: {e}"))?;
    child
        .stdin
        .take()
        .expect("stdin was requested as piped")
        .write_all(input)
        .map_err(|e| format!("failed writing to {cmd:?}'s stdin: {e}"))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for {cmd:?}: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{cmd:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn dump_json_metadata(path: &Path) -> Result<Value, DomainError> {
    let output = Command::new("cryptsetup")
        .arg("luksDump")
        .arg("--dump-json-metadata")
        .arg(path)
        .output()
        .map_err(|e| {
            DomainError::AdapterFailure(format!("failed to run cryptsetup luksDump: {e}"))
        })?;

    if !output.status.success() {
        return Err(DomainError::AdapterFailure(format!(
            "cryptsetup luksDump --dump-json-metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| DomainError::AdapterFailure(format!("failed to parse luksDump JSON: {e}")))
}

fn tokens_object(metadata: &Value) -> Result<&serde_json::Map<String, Value>, DomainError> {
    metadata
        .get("tokens")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            DomainError::AdapterFailure("luksDump JSON missing a tokens object".to_string())
        })
}

fn find_systemd_fido2_token_id(path: &Path) -> Result<String, DomainError> {
    let metadata = dump_json_metadata(path)?;
    let tokens = tokens_object(&metadata)?;

    tokens
        .iter()
        .find(|(_, token)| token.get("type").and_then(Value::as_str) == Some("systemd-fido2"))
        .map(|(id, _)| id.clone())
        .ok_or_else(|| {
            DomainError::AdapterFailure("no systemd-fido2 token found after enrollment".to_string())
        })
}

impl ExecAdapter {
    /// Writes `metadata`'s fields directly onto the `systemd-fido2` token
    /// `systemd-cryptenroll` just created (AD-2 — confirmed by Story 1.5's
    /// Task 1 spike that the plugin tolerates these extra fields).
    fn write_fido2_token_metadata(
        &self,
        path: &Path,
        metadata: KeyMetadata,
    ) -> Result<(), DomainError> {
        let token_id = find_systemd_fido2_token_id(path)?;

        let export = Command::new("cryptsetup")
            .args(["token", "export", "--token-id", &token_id])
            .arg(path)
            .output()
            .map_err(|e| {
                DomainError::AdapterFailure(format!("failed to run cryptsetup token export: {e}"))
            })?;

        if !export.status.success() {
            return Err(DomainError::AdapterFailure(format!(
                "cryptsetup token export failed: {}",
                String::from_utf8_lossy(&export.stderr).trim()
            )));
        }

        let mut token: Value = serde_json::from_slice(&export.stdout).map_err(|e| {
            DomainError::AdapterFailure(format!("failed to parse exported token JSON: {e}"))
        })?;

        // systemd-cryptenroll already wrote this at enroll time; it is the
        // FIDO2 credential ID, non-secret (AD-3).
        let credential_id = token
            .get("fido2-credential")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());

        let filesystem_name = match metadata.filesystem {
            Filesystem::Ext4 => "ext4",
        };

        let object = token.as_object_mut().ok_or_else(|| {
            DomainError::AdapterFailure("exported token JSON was not an object".to_string())
        })?;

        object.insert("key_label".to_string(), Value::String(metadata.key_label));
        object.insert(
            "filesystem".to_string(),
            Value::String(filesystem_name.to_string()),
        );
        object.insert("credential_id".to_string(), Value::String(credential_id));
        object.insert("created_at".to_string(), Value::String(created_at));

        let payload = serde_json::to_vec(&token).map_err(|e| {
            DomainError::AdapterFailure(format!("failed to serialize updated token JSON: {e}"))
        })?;

        run_piping_stdin(
            Command::new("cryptsetup")
                .args([
                    "token",
                    "import",
                    "--token-id",
                    &token_id,
                    "--token-replace",
                ])
                .arg(path),
            &payload,
        )
        .map_err(DomainError::AdapterFailure)
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

        if !binary_on_path("sudo") {
            missing.push(
                "sudo binary not found on PATH (needed to elevate privileges for luksOpen/close)"
                    .to_string(),
            );
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }

    fn bootstrap_format_and_open(
        &self,
        path: &Path,
        name: &str,
        filesystem: Filesystem,
    ) -> Result<MapperHandle, DomainError> {
        // v1 has only one Filesystem variant; luksFormat/luksOpen don't need
        // to know which one — mkfs (a separate port call) is what cares.
        let _ = filesystem;

        let passphrase = generate_transient_passphrase().map_err(DomainError::AdapterFailure)?;

        run_piping_stdin(
            Command::new("cryptsetup")
                .args([
                    "luksFormat",
                    "--type",
                    "luks2",
                    "--batch-mode",
                    "--key-file",
                    "-",
                ])
                .arg(path),
            passphrase.as_bytes(),
        )
        .map_err(DomainError::AdapterFailure)?;

        run_piping_stdin(
            privileged("cryptsetup")
                .args(["luksOpen", "--key-file", "-"])
                .arg(path)
                .arg(name),
            passphrase.as_bytes(),
        )
        .map_err(DomainError::AdapterFailure)?;

        // Not wiped yet: enroll_fido2_key still needs it to authenticate
        // adding the real key's keyslot. Cached here, never surfaced to
        // `domain` (AD-3/AD-9).
        *self.transient_passphrase.borrow_mut() = Some(passphrase);

        Ok(MapperHandle {
            name: name.to_string(),
            source_path: path.to_path_buf(),
        })
    }

    fn enroll_fido2_key(
        &self,
        mapper: &MapperHandle,
        metadata: KeyMetadata,
    ) -> Result<(), DomainError> {
        let passphrase = self
            .transient_passphrase
            .borrow_mut()
            .take()
            .ok_or_else(|| {
                DomainError::AdapterFailure(
                    "no transient bootstrap passphrase available to authenticate FIDO2 enrollment"
                        .to_string(),
                )
            })?;

        // `systemd-cryptenroll` doesn't read a piped (non-tty) stdin as a
        // passphrase the way `cryptsetup` does — confirmed empirically: it
        // instead falls back to systemd's ask-password broadcast/agent
        // mechanism and hangs waiting for an agent. `--unlock-key-file` is
        // the documented non-interactive path instead, so the passphrase is
        // written to a tightly-permissioned, promptly-deleted temp file.
        let key_file =
            TempKeyFile::create(passphrase.as_bytes()).map_err(DomainError::AdapterFailure)?;

        // The in-memory passphrase is dropped (zeroized) here — strictly
        // before `create::run` goes on to call `mkfs` (AC #3, AD-3). The
        // on-disk copy in `key_file` is wiped by its own Drop impl once this
        // function returns.
        drop(passphrase);

        // stdin/stdout/stderr all stay inherited: the unlock credential now
        // travels via --unlock-key-file, so the user's terminal is free to
        // handle systemd-cryptenroll's own FIDO2 touch/PIN prompt normally.
        let status = Command::new("systemd-cryptenroll")
            .arg("--fido2-device=auto")
            .arg(format!("--unlock-key-file={}", key_file.path.display()))
            .arg(&mapper.source_path)
            .status()
            .map_err(|e| {
                DomainError::AdapterFailure(format!("failed to run systemd-cryptenroll: {e}"))
            })?;

        if !status.success() {
            return Err(DomainError::AdapterFailure(
                "systemd-cryptenroll --fido2-device=auto failed".to_string(),
            ));
        }

        self.write_fido2_token_metadata(&mapper.source_path, metadata)
    }

    fn list_fido2_keyslots(&self, path: &Path) -> Result<Vec<KeyslotInfo>, DomainError> {
        let metadata = dump_json_metadata(path)?;
        let tokens = tokens_object(&metadata)?;

        let mut keyslots = Vec::new();
        for token in tokens.values() {
            if token.get("type").and_then(Value::as_str) != Some("systemd-fido2") {
                continue;
            }
            let Some(token_keyslots) = token.get("keyslots").and_then(Value::as_array) else {
                continue;
            };
            for slot in token_keyslots {
                if let Some(slot_num) = slot.as_str().and_then(|s| s.parse::<u32>().ok()) {
                    keyslots.push(KeyslotInfo {
                        keyslot: KeyslotRef(slot_num),
                    });
                }
            }
        }

        Ok(keyslots)
    }

    fn remove_key(&self, path: &Path, keyslot: KeyslotRef) -> Result<(), DomainError> {
        let metadata = dump_json_metadata(path)?;
        let slot_str = keyslot.0.to_string();

        let token_id = tokens_object(&metadata)?
            .iter()
            .find(|(_, token)| {
                token
                    .get("keyslots")
                    .and_then(Value::as_array)
                    .is_some_and(|ks| ks.iter().any(|s| s.as_str() == Some(slot_str.as_str())))
            })
            .map(|(id, _)| id.clone());

        // Token metadata removed first, keyslot second (AD-5's crash-safe
        // ordering) — a keyslot with no token is orphaned-but-safe; a token
        // with no keyslot would let a later count overcount live keys.
        if let Some(token_id) = token_id {
            let output = Command::new("cryptsetup")
                .args(["token", "remove", "--token-id", &token_id])
                .arg(path)
                .output()
                .map_err(|e| {
                    DomainError::AdapterFailure(format!(
                        "failed to run cryptsetup token remove: {e}"
                    ))
                })?;

            if !output.status.success() {
                return Err(DomainError::AdapterFailure(format!(
                    "cryptsetup token remove failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
        }

        // `--batch-mode` with no passphrase supplied removes the keyslot
        // unconditionally (no re-authentication needed, per
        // cryptsetup-luksKillSlot(8)).
        let output = Command::new("cryptsetup")
            .args(["luksKillSlot", "--batch-mode"])
            .arg(path)
            .arg(slot_str)
            .output()
            .map_err(|e| {
                DomainError::AdapterFailure(format!("failed to run cryptsetup luksKillSlot: {e}"))
            })?;

        if output.status.success() {
            Ok(())
        } else {
            Err(DomainError::AdapterFailure(format!(
                "cryptsetup luksKillSlot failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    fn close(&self, mapper: &MapperHandle) -> Result<(), DomainError> {
        let output = privileged("cryptsetup")
            .arg("close")
            .arg(&mapper.name)
            .output()
            .map_err(|e| {
                DomainError::AdapterFailure(format!("failed to run cryptsetup close: {e}"))
            })?;

        if output.status.success() {
            Ok(())
        } else {
            Err(DomainError::AdapterFailure(format!(
                "cryptsetup close failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
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

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn set_backing_file_size(&self, path: &Path, size: u64) -> Result<(), DomainError> {
        let file = std::fs::File::create(path).map_err(|e| {
            DomainError::AdapterFailure(format!("failed to create {}: {e}", path.display()))
        })?;
        file.set_len(size).map_err(|e| {
            DomainError::AdapterFailure(format!("failed to size {}: {e}", path.display()))
        })?;
        Ok(())
    }

    fn mkfs(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<(), DomainError> {
        match fs {
            Filesystem::Ext4 => {
                let output = privileged("mkfs.ext4")
                    .arg("-F")
                    .arg(mapper.device_node())
                    .output()
                    .map_err(|e| {
                        DomainError::AdapterFailure(format!("failed to run mkfs.ext4: {e}"))
                    })?;

                if output.status.success() {
                    Ok(())
                } else {
                    Err(DomainError::AdapterFailure(format!(
                        "mkfs.ext4 failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )))
                }
            }
        }
    }
}
