use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::adapters::exec::ExecAdapter;
use crate::cli::ux;
use crate::domain::preflight;
use crate::domain::types::{CreateTarget, Filesystem};
use crate::domain::workflows::create::{self, MIN_TOMB_SIZE_BYTES};
use crate::domain::workflows::enroll;
use crate::domain::workflows::revoke;
use crate::domain::workflows::unlock;
use crate::ports::fido2_backend::Fido2DeviceSelection;

// `name`/`version`/`about` are populated by clap from this crate's own
// `CARGO_PKG_*` metadata (AD-13) — never a hardcoded product-name literal.
#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new tomb
    Create {
        #[command(subcommand)]
        mode: CreateMode,
    },

    /// Unlock and mount an existing tomb
    Unlock {
        /// Path to the existing tomb's backing file or device
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,
    },

    /// Enroll an additional FIDO2 key on an existing tomb
    Enroll {
        /// Path to the existing tomb's backing file or device
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// Label for the new key, shown later when listing enrolled keys
        #[arg(long, value_parser = parse_label)]
        label: String,

        /// Hidraw path (e.g. /dev/hidraw1) of the new security key to enroll
        /// — for unattended/scripted use. Must be given together with
        /// --unlock-fido2-device; omit both to be prompted interactively.
        #[arg(long, requires = "unlock_fido2_device")]
        fido2_device: Option<PathBuf>,

        /// Hidraw path of the already-enrolled security key to authenticate
        /// this enrollment with. Must be given together with --fido2-device.
        #[arg(long, requires = "fido2_device")]
        unlock_fido2_device: Option<PathBuf>,
    },

    /// Revoke a FIDO2 key's keyslot from an existing tomb
    Revoke {
        /// Path to the existing tomb's backing file or device
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// Label of the enrolled key to revoke
        #[arg(long, value_parser = parse_label)]
        label: String,
    },
}

// AD-9 requires the CLI to resolve an explicit target mode, never inferred by
// sniffing the path — hence two distinct subcommands rather than a single
// flat `create` with an optional device flag.
#[derive(Subcommand)]
enum CreateMode {
    /// Create a new file-backed tomb
    File {
        /// Destination path for the backing file; must not already exist
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// Size to allocate for the backing file (e.g. 500M, 10G, or a plain
        /// byte count)
        #[arg(long, value_parser = parse_size)]
        size: u64,

        /// Filesystem to create inside the tomb
        #[arg(long, value_enum, default_value = "ext4")]
        filesystem: CliFilesystem,

        /// Hidraw path (e.g. /dev/hidraw1) of the security key to enroll —
        /// for unattended/scripted use. Omit to be prompted interactively.
        #[arg(long)]
        fido2_device: Option<PathBuf>,
    },

    /// Create a new tomb on an existing raw device or partition
    Device {
        /// Path to the target device or partition; must not already carry a
        /// LUKS2 header
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// Size to use (e.g. 500M, 10G, or a plain byte count); defaults to
        /// the device's full capacity when omitted
        #[arg(long, value_parser = parse_size)]
        size: Option<u64>,

        /// Filesystem to create inside the tomb
        #[arg(long, value_enum, default_value = "ext4")]
        filesystem: CliFilesystem,

        /// Hidraw path (e.g. /dev/hidraw1) of the security key to enroll —
        /// for unattended/scripted use. Omit to be prompted interactively.
        #[arg(long)]
        fido2_device: Option<PathBuf>,
    },
}

/// Rejects an empty or whitespace-only label — a blank label would silently
/// defeat AD-2's whole point of telling enrolled keys apart later.
pub fn parse_label(input: &str) -> Result<String, String> {
    if input.trim().is_empty() {
        Err("label must not be empty".to_string())
    } else {
        Ok(input.to_string())
    }
}

/// Parses a size string with an optional K/M/G/T suffix (binary, powers of
/// 1024 — matching `resize2fs`/`lvreduce` convention) into a byte count. No
/// suffix means a plain byte count.
pub fn parse_size(input: &str) -> Result<u64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("size must not be empty".to_string());
    }

    let (number_part, multiplier) = match trimmed.chars().last().expect("checked non-empty above") {
        c if c.eq_ignore_ascii_case(&'k') => (&trimmed[..trimmed.len() - 1], 1024u64),
        c if c.eq_ignore_ascii_case(&'m') => (&trimmed[..trimmed.len() - 1], 1024u64.pow(2)),
        c if c.eq_ignore_ascii_case(&'g') => (&trimmed[..trimmed.len() - 1], 1024u64.pow(3)),
        c if c.eq_ignore_ascii_case(&'t') => (&trimmed[..trimmed.len() - 1], 1024u64.pow(4)),
        _ => (trimmed, 1u64),
    };

    let number: u64 = number_part.trim().parse().map_err(|_| {
        format!("invalid size {input:?} (expected a number, optionally followed by K/M/G/T)")
    })?;

    let bytes = number
        .checked_mul(multiplier)
        .ok_or_else(|| format!("size {input:?} is too large"))?;

    if bytes < MIN_TOMB_SIZE_BYTES {
        return Err(format!(
            "size {input:?} is too small (minimum is {MIN_TOMB_SIZE_BYTES} bytes / 16M)"
        ));
    }

    Ok(bytes)
}

