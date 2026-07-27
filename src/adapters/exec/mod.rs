use std::cell::RefCell;
use std::collections::HashSet;
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use zeroize::Zeroizing;

use crate::domain::errors::DomainError;
use crate::domain::types::{Filesystem, KeyMetadata, KeyslotInfo, KeyslotRef, MapperHandle};
use crate::ports::fido2_backend::{Fido2Backend, Fido2DeviceSelection};
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

/// Hex-encodes 8 random bytes — used to keep sibling generated names (temp
/// key files, mount-point directories) from colliding across concurrent runs,
/// without embedding any secret.
fn random_hex_suffix() -> Result<String, String> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| format!("failed to generate random suffix: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// The real (non-root) user invoking this process — uid, gid, and username,
/// each read via a plain unprivileged `id` query. `tomb_fido2` always runs
/// unprivileged itself (only specific calls escalate via `privileged()`), so
/// these already report the real invoker, not root; never wrap them in
/// `privileged()`, which would prompt `sudo` for information the process
/// already has.
struct InvokingIdentity {
    uid: String,
    gid: String,
    username: String,
}

fn invoking_identity() -> Result<InvokingIdentity, DomainError> {
    fn run_id(flag: &str) -> Result<String, DomainError> {
        let output = Command::new("id")
            .arg(flag)
            .output()
            .map_err(|e| DomainError::AdapterFailure(format!("failed to run id {flag}: {e}")))?;

        if !output.status.success() {
            return Err(DomainError::AdapterFailure(format!(
                "id {flag} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    Ok(InvokingIdentity {
        uid: run_id("-u")?,
        gid: run_id("-g")?,
        username: run_id("-un")?,
    })
}

/// Creates a fresh directory named `tomb_name` under `base` (AD-12: still
/// deterministic from `mapper.source_path` on the common path — no registry).
/// Falls back to a short random-suffixed name only on an actual collision,
/// bounded to a small number of attempts.
fn create_mount_point(base: &Path, tomb_name: &str) -> Result<PathBuf, DomainError> {
    const MAX_ATTEMPTS: u32 = 3;
    let mut candidate = base.join(tomb_name);
    let mut attempt = 1;

    loop {
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < MAX_ATTEMPTS => {
                let suffix = random_hex_suffix().map_err(DomainError::AdapterFailure)?;
                candidate = base.join(format!("{tomb_name}-{}", &suffix[..4]));
                attempt += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(DomainError::AdapterFailure(format!(
                    "could not find an available mount point under {} after {MAX_ATTEMPTS} attempts",
                    base.display()
                )));
            }
            Err(e) => {
                return Err(DomainError::AdapterFailure(format!(
                    "failed to create mount point {}: {e}",
                    candidate.display()
                )));
            }
        }
    }
}

/// The raw byte capacity of the storage backing `path` — a block device's
/// full size via `blockdev --getsize64`, or a regular file's own length.
/// Used by `bootstrap_format_and_open` to decide whether its `resize` step is
/// needed at all: resizing a LUKS2 mapping to exactly the full raw capacity
/// fails outright (there's no room left for the header alongside a
/// full-size payload), confirmed empirically on real hardware.
fn actual_raw_size(path: &Path) -> Result<u64, String> {
    use std::os::unix::fs::FileTypeExt;

    let metadata =
        std::fs::metadata(path).map_err(|e| format!("failed to stat {}: {e}", path.display()))?;

    if !metadata.file_type().is_block_device() {
        return Ok(metadata.len());
    }

    let output = Command::new("blockdev")
        .arg("--getsize64")
        .arg(path)
        .output()
        .map_err(|e| format!("failed to run blockdev --getsize64: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "blockdev --getsize64 failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|e| format!("failed to parse blockdev --getsize64 output: {e}"))
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
            let fallback = std::env::temp_dir();
            eprintln!(
                "warning: /dev/shm (tmpfs) unavailable; the transient bootstrap credential's temp file will be written to disk-backed storage at {}",
                fallback.display()
            );
            fallback
        };

        let suffix_hex = random_hex_suffix()?;
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
///
/// The stdin write happens on a separate thread from `wait_with_output`'s
/// stdout/stderr draining: if `cmd` writes enough output to fill its pipe
/// before it has read all of `input`, a single-threaded write-then-wait
/// would deadlock (this thread blocked writing stdin, the child blocked
/// writing to a full stdout/stderr pipe nobody is draining yet).
fn run_piping_stdin(cmd: &mut Command, input: &[u8]) -> Result<(), String> {
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn {cmd:?}: {e}"))?;

    let mut stdin = child.stdin.take().expect("stdin was requested as piped");
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for {cmd:?}: {e}"))?;

    if output.status.success() {
        // Only surface a stdin-write failure if the process itself otherwise
        // succeeded — a process failure's own stderr is the more useful signal.
        if let Ok(Err(e)) = writer.join() {
            return Err(format!("failed writing to {cmd:?}'s stdin: {e}"));
        }
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

/// Keyslot numbers actually present in the LUKS2 header's own top-level
/// `keyslots` object — the ground truth for "does this keyslot still exist,"
/// independent of what any token claims (AD-5: "a stale `systemd-fido2` token
/// pointing at an already-gone keyslot must never be counted as a live key").
fn live_keyslot_numbers(metadata: &Value) -> Result<HashSet<u32>, DomainError> {
    let keyslots = metadata
        .get("keyslots")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            DomainError::AdapterFailure("luksDump JSON missing a keyslots object".to_string())
        })?;

    keyslots
        .keys()
        .map(|id| {
            id.parse::<u32>().map_err(|_| {
                DomainError::AdapterFailure(format!(
                    "unrecognized keyslot id {id:?} in luksDump JSON"
                ))
            })
        })
        .collect()
}

/// All `systemd-fido2` token ids present in a `luksDump
/// --dump-json-metadata` snapshot already in hand.
fn systemd_fido2_token_ids(metadata: &Value) -> Result<HashSet<String>, DomainError> {
    Ok(tokens_object(metadata)?
        .iter()
        .filter(|(_, token)| token.get("type").and_then(Value::as_str) == Some("systemd-fido2"))
        .map(|(id, _)| id.clone())
        .collect())
}

/// All `systemd-fido2` token ids currently present in `path`'s LUKS2 header.
/// Called both before and after a `systemd-cryptenroll` call so the caller
/// can diff the two sets and identify exactly which token id is newly
/// created — the only reliable way to tell "the token just enrolled" apart
/// from any other `systemd-fido2` token already on the header (a tomb with
/// two enrolled keys has two of them).
fn find_systemd_fido2_token_ids(path: &Path) -> Result<HashSet<String>, DomainError> {
    systemd_fido2_token_ids(&dump_json_metadata(path)?)
}

/// `key_label` values already present on any `systemd-fido2` token in
/// `metadata` — used to reject enrolling a label that collides with an
/// already-enrolled key's, which would defeat AD-2's whole point of telling
/// keys apart at revoke-time listing.
fn existing_key_labels(metadata: &Value) -> Result<Vec<String>, DomainError> {
    Ok(tokens_object(metadata)?
        .values()
        .filter(|token| token.get("type").and_then(Value::as_str) == Some("systemd-fido2"))
        .filter_map(|token| token.get("key_label").and_then(Value::as_str))
        .map(str::to_string)
        .collect())
}

/// Keyslot numbers `metadata`'s `token_id` token currently references — used
/// to roll back a keyslot `systemd-cryptenroll` already created if writing
/// its metadata afterward then fails (see `enroll_fido2_key`), rather than
/// leaving an unlabeled, un-bookkept key on the tomb.
fn keyslots_for_token(metadata: &Value, token_id: &str) -> Vec<u32> {
    tokens_object(metadata)
        .ok()
        .and_then(|tokens| tokens.get(token_id))
        .and_then(|token| token.get("keyslots"))
        .and_then(Value::as_array)
        .map(|slots| {
            slots
                .iter()
                .filter_map(|slot| slot.as_str()?.parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// One FIDO2 security key as reported by a single `fido2-token -L`
/// enumeration: its hidraw path (e.g. `/dev/hidraw1`) plus whatever
/// product/vendor description `fido2-token` prints alongside it. A pure
/// enumeration query — never requires a touch or PIN.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Fido2Device {
    path: String,
    description: String,
}

/// Every FIDO2 security key currently plugged in, per one `fido2-token -L`
/// call — a single point-in-time snapshot, in the order `fido2-token`
/// reports them. This is the "spatial choice" primitive `enroll_fido2_key`'s
/// device-selection logic is built on: identify devices by what's plugged in
/// *right now*, never by diffing two enumerations taken before/after a
/// device is plugged in (that temporal-diff approach was tried and found
/// racy — see the story's Dev Notes/"Architect consultation resolved").
fn list_fido2_devices() -> Result<Vec<Fido2Device>, DomainError> {
    let output = Command::new("fido2-token")
        .arg("-L")
        .output()
        .map_err(|e| DomainError::AdapterFailure(format!("failed to run fido2-token -L: {e}")))?;

    if !output.status.success() {
        return Err(DomainError::AdapterFailure(format!(
            "fido2-token -L failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            line.split_once(':').map(|(path, description)| Fido2Device {
                path: path.trim().to_string(),
                description: description.trim().to_string(),
            })
        })
        .filter(|device| !device.path.is_empty())
        .collect())
}

/// Blocks, polling `fido2-token -L`, until at least `needed` devices are
/// enumerated — subsumes the "enroll/unlock fails immediately if no key is
/// plugged in yet" complaint (deferred-work item) for every caller that
/// needs at least one device present. Prints a friendly wait message only
/// when the enumerated count changes, so it doesn't spam the terminal every
/// poll tick.
fn wait_for_enough_fido2_devices(needed: usize) -> Result<Vec<Fido2Device>, DomainError> {
    let mut last_seen = usize::MAX;
    loop {
        let devices = list_fido2_devices()?;
        if devices.len() >= needed {
            return Ok(devices);
        }
        if devices.len() != last_seen {
            let more = needed - devices.len();
            println!(
                "Found {} FIDO2 security key{} plugged in — plug in {} more and keep it there.",
                devices.len(),
                if devices.len() == 1 { "" } else { "s" },
                more
            );
            last_seen = devices.len();
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Prints every currently-enumerated device, numbered from 1, with its
/// hidraw path and whatever description `fido2-token -L` exposes — the
/// user's only way to tell physically-identical keys apart when 3+ are
/// plugged in at once (a known, accepted limitation; see Dev Notes).
fn print_numbered_fido2_devices(devices: &[Fido2Device]) {
    println!("Currently plugged-in FIDO2 security keys:");
    for (index, device) in devices.iter().enumerate() {
        println!("  [{}] {} ({})", index + 1, device.path, device.description);
    }
}

/// Prompts `prompt`, then blocks for a 1-based index into `devices`,
/// re-prompting on anything out of range or unparseable. Never reads or
/// echoes anything secret (AD-3 governs PIN/touch material, not this plain
/// orchestration step).
fn prompt_for_device_index(prompt: &str, devices: &[Fido2Device]) -> Result<usize, DomainError> {
    loop {
        print!("{prompt} [1-{}]: ", devices.len());
        let _ = io::stdout().flush();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            // `read_line` returns `Ok(0)` (not an `Err`) on EOF — without this
            // arm, a closed/piped stdin (non-interactive invocation) spun
            // this loop immediately and indefinitely instead of failing.
            Ok(0) => {
                return Err(DomainError::AdapterFailure(
                    "stdin closed while waiting for a FIDO2 device selection".to_string(),
                ));
            }
            Err(_) => {
                return Err(DomainError::AdapterFailure(
                    "failed to read FIDO2 device selection from stdin".to_string(),
                ));
            }
            Ok(_) => {}
        }

        match input.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= devices.len() => return Ok(n - 1),
            _ => println!("Please enter a number between 1 and {}.", devices.len()),
        }
    }
}

/// Resolves `Fido2DeviceSelection::Interactive` to concrete hidraw paths.
/// `need_existing` is false for create's bootstrap-enroll call (single "new
/// key" role only) and true for a standalone enroll authenticating against
/// an already-enrolled key (both "existing" and "new" roles).
///
/// A lone device present when only the "new key" role is needed is
/// auto-picked with no prompt at all — this is what keeps `create`'s
/// existing single-key UX unchanged. Otherwise every enumerated device is
/// listed by index and the user explicitly assigns each needed role; the
/// same list-and-pick code path handles exactly-2 and N>2 uniformly (no
/// elimination shortcut).
fn resolve_interactive_selection(
    need_existing: bool,
) -> Result<(String, Option<String>), DomainError> {
    let needed = if need_existing { 2 } else { 1 };
    let devices = wait_for_enough_fido2_devices(needed)?;

    if !need_existing && devices.len() == 1 {
        return Ok((devices[0].path.clone(), None));
    }

    print_numbered_fido2_devices(&devices);

    let existing_index = if need_existing {
        Some(prompt_for_device_index(
            "Which is your EXISTING key?",
            &devices,
        )?)
    } else {
        None
    };

    let new_index = loop {
        let index = prompt_for_device_index("Which is your NEW key?", &devices)?;
        if Some(index) == existing_index {
            println!(
                "That's the same key you picked as the existing one — choose a different index."
            );
            continue;
        }
        break index;
    };

    Ok((
        devices[new_index].path.clone(),
        existing_index.map(|index| devices[index].path.clone()),
    ))
}

/// Validates that `path` is currently enumerated among `devices`, returning
/// its display string — same before-the-mutating-call discipline as AD-4,
/// rather than letting `systemd-cryptenroll` fail deep inside with a cryptic
/// error. `role` (e.g. `"new"`/`"existing"`) only shapes the error message.
fn validate_device_enumerated(
    devices: &[Fido2Device],
    path: &Path,
    role: &str,
) -> Result<String, DomainError> {
    let path_str = path.display().to_string();
    if devices.iter().any(|device| device.path == path_str) {
        Ok(path_str)
    } else {
        Err(DomainError::AdapterFailure(format!(
            "the specified {role} FIDO2 device {path_str} is not currently enumerated by \
             fido2-token -L"
        )))
    }
}

/// Resolves `Fido2DeviceSelection::Explicit` to concrete hidraw paths against
/// a given point-in-time `devices` enumeration (injected rather than queried
/// internally, so this validation logic is exercisable without a real
/// `fido2-token` binary — see this module's `tests` below).
fn resolve_explicit_selection(
    devices: &[Fido2Device],
    new: &Path,
    existing: Option<&Path>,
    need_existing: bool,
) -> Result<(String, Option<String>), DomainError> {
    if need_existing && existing.is_none() {
        return Err(DomainError::AdapterFailure(
            "enrolling against an already-enrolled key requires an explicit \
             --unlock-fido2-device path"
                .to_string(),
        ));
    }

    let new_path = validate_device_enumerated(devices, new, "new")?;
    let existing_path = existing
        .map(|existing| validate_device_enumerated(devices, existing, "existing"))
        .transpose()?;

    if existing_path.as_deref() == Some(new_path.as_str()) {
        return Err(DomainError::AdapterFailure(
            "--fido2-device and --unlock-fido2-device must refer to different devices".to_string(),
        ));
    }

    Ok((new_path, existing_path))
}

/// Resolves `selection` to the concrete `(new_device, existing_device)`
/// hidraw paths `systemd-cryptenroll` needs — `existing_device` is `Some`
/// exactly when `need_existing` is true.
fn resolve_device_selection(
    selection: &Fido2DeviceSelection,
    need_existing: bool,
) -> Result<(String, Option<String>), DomainError> {
    match selection {
        Fido2DeviceSelection::Interactive => resolve_interactive_selection(need_existing),
        Fido2DeviceSelection::Explicit { new, existing } => {
            let devices = list_fido2_devices()?;
            resolve_explicit_selection(&devices, new, existing.as_deref(), need_existing)
        }
    }
}

impl ExecAdapter {
    /// Writes `metadata`'s fields directly onto the `systemd-fido2` token
    /// identified by `token_id` — the caller (`enroll_fido2_key`) has
    /// already resolved this to the specific token `systemd-cryptenroll`
    /// just created via a before/after diff (AD-2 — confirmed by Story 1.5's
    /// Task 1 spike that the plugin tolerates these extra fields).
    fn write_fido2_token_metadata(
        &self,
        path: &Path,
        token_id: &str,
        metadata: KeyMetadata,
    ) -> Result<(), DomainError> {
        let export = Command::new("cryptsetup")
            .args(["token", "export", "--token-id", token_id])
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
            .ok_or_else(|| {
                DomainError::AdapterFailure(
                    "exported token JSON missing expected fido2-credential field".to_string(),
                )
            })?
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
                .args(["token", "import", "--token-id", token_id, "--token-replace"])
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

    fn has_luks2_header(&self, path: &Path) -> Result<bool, DomainError> {
        let output = Command::new("cryptsetup")
            .args(["isLuks", "--type", "luks2"])
            .arg(path)
            .output()
            .map_err(|e| {
                DomainError::AdapterFailure(format!("failed to run cryptsetup isLuks: {e}"))
            })?;

        // Confirmed empirically on this machine's cryptsetup: exit 0 = is a
        // LUKS2 device, exit 1 = not a LUKS device (the true "no header"
        // case). Any other code (2 wrong parameters, 3 out of memory, 4
        // device does not exist/access denied, 5 device busy, or no code at
        // all) means isLuks could not actually determine header status —
        // collapsing those into "no header" would let a permission or
        // transient error silently bypass AC #4's refusal.
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(DomainError::AdapterFailure(format!(
                "cryptsetup isLuks could not determine LUKS2 header status for {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ))),
        }
    }

    fn bootstrap_format_and_open(
        &self,
        path: &Path,
        name: &str,
        size: u64,
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

        // `luksFormat --size`/`-b` is rejected outright by this installed
        // cryptsetup version ("Option --size is not allowed with luksFormat
        // action" — confirmed empirically, contradicting the generic --help
        // listing and this story's original assumption). The documented
        // mechanism to constrain a LUKS2 mapping to less than the underlying
        // device's full capacity is instead `cryptsetup resize --device-size`
        // on the already-open mapping (cryptsetup-resize(8)); it takes a
        // plain byte count with no unit suffix, so no sector-rounding
        // precision loss. Must run before mkfs (a separate port call) ever
        // sees the mapped device.
        //
        // Only run it when `size` asks for less than the full raw capacity
        // of `path` — resizing to exactly the full raw capacity fails
        // outright ("Device is too small"), since the LUKS2 header has
        // nowhere left to live alongside a full-size payload (confirmed on
        // real hardware during Story 1.7's hardware verification — this
        // unconditional call was assumed a harmless no-op for file-backed
        // create, where `size` always equals the backing file's own exact
        // size, but that assumption was wrong). LUKS2's default dynamic
        // sizing already gives the correct, header-excluded mapping for the
        // full-capacity case with no resize needed at all.
        //
        // Confirmed empirically (`cryptsetup status` right after this call,
        // on real hardware) that when it does run, this genuinely constrains
        // the *active* mapping mkfs subsequently sees to exactly `size`
        // bytes — even though the LUKS2 header's own `segments.0.size`
        // metadata stays `"dynamic"` (i.e. "recompute from the real device
        // size at every open") rather than being rewritten to a fixed value.
        // That's by design, not a bug: it's what lets a later grow (Story
        // 3.2) resize just the ext4 filesystem, with no LUKS2-level resize
        // ever needed — the mapping already dynamically represents the
        // device's full capacity on any future plain `luksOpen`.
        //
        // `resize` normally re-authenticates via the LUKS2 kernel keyring
        // rather than a passphrase — but that keyring lookup is scoped to
        // the calling process/session, and `luksOpen` and `resize` here are
        // two separate `sudo cryptsetup` invocations, so the key isn't
        // visible across them (confirmed empirically on real hardware:
        // resize fell back to an interactive passphrase prompt against a
        // stdin this adapter leaves unattached, producing "Nothing to read
        // on input."). Piping the still-in-scope transient passphrase via
        // `--key-file -`, the same non-interactive mechanism already used
        // for `luksFormat`/`luksOpen`, sidesteps the keyring entirely.
        let raw_size = match actual_raw_size(path) {
            Ok(raw_size) => raw_size,
            Err(e) => {
                // `luksOpen` above already succeeded — same leak this
                // function's `resize` failure branch below already guards
                // against applies here too.
                let _ = privileged("cryptsetup").arg("close").arg(name).output();
                return Err(DomainError::AdapterFailure(e));
            }
        };
        if raw_size > size {
            if let Err(e) = run_piping_stdin(
                privileged("cryptsetup")
                    .args([
                        "resize",
                        "--device-size",
                        &size.to_string(),
                        "--key-file",
                        "-",
                    ])
                    .arg(name),
                passphrase.as_bytes(),
            ) {
                // `luksOpen` above already succeeded — no `MapperHandle`
                // exists yet for the caller to close on this early return, so
                // this adapter must close the mapping itself or it leaks
                // indefinitely.
                let _ = privileged("cryptsetup").arg("close").arg(name).output();
                return Err(DomainError::AdapterFailure(e));
            }
        }

        // Not wiped yet: enroll_fido2_key still needs it to authenticate
        // adding the real key's keyslot. Cached here, never surfaced to
        // `domain` (AD-3/AD-9).
        *self.transient_passphrase.borrow_mut() = Some(passphrase);

        Ok(MapperHandle {
            name: name.to_string(),
            source_path: path.to_path_buf(),
        })
    }

    fn list_fido2_keyslots(&self, path: &Path) -> Result<Vec<KeyslotInfo>, DomainError> {
        let metadata = dump_json_metadata(path)?;
        let tokens = tokens_object(&metadata)?;
        let live_keyslots = live_keyslot_numbers(&metadata)?;

        let mut keyslots = Vec::new();
        for token in tokens.values() {
            if token.get("type").and_then(Value::as_str) != Some("systemd-fido2") {
                continue;
            }
            let Some(token_keyslots) = token.get("keyslots").and_then(Value::as_array) else {
                continue;
            };
            for slot in token_keyslots {
                let Some(slot_num) = slot.as_str().and_then(|s| s.parse::<u32>().ok()) else {
                    continue;
                };
                // A token referencing a keyslot that no longer exists in the
                // header (e.g. removed via bare `cryptsetup luksKillSlot`,
                // AD-6's break-glass path) must never count as a live key
                // (AD-5) — also dedupes if more than one token somehow
                // references the same keyslot.
                let already_counted = keyslots
                    .iter()
                    .any(|info: &KeyslotInfo| info.keyslot == KeyslotRef(slot_num));
                if live_keyslots.contains(&slot_num) && !already_counted {
                    let key_label = token
                        .get("key_label")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    keyslots.push(KeyslotInfo {
                        keyslot: KeyslotRef(slot_num),
                        key_label,
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

    fn open(&self, path: &Path, name: &str, read_only: bool) -> Result<MapperHandle, DomainError> {
        // Waits until at least one FIDO2 device is enumerated before ever
        // invoking cryptsetup — without this, unlocking with no key plugged
        // in yet failed immediately instead of waiting (deferred-work item).
        // Cryptsetup's own credential-based `--token-only auto` already
        // disambiguates correctly once at least one device is present, so no
        // picker/explicit-device flag is needed here, unlike `enroll`.
        wait_for_enough_fido2_devices(1)?;

        // `--token-only` is not optional: without it, `cryptsetup open` falls
        // back to an interactive passphrase prompt instead of the FIDO2
        // PIN/touch flow (confirmed empirically during Story 1.6's hardware
        // run). Inherited stdio (`.status()`, not `.output()`) lets the
        // systemd-fido2 plugin's own prompt reach the real terminal, the same
        // pattern `enroll_fido2_key`'s `systemd-cryptenroll` call already
        // uses.
        let mut cmd = privileged("cryptsetup");
        cmd.args(["open", "--token-only"]);
        if read_only {
            cmd.arg("--readonly");
        }
        let status = cmd.arg(path).arg(name).status().map_err(|e| {
            DomainError::AdapterFailure(format!("failed to run cryptsetup open: {e}"))
        })?;

        if status.success() {
            Ok(MapperHandle {
                name: name.to_string(),
                source_path: path.to_path_buf(),
            })
        } else {
            Err(DomainError::AdapterFailure(format!(
                "cryptsetup open --token-only failed for {} as {name}",
                path.display()
            )))
        }
    }

    fn resize(&self, mapper: &MapperHandle) -> Result<(), DomainError> {
        // No `--device-size`: the header's segment stays `"dynamic"` and
        // recomputes from the backing storage's actual current size, which
        // the caller has already grown by this point (AD-10 ordering).
        //
        // `--token-only` is load-bearing, not optional (Task 0 spike,
        // confirmed on real hardware): a bare `cryptsetup resize <name>`
        // does NOT reuse the kernel keyring entry a preceding
        // `open --token-only` populated — it falls back to an interactive
        // passphrase prompt, which this workflow has no passphrase to
        // satisfy. `--token-only` instead re-authenticates via the enrolled
        // FIDO2 token, the same mechanism `open` already uses. Inherited
        // stdio (`.status()`, not `.output()`) lets that touch/PIN prompt
        // reach the real terminal, same pattern as `open`.
        let status = privileged("cryptsetup")
            .args(["resize", "--token-only"])
            .arg(&mapper.name)
            .status()
            .map_err(|e| {
                DomainError::AdapterFailure(format!("failed to run cryptsetup resize: {e}"))
            })?;

        if status.success() {
            Ok(())
        } else {
            Err(DomainError::AdapterFailure(format!(
                "cryptsetup resize --token-only failed for {}",
                mapper.name
            )))
        }
    }

    fn read_filesystem(&self, path: &Path) -> Result<Filesystem, DomainError> {
        let metadata = dump_json_metadata(path)?;
        let tokens = tokens_object(&metadata)?;

        let filesystem_str = tokens
            .values()
            .find(|token| token.get("type").and_then(Value::as_str) == Some("systemd-fido2"))
            .and_then(|token| token.get("filesystem"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DomainError::AdapterFailure(format!(
                    "{} has no systemd-fido2 token with a filesystem field",
                    path.display()
                ))
            })?;

        match filesystem_str {
            "ext4" => Ok(Filesystem::Ext4),
            other => Err(DomainError::AdapterFailure(format!(
                "unrecognized filesystem {other:?} recorded on {}'s systemd-fido2 token",
                path.display()
            ))),
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

    fn enroll_fido2_key(
        &self,
        mapper: &MapperHandle,
        metadata: KeyMetadata,
        selection: Fido2DeviceSelection,
    ) -> Result<(), DomainError> {
        let path = &mapper.source_path;

        // Snapshotted before `systemd-cryptenroll` runs, so the new token can
        // be identified afterward by set difference rather than picking "the
        // first" `systemd-fido2` token — a tomb can now carry more than one
        // (this story's whole point), and picking the wrong one would
        // silently overwrite an existing key's metadata instead of writing
        // the new one. Empty for create's bootstrap-enroll call (no token
        // exists yet), so behavior there is unchanged.
        let existing_metadata = dump_json_metadata(path)?;
        let existing_token_ids = systemd_fido2_token_ids(&existing_metadata)?;

        // Reject a label that collides with an already-enrolled key's — two
        // identically-labeled keys would defeat AD-2's whole point of
        // telling them apart at revoke-time listing.
        if existing_key_labels(&existing_metadata)?
            .iter()
            .any(|label| label == &metadata.key_label)
        {
            return Err(DomainError::AdapterFailure(format!(
                "a key labeled {:?} is already enrolled on this tomb — choose a different label",
                metadata.key_label
            )));
        }

        let has_transient_passphrase = self.transient_passphrase.borrow().is_some();

        // Device selection resolved before the transient passphrase (if any)
        // is taken out of its `RefCell` — this keeps the plaintext secret's
        // in-memory lifetime from including a potentially long interactive
        // wait for a FIDO2 device to be plugged in. Only the "new key" role
        // needs filling when a transient passphrase exists (create's
        // bootstrap-enroll call); a standalone enroll also needs an
        // "existing key" role to authenticate against.
        let (new_device, existing_device) =
            resolve_device_selection(&selection, !has_transient_passphrase)?;

        let passphrase = self.transient_passphrase.borrow_mut().take();

        // `systemd-cryptenroll` doesn't read a piped (non-tty) stdin as a
        // passphrase the way `cryptsetup` does — confirmed empirically: it
        // instead falls back to systemd's ask-password broadcast/agent
        // mechanism and hangs waiting for an agent. `--unlock-key-file` is
        // the documented non-interactive path instead, so the passphrase is
        // written to a tightly-permissioned, promptly-deleted temp file.
        //
        // stdin/stdout/stderr all stay inherited in both branches
        // (`.status()`, never `.output()`): the unlock credential, when one
        // exists, travels via `--unlock-key-file` rather than stdin, so the
        // user's terminal is free to handle systemd-cryptenroll's own FIDO2
        // touch/PIN prompt normally.
        let status = match passphrase {
            Some(passphrase) => {
                let key_file = TempKeyFile::create(passphrase.as_bytes())
                    .map_err(DomainError::AdapterFailure)?;

                // The in-memory passphrase is dropped (zeroized) here —
                // strictly before `create::run` goes on to call `mkfs` (AC
                // #3, AD-3). The on-disk copy in `key_file` is wiped by its
                // own Drop impl once this function returns.
                drop(passphrase);

                Command::new("systemd-cryptenroll")
                    .arg(format!("--fido2-device={new_device}"))
                    .arg(format!("--unlock-key-file={}", key_file.path.display()))
                    .arg(path)
                    .status()
            }
            None => {
                // No transient bootstrap passphrase exists (this story's
                // standalone enroll, as opposed to create's bootstrap-enroll
                // call). Confirmed on real hardware: `systemd-cryptenroll`
                // does NOT automatically try an already-enrolled FIDO2 token
                // to unlock just because no unlock method was given — it
                // falls back to an interactive passphrase prompt instead,
                // which can never succeed once the bootstrap passphrase
                // keyslot has been removed (this tomb has no passphrase
                // keyslot at all). Per systemd-cryptenroll(1)'s UNLOCKING
                // section, unlocking via FIDO2 during an enroll call needs
                // an *explicit* `--unlock-fido2-device=` path — `auto` is
                // unsupported for it once `--fido2-device=` is also given,
                // which it always is here (the new key being enrolled).
                //
                // Both roles are resolved by a single point-in-time
                // enumeration plus explicit user choice (or an explicit CLI
                // flag), never by diffing two enumerations taken before/after
                // a device is plugged in — that temporal-diff approach is
                // what produced the "More than one FIDO device found"/hang
                // failure this design replaces. Already resolved above,
                // before the passphrase check.
                let existing_device = existing_device
                    .expect("resolve_device_selection guarantees Some when need_existing is true");

                Command::new("systemd-cryptenroll")
                    .arg(format!("--fido2-device={new_device}"))
                    .arg(format!("--unlock-fido2-device={existing_device}"))
                    .arg(path)
                    .status()
            }
        }
        .map_err(|e| {
            DomainError::AdapterFailure(format!("failed to run systemd-cryptenroll: {e}"))
        })?;

        if !status.success() {
            return Err(DomainError::AdapterFailure(
                "systemd-cryptenroll failed".to_string(),
            ));
        }

        let new_token_ids = find_systemd_fido2_token_ids(path)?;
        let mut newly_created: Vec<String> = new_token_ids
            .difference(&existing_token_ids)
            .cloned()
            .collect();
        if newly_created.len() != 1 {
            return Err(DomainError::AdapterFailure(format!(
                "expected exactly one new systemd-fido2 token after enrollment, found {}",
                newly_created.len()
            )));
        }
        let token_id = newly_created.pop().expect("checked len == 1 above");

        if let Err(err) = self.write_fido2_token_metadata(path, &token_id, metadata) {
            // `systemd-cryptenroll` already created a real, working keyslot
            // above — if writing its metadata then fails, roll it back
            // rather than leaving an unlabeled, un-bookkept key on the tomb.
            // Best-effort: if this re-dump itself fails, the original `err`
            // is still what's returned.
            if let Ok(rollback_metadata) = dump_json_metadata(path) {
                for keyslot in keyslots_for_token(&rollback_metadata, &token_id) {
                    let _ = LuksBackend::remove_key(self, path, KeyslotRef(keyslot));
                }
            }
            return Err(err);
        }

        Ok(())
    }
}

impl FilesystemBackend for ExecAdapter {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        let mut missing = Vec::new();

        for binary in [
            "mkfs.ext4",
            "resize2fs",
            "e2fsck",
            "dumpe2fs",
            "blockdev",
            "mount",
            "umount",
            "findmnt",
            "id",
        ] {
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

    fn is_block_device(&self, path: &Path) -> Result<bool, DomainError> {
        use std::os::unix::fs::FileTypeExt;

        let metadata = std::fs::metadata(path).map_err(|e| {
            DomainError::AdapterFailure(format!("failed to stat {}: {e}", path.display()))
        })?;
        Ok(metadata.file_type().is_block_device())
    }

    fn device_capacity(&self, path: &Path) -> Result<u64, DomainError> {
        let output = Command::new("blockdev")
            .arg("--getsize64")
            .arg(path)
            .output()
            .map_err(|e| {
                DomainError::AdapterFailure(format!("failed to run blockdev --getsize64: {e}"))
            })?;

        if !output.status.success() {
            return Err(DomainError::AdapterFailure(format!(
                "blockdev --getsize64 failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .map_err(|e| {
                DomainError::AdapterFailure(format!(
                    "failed to parse blockdev --getsize64 output: {e}"
                ))
            })
    }

    fn set_backing_file_size(&self, path: &Path, size: u64) -> Result<(), DomainError> {
        // Branches on whether `path` already exists (Story 3.2, AC #1):
        // `create`'s call site never hits the grow branch (it already checks
        // `path_exists` first and refuses if true), so that branch's
        // behavior is exactly unchanged. `resize`'s call site only ever
        // hits the grow branch (its target already has a LUKS2 header).
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                // Grow an existing file. First confirm it's a plain regular
                // file, not a symlink — a plain `File::open` would instead
                // follow a symlink and silently write through it, the same
                // clobber protection the create branch below gets from
                // `create_new`, extended to this path too.
                if !metadata.file_type().is_file() {
                    return Err(DomainError::AdapterFailure(format!(
                        "{} is not a regular file — refusing to grow it",
                        path.display()
                    )));
                }

                use std::os::unix::fs::OpenOptionsExt;
                // Linux's `O_NOFOLLOW` (this project only targets Linux, see
                // Cargo.toml's dist `targets`) — makes the open itself
                // atomically refuse a symlink, closing the race window
                // between the `symlink_metadata` check above and this call
                // (a symlink swapped into place in between would make this
                // open fail instead of silently following it and writing
                // through it — review finding, 2026-07-26).
                const O_NOFOLLOW: i32 = 0o400000;
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(O_NOFOLLOW)
                    .open(path)
                    .map_err(|e| {
                        DomainError::AdapterFailure(format!(
                            "failed to open {} for growing: {e}",
                            path.display()
                        ))
                    })?;
                file.set_len(size).map_err(|e| {
                    DomainError::AdapterFailure(format!("failed to size {}: {e}", path.display()))
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // `create_new` makes the OS enforce exclusivity: it fails if
                // anything (a regular file or a symlink, dangling or not)
                // already exists at `path`, closing the race window between
                // the caller's `path_exists` check and this call — a plain
                // `File::create` would instead follow a symlink and silently
                // truncate/write through it (AC #2).
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|e| {
                        if e.kind() == std::io::ErrorKind::AlreadyExists {
                            DomainError::DestinationExists(path.to_path_buf())
                        } else {
                            DomainError::AdapterFailure(format!(
                                "failed to create {}: {e}",
                                path.display()
                            ))
                        }
                    })?;
                file.set_len(size).map_err(|e| {
                    DomainError::AdapterFailure(format!("failed to size {}: {e}", path.display()))
                })
            }
            Err(e) => Err(DomainError::AdapterFailure(format!(
                "failed to stat {}: {e}",
                path.display()
            ))),
        }
    }

    fn remove_backing_file(&self, path: &Path) -> Result<(), DomainError> {
        std::fs::remove_file(path).map_err(|e| {
            DomainError::AdapterFailure(format!("failed to remove {}: {e}", path.display()))
        })
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

    fn growfs(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<(), DomainError> {
        match fs {
            Filesystem::Ext4 => {
                // Confirmed empirically on real hardware: `resize2fs` refuses
                // to grow an unmounted filesystem that hasn't been checked
                // ("Please run 'e2fsck -f <device>' first"), even one that
                // was never actually corrupted — this workflow never mounts
                // the filesystem (Task 3's doc comment), so it always hits
                // this. `-p` (preen) auto-fixes non-conflicting problems
                // without prompting; exit code 1 means "errors corrected" and
                // is still a success per e2fsck(8) (2+ means something more
                // serious, e.g. "reboot needed" or "operational error").
                let fsck_output = privileged("e2fsck")
                    .args(["-f", "-p"])
                    .arg(mapper.device_node())
                    .output()
                    .map_err(|e| {
                        DomainError::AdapterFailure(format!("failed to run e2fsck: {e}"))
                    })?;
                match fsck_output.status.code() {
                    Some(0) | Some(1) => {}
                    _ => {
                        return Err(DomainError::AdapterFailure(format!(
                            "e2fsck -f failed: {}",
                            String::from_utf8_lossy(&fsck_output.stderr).trim()
                        )));
                    }
                }

                // No explicit target size: grows to fill the now-larger
                // mapping (`resize2fs`'s documented behavior when no size
                // argument is given). Runs against the unmounted mapper
                // device node — resize2fs also supports online (mounted)
                // growth, but this workflow never mounts the filesystem.
                let output = privileged("resize2fs")
                    .arg(mapper.device_node())
                    .output()
                    .map_err(|e| {
                        DomainError::AdapterFailure(format!("failed to run resize2fs: {e}"))
                    })?;

                if output.status.success() {
                    Ok(())
                } else {
                    Err(DomainError::AdapterFailure(format!(
                        "resize2fs failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )))
                }
            }
        }
    }

    fn filesystem_size(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<u64, DomainError> {
        match fs {
            Filesystem::Ext4 => {
                let output = privileged("dumpe2fs")
                    .arg("-h")
                    .arg(mapper.device_node())
                    .output()
                    .map_err(|e| {
                        DomainError::AdapterFailure(format!("failed to run dumpe2fs: {e}"))
                    })?;

                if !output.status.success() {
                    return Err(DomainError::AdapterFailure(format!(
                        "dumpe2fs -h failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )));
                }

                let text = String::from_utf8_lossy(&output.stdout);
                let block_count: u64 = text
                    .lines()
                    .find_map(|line| line.strip_prefix("Block count:"))
                    .and_then(|value| value.trim().parse().ok())
                    .ok_or_else(|| {
                        DomainError::AdapterFailure(
                            "dumpe2fs -h output missing a parsable Block count".to_string(),
                        )
                    })?;
                let block_size: u64 = text
                    .lines()
                    .find_map(|line| line.strip_prefix("Block size:"))
                    .and_then(|value| value.trim().parse().ok())
                    .ok_or_else(|| {
                        DomainError::AdapterFailure(
                            "dumpe2fs -h output missing a parsable Block size".to_string(),
                        )
                    })?;

                Ok(block_count * block_size)
            }
        }
    }

    fn mount(&self, mapper: &MapperHandle, read_only: bool) -> Result<PathBuf, DomainError> {
        // Fetched once up front: needed both for the `/run/media/<username>`
        // base directory below and for the mount-point `chown` further down.
        let identity = invoking_identity()?;

        // `file_stem()` strips the extension for a file path (`vault.img` ->
        // `vault`) and returns the whole name for an extensionless device
        // path (`/dev/sdb1` -> `sdb1`); falling back to the full name covers
        // the never-expected case where `file_stem()` itself returns `None`.
        let tomb_name = mapper
            .source_path
            .file_stem()
            .unwrap_or(mapper.source_path.as_os_str())
            .to_string_lossy()
            .into_owned();

        let base = PathBuf::from(format!("/run/media/{}", identity.username));
        if !base.exists() {
            let mkdir_output = privileged("mkdir")
                .args(["-p", "-m", "0755"])
                .arg(&base)
                .output()
                .map_err(|e| {
                    DomainError::AdapterFailure(format!(
                        "failed to run mkdir -p {}: {e}",
                        base.display()
                    ))
                })?;
            if !mkdir_output.status.success() {
                return Err(DomainError::AdapterFailure(format!(
                    "failed to create {}: {}",
                    base.display(),
                    String::from_utf8_lossy(&mkdir_output.stderr).trim()
                )));
            }

            let chown_output = privileged("chown")
                .arg(format!("{}:{}", identity.uid, identity.gid))
                .arg(&base)
                .output()
                .map_err(|e| {
                    DomainError::AdapterFailure(format!(
                        "failed to run chown on {}: {e}",
                        base.display()
                    ))
                })?;
            if !chown_output.status.success() {
                return Err(DomainError::AdapterFailure(format!(
                    "failed to set ownership of {}: {}",
                    base.display(),
                    String::from_utf8_lossy(&chown_output.stderr).trim()
                )));
            }
        }

        let mountpoint = create_mount_point(&base, &tomb_name)?;

        // No `-t`: let mount auto-detect the filesystem type from the
        // superblock (standard kernel behavior) rather than re-deriving it
        // from LUKS2 token metadata unlock has no other reason to read.
        let mut mount_cmd = privileged("mount");
        if read_only {
            // `noload`: a read-only `cryptsetup open` also makes the
            // underlying mapping unwritable, so the kernel can't auto-replay
            // an unclean ext4 journal (replay itself needs a block-device
            // write) — without `noload`, `mount -o ro` on a tomb that wasn't
            // cleanly closed fails outright. `noload` skips replay, which is
            // exactly the read-only guarantee this flag exists to uphold.
            mount_cmd.args(["-o", "ro,noload"]);
        }
        let output = match mount_cmd
            .arg(mapper.device_node())
            .arg(&mountpoint)
            .output()
        {
            Ok(output) => output,
            Err(e) => {
                let _ = std::fs::remove_dir(&mountpoint);
                return Err(DomainError::AdapterFailure(format!(
                    "failed to run mount: {e}"
                )));
            }
        };

        if !output.status.success() {
            let _ = std::fs::remove_dir(&mountpoint);
            return Err(DomainError::AdapterFailure(format!(
                "mount failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        // A read-only mount refuses metadata writes (EROFS), so chown/chmod
        // below would themselves fail. Ownership/permissions on the volume's
        // root inode are whatever a prior *writable* unlock already
        // persisted there — every normal unlock chowns to the invoking user,
        // so any tomb ever unlocked writably already carries correct
        // ownership by the time a read-only unlock reaches this point. A
        // tomb never unlocked writably shows root-owned, mkfs.ext4-default
        // (0755) permissions on its first-ever read-only unlock —
        // world-readable/traversable to every local user, not just the
        // invoking one: an accepted limitation, but one worth surfacing
        // rather than leaving silent (see the stat check below).
        if read_only {
            // Non-mutating: a `stat`, not a write, so it doesn't touch the
            // read-only guarantee. Warns rather than fails, since this is a
            // pre-existing exposure this story doesn't introduce and can't
            // fix without writing to a filesystem it just promised not to.
            let owned_by_invoking_user = std::fs::metadata(&mountpoint)
                .ok()
                .and_then(|meta| {
                    identity
                        .uid
                        .parse::<u32>()
                        .ok()
                        .map(|uid| meta.uid() == uid)
                })
                .unwrap_or(true);
            if !owned_by_invoking_user {
                eprintln!(
                    "Warning: this tomb has never been unlocked in read-write mode, so its \
                     contents are still owned by root with default permissions — readable by \
                     any local user, not just you. Unlock it read-write once to restrict access."
                );
            }
            return Ok(mountpoint);
        }

        // The mount point's underlying inode was created by a privileged
        // `mkfs.ext4` at `create` time, so it's currently root-owned; hand it
        // to the invoking user (identity fetched at the top of this
        // function) before restricting it below, or the invoking user would
        // be locked out of their own just-unlocked tomb.
        match privileged("chown")
            .arg(format!("{}:{}", identity.uid, identity.gid))
            .arg(&mountpoint)
            .output()
        {
            Ok(chown_output) if chown_output.status.success() => {}
            Ok(chown_output) => {
                let _ = privileged("umount").arg(&mountpoint).output();
                let _ = std::fs::remove_dir(&mountpoint);
                return Err(DomainError::AdapterFailure(format!(
                    "failed to change mount point ownership: {}",
                    String::from_utf8_lossy(&chown_output.stderr).trim()
                )));
            }
            Err(e) => {
                let _ = privileged("umount").arg(&mountpoint).output();
                let _ = std::fs::remove_dir(&mountpoint);
                return Err(DomainError::AdapterFailure(format!(
                    "failed to run chown: {e}"
                )));
            }
        }

        // `mkfs.ext4` always creates `lost+found` as root:root — the chown
        // above only covers the mount point's own root inode, not this
        // pre-existing entry underneath it, so it would otherwise stay
        // root-owned forever even though everything else in the tomb is now
        // the invoking user's. Best-effort, not fatal: a tomb whose user has
        // since deleted `lost+found` (harmless, some people do) shouldn't
        // block unlock over it, unlike the mount point's own chown above.
        let lost_and_found = mountpoint.join("lost+found");
        if lost_and_found.exists() {
            if let Ok(output) = privileged("chown")
                .arg(format!("{}:{}", identity.uid, identity.gid))
                .arg(&lost_and_found)
                .output()
            {
                if !output.status.success() {
                    eprintln!(
                        "Warning: couldn't change ownership of this tomb's lost+found directory \
                         — it will stay root-owned."
                    );
                }
            }
        }

        // Restrict the mount point to the invoking user only. Without this,
        // the mounted filesystem's own root-inode permissions (e.g.
        // mkfs.ext4's default 0755) are what's visible at `mountpoint` — left
        // as-is, any local user could read the just-unlocked tomb's contents
        // under a world-traversable `/tmp`, defeating the FIDO2 gate.
        match privileged("chmod").arg("0700").arg(&mountpoint).output() {
            Ok(chmod_output) if chmod_output.status.success() => Ok(mountpoint),
            Ok(chmod_output) => {
                let _ = privileged("umount").arg(&mountpoint).output();
                let _ = std::fs::remove_dir(&mountpoint);
                Err(DomainError::AdapterFailure(format!(
                    "failed to restrict mount point permissions: {}",
                    String::from_utf8_lossy(&chmod_output.stderr).trim()
                )))
            }
            Err(e) => {
                let _ = privileged("umount").arg(&mountpoint).output();
                let _ = std::fs::remove_dir(&mountpoint);
                Err(DomainError::AdapterFailure(format!(
                    "failed to run chmod: {e}"
                )))
            }
        }
    }

    fn umount(&self, mapper: &MapperHandle) -> Result<(), DomainError> {
        let device_node = mapper.device_node();

        // Distinct from "not currently mounted" below: no dm-crypt mapping at
        // all means the tomb was never unlocked, or a prior `close` already
        // fully completed — neither is "just needs a retry" (review finding,
        // 2026-07-26).
        if !device_node.exists() {
            return Err(DomainError::AdapterFailure(format!(
                "{} has no active mapping",
                device_node.display()
            )));
        }

        let findmnt_output = Command::new("findmnt")
            .args(["-n", "-o", "TARGET"])
            .arg(&device_node)
            .output()
            .map_err(|e| DomainError::AdapterFailure(format!("failed to run findmnt: {e}")))?;

        // Only the first line: a device mounted at more than one target
        // would otherwise hand `umount` a multi-line argument with an
        // embedded newline (review finding, 2026-07-26).
        let mountpoint = String::from_utf8_lossy(&findmnt_output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();

        if !findmnt_output.status.success() || mountpoint.is_empty() {
            return Err(DomainError::AdapterFailure(format!(
                "{} is not currently mounted",
                device_node.display()
            )));
        }

        let umount_output = privileged("umount")
            .arg(&mountpoint)
            .output()
            .map_err(|e| DomainError::AdapterFailure(format!("failed to run umount: {e}")))?;

        if !umount_output.status.success() {
            return Err(DomainError::AdapterFailure(format!(
                "umount failed: {}",
                String::from_utf8_lossy(&umount_output.stderr).trim()
            )));
        }

        // Mirrors `mount`'s own cleanup-on-error paths: the mount point
        // directories `create_mount_point` makes are meant to be ephemeral,
        // not accumulate across unlock/close cycles. Guarded to this tool's
        // own mount root — never rmdir a path `findmnt` happens to report if
        // it isn't one `mount` created (review finding, 2026-07-26).
        if mountpoint.starts_with("/run/media/") {
            let _ = std::fs::remove_dir(&mountpoint);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(path: &str) -> Fido2Device {
        Fido2Device {
            path: path.to_string(),
            description: "test key".to_string(),
        }
    }

    #[test]
    fn explicit_selection_auto_passes_through_when_new_key_enumerated_and_no_existing_needed() {
        let devices = [device("/dev/hidraw0"), device("/dev/hidraw1")];
        let result =
            resolve_explicit_selection(&devices, Path::new("/dev/hidraw0"), None, false).unwrap();
        assert_eq!(result, ("/dev/hidraw0".to_string(), None));
    }

    #[test]
    fn explicit_selection_rejects_new_device_not_enumerated() {
        let devices = [device("/dev/hidraw0")];
        let err = resolve_explicit_selection(&devices, Path::new("/dev/hidraw9"), None, false)
            .unwrap_err();
        let DomainError::AdapterFailure(message) = err else {
            panic!("expected AdapterFailure");
        };
        assert!(message.contains("/dev/hidraw9"));
        assert!(message.contains("not currently enumerated"));
    }

    #[test]
    fn explicit_selection_requires_existing_when_needed() {
        let devices = [device("/dev/hidraw0")];
        let err = resolve_explicit_selection(&devices, Path::new("/dev/hidraw0"), None, true)
            .unwrap_err();
        let DomainError::AdapterFailure(message) = err else {
            panic!("expected AdapterFailure");
        };
        assert!(message.contains("--unlock-fido2-device"));
    }

    #[test]
    fn explicit_selection_rejects_existing_device_not_enumerated() {
        let devices = [device("/dev/hidraw0")];
        let err = resolve_explicit_selection(
            &devices,
            Path::new("/dev/hidraw0"),
            Some(Path::new("/dev/hidraw9")),
            true,
        )
        .unwrap_err();
        let DomainError::AdapterFailure(message) = err else {
            panic!("expected AdapterFailure");
        };
        assert!(message.contains("/dev/hidraw9"));
        assert!(message.contains("existing FIDO2 device"));
    }

    #[test]
    fn explicit_selection_rejects_identical_new_and_existing_device() {
        let devices = [device("/dev/hidraw0"), device("/dev/hidraw1")];
        let err = resolve_explicit_selection(
            &devices,
            Path::new("/dev/hidraw0"),
            Some(Path::new("/dev/hidraw0")),
            true,
        )
        .unwrap_err();
        let DomainError::AdapterFailure(message) = err else {
            panic!("expected AdapterFailure");
        };
        assert!(message.contains("must refer to different devices"));
    }

    #[test]
    fn explicit_selection_accepts_distinct_new_and_existing_devices() {
        let devices = [device("/dev/hidraw0"), device("/dev/hidraw1")];
        let result = resolve_explicit_selection(
            &devices,
            Path::new("/dev/hidraw1"),
            Some(Path::new("/dev/hidraw0")),
            true,
        )
        .unwrap();
        assert_eq!(
            result,
            ("/dev/hidraw1".to_string(), Some("/dev/hidraw0".to_string()))
        );
    }

    struct TempPath(PathBuf);

    impl TempPath {
        fn unique(name: &str) -> Self {
            let suffix = random_hex_suffix().expect("failed to generate random suffix");
            Self(std::env::temp_dir().join(format!("tomb-fido2-unit-test-{name}-{suffix}")))
        }
    }

    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn set_backing_file_size_creates_a_new_file_when_missing() {
        let path = TempPath::unique("create-new");
        let adapter = ExecAdapter::default();

        let result = adapter.set_backing_file_size(&path.0, 4096);

        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert_eq!(std::fs::metadata(&path.0).unwrap().len(), 4096);
    }

    #[test]
    fn set_backing_file_size_grows_an_existing_file() {
        let path = TempPath::unique("grow-existing");
        std::fs::write(&path.0, [0u8; 1024]).expect("failed to write fixture file");
        let adapter = ExecAdapter::default();

        let result = adapter.set_backing_file_size(&path.0, 8192);

        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert_eq!(std::fs::metadata(&path.0).unwrap().len(), 8192);
    }

    #[test]
    fn set_backing_file_size_refuses_to_grow_through_a_symlink() {
        let target = TempPath::unique("symlink-target");
        std::fs::write(&target.0, [0u8; 1024]).expect("failed to write fixture file");
        let link = TempPath::unique("symlink-link");
        std::os::unix::fs::symlink(&target.0, &link.0).expect("failed to create symlink");
        let adapter = ExecAdapter::default();

        let result = adapter.set_backing_file_size(&link.0, 8192);

        let Err(DomainError::AdapterFailure(message)) = result else {
            panic!("expected AdapterFailure, got {result:?}");
        };
        assert!(message.contains("not a regular file"));
        assert_eq!(
            std::fs::metadata(&target.0).unwrap().len(),
            1024,
            "the symlink target must be left untouched"
        );
    }
}