/// Builds the `Fido2DeviceSelection` for `create`'s single `--fido2-device`
/// flag: `Explicit` (no existing-key role to fill) when given, `Interactive`
/// otherwise.
fn fido2_selection_for_create(fido2_device: Option<PathBuf>) -> Fido2DeviceSelection {
    match fido2_device {
        Some(new) => Fido2DeviceSelection::Explicit {
            new,
            existing: None,
        },
        None => Fido2DeviceSelection::Interactive,
    }
}

/// Builds the `Fido2DeviceSelection` for `enroll`'s pair of flags. Clap's
/// `requires` attributes on both fields already enforce all-or-nothing, so
/// `fido2_device.is_some()` implies `unlock_fido2_device.is_some()` here.
fn fido2_selection_for_enroll(
    fido2_device: Option<PathBuf>,
    unlock_fido2_device: Option<PathBuf>,
) -> Fido2DeviceSelection {
    match fido2_device {
        Some(new) => Fido2DeviceSelection::Explicit {
            new,
            existing: unlock_fido2_device,
        },
        None => Fido2DeviceSelection::Interactive,
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum CliFilesystem {
    Ext4,
}

impl From<CliFilesystem> for Filesystem {
    fn from(value: CliFilesystem) -> Self {
        match value {
            CliFilesystem::Ext4 => Filesystem::Ext4,
        }
    }
}

/// Whether a line the user typed at the wipe-confirmation prompt counts as
/// consent — exactly `yes`, ignoring surrounding whitespace/newline. Split
/// out from `confirm_device_wipe` so this comparison is unit-testable
/// without going through real stdin.
pub fn confirms_wipe(input: &str) -> bool {
    input.trim() == "yes"
}

/// Prints the wipe/data-loss warning naming `path` and reads a line from
/// stdin, requiring the user to type exactly `yes` to proceed. Deliberately
/// interactive rather than a scriptable `--confirm`/`--yes` flag: a flag
/// could be pasted/scripted without the user ever reading the warning, which
/// would undercut the whole point of a wrong-device confirmation gate.
/// `domain::workflows::create::run` enforces the check itself regardless —
/// this is never trusted as the sole gate.
fn confirm_device_wipe(path: &Path) -> bool {
    println!(
        "WARNING: this will erase any existing data on {} and format it as a new encrypted tomb.",
        path.display()
    );
    print!("Type \"yes\" to continue: ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    io::stdin().read_line(&mut input).is_ok() && confirms_wipe(&input)
}

/// Builds the adapter, runs `create::run`, and reports the result — shared by
/// both `create` subcommands. `announce` gates the "Creating tomb..." message:
/// the Device arm passes `false` when the user already declined the wipe
/// confirmation, so the message doesn't imply work started when the call is
/// about to fail immediately on `domain`'s own confirmation check.
fn run_create(
    target: CreateTarget,
    filesystem: Filesystem,
    fido2_selection: Fido2DeviceSelection,
    display_path: &str,
    announce: bool,
) {
    let adapter = ExecAdapter::default();

    if announce {
        println!("Creating tomb at {display_path}...");
    }

    if let Err(err) = create::run(
        target,
        filesystem,
        fido2_selection,
        &adapter,
        &adapter,
        &adapter,
    ) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("Tomb created at {display_path}.");
}

/// Builds the adapter, runs `unlock::run`, and reports the result. Checks
/// preflight first so a missing-dependency error surfaces before the
/// touch-key prompt below, rather than after it — `unlock::run` re-checks
/// preflight itself regardless (AD-4), so this is a cheap, side-effect-free
/// re-check, not a bypass. Prints a plain-language line before calling
/// `unlock::run` (FR5/NFR3 — prompts assume zero FIDO2 knowledge);
/// `cryptsetup`'s own systemd-fido2 prompt text still appears as-is via
/// inherited stdio, same accepted limitation as `enroll`'s
/// `systemd-cryptenroll` prompt, and is left untranslated here (Story 1.8's
/// job, not this one's).
fn run_unlock(path: PathBuf) {
    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("Touch your security key now (you may also be asked for its PIN).");

    match unlock::run(&path, &adapter, &adapter, &adapter) {
        Ok(mountpoint) => println!("Tomb unlocked and mounted at {}.", mountpoint.display()),
        Err(err) => {
            eprintln!("{}", ux::translate(&err));
            std::process::exit(1);
        }
    }
}

/// Builds the adapter, runs `enroll::run`, and reports the result. Mirrors
/// `run_unlock`'s shape: preflight check first, then a plain-language intro
/// (FR5/NFR3). The device-selection prompts (waiting for both keys to be
/// plugged in simultaneously, then "Which is your EXISTING key?"/"Which is
/// your NEW key?" by numbered index) happen inside
/// `Fido2Backend::enroll_fido2_key` itself (it's the only layer that can
/// identify which physical key is which, via `fido2-token -L`), and
/// `systemd-cryptenroll`'s own untranslated touch/PIN prompt text still
/// appears as-is via inherited stdio, the same accepted limitation
/// `run_unlock` documents for its own prompt.
fn run_enroll(path: PathBuf, label: String, fido2_selection: Fido2DeviceSelection) {
    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("Enrolling a new security key on this tomb. Follow the prompts below.");

    match enroll::run(&path, label, fido2_selection, &adapter, &adapter, &adapter) {
        Ok(()) => println!("New key enrolled."),
        Err(err) => {
            eprintln!("{}", ux::translate(&err));
            std::process::exit(1);
        }
    }
}

/// Whether a line the user typed at the revoke-confirmation prompt counts as
/// consent — exactly `yes`, ignoring surrounding whitespace/newline. Split
/// out from `confirm_revoke` so this comparison is unit-testable without
/// going through real stdin. Mirrors `confirms_wipe`'s convention.
pub fn confirms_revoke(input: &str) -> bool {
    input.trim() == "yes"
}

/// Prints the revoke warning naming `label`/`path` and reads a line from
/// stdin, requiring the user to type exactly `yes` to proceed. Revoke is
/// irreversible (the token and keyslot are gone once removed) and has no
/// other confirmation gate — a typo'd `--label` that happens to match a
/// different real, enrolled key would otherwise proceed straight to removal
/// with no warning. Deliberately interactive rather than a scriptable
/// `--confirm`/`--yes` flag, same reasoning as `confirm_device_wipe`.
fn confirm_revoke(label: &str, path: &Path) -> bool {
    println!(
        "WARNING: this will permanently revoke key \"{label}\" from {}.",
        path.display()
    );
    print!("Type \"yes\" to continue: ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    io::stdin().read_line(&mut input).is_ok() && confirms_revoke(&input)
}

/// Builds the adapter, runs `revoke::run`, and reports the result. Mirrors
/// `run_enroll`'s shape: preflight check first, then a plain-language intro
/// (FR5/NFR3). Unlike `enroll`, revoke never touches a physical key, so there
/// is no touch/PIN prompt to warn about beforehand — but it does need its own
/// confirmation gate (see `confirm_revoke`).
fn run_revoke(path: PathBuf, label: String) {
    if !confirm_revoke(&label, &path) {
        println!("Revoke cancelled.");
        return;
    }

    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("Revoking key \"{label}\" from this tomb.");

    match revoke::run(&path, &label, &adapter, &adapter, &adapter) {
        Ok(()) => println!("Key \"{label}\" revoked."),
        Err(err) => {
            eprintln!("{}", ux::translate(&err));
            std::process::exit(1);
        }
    }
}

pub fn run() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Create { mode } => match mode {
            CreateMode::File {
                path,
                size,
                filesystem,
                fido2_device,
            } => {
                let display_path = path.display().to_string();
                let target = CreateTarget::File { path, size };
                let selection = fido2_selection_for_create(fido2_device);
                run_create(target, filesystem.into(), selection, &display_path, true);
            }
            CreateMode::Device {
                path,
                size,
                filesystem,
                fido2_device,
            } => {
                let confirmed = confirm_device_wipe(&path);
                let display_path = path.display().to_string();
                let target = CreateTarget::Device {
                    path,
                    size,
                    confirmed,
                };
                let selection = fido2_selection_for_create(fido2_device);
                run_create(
                    target,
                    filesystem.into(),
                    selection,
                    &display_path,
                    confirmed,
                );
            }
        },
        Commands::Unlock { path } => run_unlock(path),
        Commands::Enroll {
            path,
            label,
            fido2_device,
            unlock_fido2_device,
        } => {
            let selection = fido2_selection_for_enroll(fido2_device, unlock_fido2_device);
            run_enroll(path, label, selection);
        }
        Commands::Revoke { path, label } => run_revoke(path, label),
    }
}
