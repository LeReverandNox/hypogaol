use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::adapters::exec::ExecAdapter;
use crate::cli::ux;
use crate::domain::hooks::HookWarning;
use crate::domain::preflight;
use crate::domain::progress::{CreateStage, ResizeStage};
use crate::domain::types::{CreateTarget, Filesystem};
use crate::domain::workflows::close;
use crate::domain::workflows::close_all;
use crate::domain::workflows::create::{self, MIN_VOLUME_SIZE_BYTES};
use crate::domain::workflows::enroll;
use crate::domain::workflows::info;
use crate::domain::workflows::resize;
use crate::domain::workflows::revoke;
use crate::domain::workflows::slam;
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
    /// Create a new volume
    Create {
        #[command(subcommand)]
        mode: CreateMode,
    },

    /// Unlock and mount an existing volume
    Unlock {
        /// Path to the existing volume's backing file or device
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// Unlock read-only — refuses all writes at both the block-device
        /// and filesystem level
        #[arg(long)]
        read_only: bool,

        /// Skip bind-hooks and exec-hooks processing for this command.
        #[arg(long)]
        skip_hooks: bool,
    },

    /// Enroll an additional FIDO2 key on an existing volume
    Enroll {
        /// Path to the existing volume's backing file or device
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

        /// Require the device's own fingerprint/PIN check at unlock time,
        /// not touch alone. Fails enrollment outright if the device has no
        /// on-device verification method (e.g. no fingerprint sensor)
        #[arg(long)]
        user_verification: bool,
    },

    /// Revoke a FIDO2 key's keyslot from an existing volume
    Revoke {
        /// Path to the existing volume's backing file or device
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// Label of the enrolled key to revoke
        #[arg(long, value_parser = parse_label)]
        label: String,
    },

    /// Unmount an unlocked volume's filesystem and re-lock its LUKS2 mapping
    Close {
        /// Path to the existing volume's backing file or device
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// Skip bind-hooks and exec-hooks processing for this command.
        #[arg(long)]
        skip_hooks: bool,
    },

    /// Close every currently open/unlocked volume in one command
    CloseAll {
        /// Skip bind-hooks and exec-hooks processing for every volume closed in this batch.
        #[arg(long)]
        skip_hooks: bool,
    },

    /// Grow an existing volume's LUKS2 mapping and filesystem to a larger size
    Resize {
        /// Path to the existing volume's backing file or device
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// New total size for the volume (e.g. 20G) — must be larger than its
        /// current size; resize is grow-only
        #[arg(long, value_parser = parse_size)]
        size: u64,
    },

    /// Emergency: force-close every open volume immediately, escalating past
    /// any busy mount, with zero confirmation
    Slam,

    /// Show a volume's technical info, including its enrolled FIDO2 keys
    Info {
        /// Path to the existing volume's backing file or device
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,
    },
}

// AD-9 requires the CLI to resolve an explicit target mode, never inferred by
// sniffing the path — hence two distinct subcommands rather than a single
// flat `create` with an optional device flag.
#[derive(Subcommand)]
enum CreateMode {
    /// Create a new file-backed volume
    File {
        /// Destination path for the backing file; must not already exist
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// Size to allocate for the backing file (e.g. 500M, 10G, or a plain
        /// byte count)
        #[arg(long, value_parser = parse_size)]
        size: u64,

        /// Filesystem to create inside the volume
        #[arg(long, value_enum, default_value = "ext4")]
        filesystem: CliFilesystem,

        /// Hidraw path (e.g. /dev/hidraw1) of the security key to enroll —
        /// for unattended/scripted use. Omit to be prompted interactively.
        #[arg(long)]
        fido2_device: Option<PathBuf>,

        /// Require the device's own fingerprint/PIN check at unlock time,
        /// not touch alone. Fails enrollment outright if the device has no
        /// on-device verification method (e.g. no fingerprint sensor)
        #[arg(long)]
        user_verification: bool,
    },

    /// Create a new volume on an existing raw device or partition
    Device {
        /// Path to the target device or partition; must not already carry a
        /// LUKS2 header
        #[arg(allow_hyphen_values = true)]
        path: PathBuf,

        /// Size to use (e.g. 500M, 10G, or a plain byte count); defaults to
        /// the device's full capacity when omitted
        #[arg(long, value_parser = parse_size)]
        size: Option<u64>,

        /// Filesystem to create inside the volume
        #[arg(long, value_enum, default_value = "ext4")]
        filesystem: CliFilesystem,

        /// Hidraw path (e.g. /dev/hidraw1) of the security key to enroll —
        /// for unattended/scripted use. Omit to be prompted interactively.
        #[arg(long)]
        fido2_device: Option<PathBuf>,

        /// Require the device's own fingerprint/PIN check at unlock time,
        /// not touch alone. Fails enrollment outright if the device has no
        /// on-device verification method (e.g. no fingerprint sensor)
        #[arg(long)]
        user_verification: bool,
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

    if bytes < MIN_VOLUME_SIZE_BYTES {
        return Err(format!(
            "size {input:?} is too small (minimum is {MIN_VOLUME_SIZE_BYTES} bytes / {}M)",
            MIN_VOLUME_SIZE_BYTES / (1024 * 1024)
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
        "WARNING: this will erase any existing data on {} and format it as a new encrypted volume.",
        path.display()
    );
    print!("Type \"yes\" to continue: ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    io::stdin().read_line(&mut input).is_ok() && confirms_wipe(&input)
}

/// Builds the adapter, runs `create::run`, and reports the result — shared by
/// both `create` subcommands. `announce` gates the "Creating volume..." message:
/// the Device arm passes `false` when the user already declined the wipe
/// confirmation, so the message doesn't imply work started when the call is
/// about to fail immediately on `domain`'s own confirmation check.
fn run_create(
    target: CreateTarget,
    filesystem: Filesystem,
    user_verification: bool,
    fido2_selection: Fido2DeviceSelection,
    display_path: &str,
    announce: bool,
) {
    let adapter = ExecAdapter::default();

    if announce {
        println!("Creating volume at {display_path}...");
    }

    if let Err(err) = create::run(
        target,
        filesystem,
        user_verification,
        fido2_selection,
        &|stage: CreateStage| println!("{}", ux::translate_create_stage(&stage)),
        &adapter,
        &adapter,
        &adapter,
    ) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("Volume created at {display_path}.");
}

/// Plain-language intro line printed before `unlock::run` (FR5/NFR3),
/// pulled out as a pure function so `run_unlock`'s read-only-specific text
/// is unit-testable without a stdout-capture harness.
pub fn unlock_intro_message(read_only: bool) -> String {
    let intro = "Touch your security key now (you may also be asked for its PIN).";
    if read_only {
        format!("{intro} Unlocking read-only — no changes will be saved.")
    } else {
        intro.to_string()
    }
}

/// Plain-language success line printed after a successful `unlock::run`,
/// pulled out for the same reason as `unlock_intro_message`.
pub fn unlock_success_message(read_only: bool, mountpoint: &Path) -> String {
    if read_only {
        format!(
            "Volume unlocked (read-only) and mounted at {}.",
            mountpoint.display()
        )
    } else {
        format!("Volume unlocked and mounted at {}.", mountpoint.display())
    }
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
/// Shared `warn` seam for `unlock`/`close`'s hooks step (AD-19) — prints the
/// translated `HookWarning` to stderr. A plain `fn` rather than a closure
/// defined at each call site, since both callers built the identical
/// one-liner independently.
fn print_hook_warning(w: HookWarning) {
    eprintln!("{}", ux::translate_hook_warning(&w));
}

fn run_unlock(path: PathBuf, read_only: bool, skip_hooks: bool) {
    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("{}", unlock_intro_message(read_only));

    match unlock::run(
        &path,
        read_only,
        skip_hooks,
        &print_hook_warning,
        &adapter,
        &adapter,
        &adapter,
    ) {
        Ok(mountpoint) => println!("{}", unlock_success_message(read_only, &mountpoint)),
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
fn run_enroll(
    path: PathBuf,
    label: String,
    fido2_selection: Fido2DeviceSelection,
    user_verification: bool,
) {
    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("Enrolling a new security key on this volume. Follow the prompts below.");

    match enroll::run(
        &path,
        label,
        fido2_selection,
        user_verification,
        &adapter,
        &adapter,
        &adapter,
    ) {
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

    println!("Revoking key \"{label}\" from this volume.");

    match revoke::run(&path, &label, &adapter, &adapter, &adapter) {
        Ok(()) => println!("Key \"{label}\" revoked."),
        Err(err) => {
            eprintln!("{}", ux::translate(&err));
            std::process::exit(1);
        }
    }
}

/// Builds the adapter, runs `close::run`, and reports the result. Mirrors
/// `run_unlock`'s shape: preflight check first, then a plain-language intro
/// (FR5/NFR3). No confirmation prompt — unlike `create`'s wipe warning or
/// `revoke`'s irreversible-key warning, closing is fully reversible (a normal
/// `unlock` with the same FIDO2 key gets you back in), so it doesn't fit the
/// pattern that justifies those two interactive gates.
fn run_close(path: PathBuf, skip_hooks: bool) {
    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("Closing this volume.");

    match close::run(
        &path,
        skip_hooks,
        &print_hook_warning,
        &adapter,
        &adapter,
        &adapter,
    ) {
        Ok(()) => println!("Volume closed."),
        Err(err) => {
            eprintln!("{}", ux::translate(&err));
            std::process::exit(1);
        }
    }
}

/// Builds the adapter, runs `close_all::run`, and reports the result.
/// Mirrors `run_close`'s shape: preflight check first, same
/// `print_hook_warning` seam, no confirmation prompt (same reasoning as
/// `run_close`'s own doc comment — closing is fully reversible). Unlike
/// `run_close`, failures never abort early: every discovered mapping's
/// outcome is printed (AC #2) before `std::process::exit(1)` runs, so a
/// batch with some failures still reports every success alongside them.
fn run_close_all(skip_hooks: bool) {
    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("Checking for open volumes to close.");

    match close_all::run(
        skip_hooks,
        &print_hook_warning,
        &adapter,
        &adapter,
        &adapter,
    ) {
        Ok(results) => {
            if results.is_empty() {
                println!("No volumes are currently open.");
                return;
            }

            let mut any_failed = false;
            for (mapper, result) in results {
                match result {
                    Ok(()) => println!("Closed {}.", mapper.source_path.display()),
                    Err(err) => {
                        any_failed = true;
                        eprintln!(
                            "Failed to close {}: {}",
                            mapper.source_path.display(),
                            ux::translate(&err)
                        );
                    }
                }
            }

            if any_failed {
                std::process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("{}", ux::translate(&err));
            std::process::exit(1);
        }
    }
}

/// Builds the adapter, runs `slam::run`, and reports the result. Mirrors
/// `run_close_all`'s shape: preflight check first, same `print_hook_warning`
/// seam, same per-mapping success/failure reporting loop with a single
/// `std::process::exit(1)` after every line is printed — but with **no**
/// confirmation prompt of any kind and no pre-run "checking" framing (AC #2):
/// the intro line reads as immediate/urgent plain language (NFR3), never
/// implying a pause or a check step exists.
fn run_slam() {
    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!("Slamming every open volume — closing immediately, no confirmation.");

    match slam::run(&print_hook_warning, &adapter, &adapter, &adapter) {
        Ok(results) => {
            if results.is_empty() {
                println!("No volumes are currently open.");
                return;
            }

            let mut any_failed = false;
            for (mapper, result) in results {
                match result {
                    Ok(()) => println!("Closed {}.", mapper.source_path.display()),
                    Err(err) => {
                        any_failed = true;
                        eprintln!(
                            "Failed to close {}: {}",
                            mapper.source_path.display(),
                            ux::translate(&err)
                        );
                    }
                }
            }

            if any_failed {
                std::process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("{}", ux::translate(&err));
            std::process::exit(1);
        }
    }
}

/// Builds the adapter, runs `resize::run`, and reports the result. Mirrors
/// `run_close`'s shape: preflight check first, then a plain-language intro
/// (FR5/NFR3) — but unlike `close`, `resize` calls `luks.open` (a real FIDO2
/// touch), so it also warns about that beforehand, mirroring `run_unlock`'s
/// own intro line. No confirmation prompt: like `close`, growing is
/// non-destructive (existing data is preserved, AC #4), so it doesn't fit
/// the pattern that justifies `create`'s wipe warning or `revoke`'s
/// irreversible-key warning.
fn run_resize(path: PathBuf, new_size: u64) {
    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    println!(
        "Growing this volume. Touch your security key now (you may also be asked for its PIN)."
    );

    match resize::run(
        &path,
        new_size,
        &|stage: ResizeStage| println!("{}", ux::translate_resize_stage(&stage)),
        &adapter,
        &adapter,
        &adapter,
    ) {
        Ok(()) => println!("Volume grown to {new_size} bytes."),
        Err(err) => {
            eprintln!("{}", ux::translate(&err));
            std::process::exit(1);
        }
    }
}

/// Builds the adapter, runs `info::run`, and reports the result. Mirrors
/// `run_close`'s shape: preflight check first, then the call, then report.
/// No intro line is needed beforehand — unlike `unlock`/`resize`, info never
/// touches a physical FIDO2 key, so there's no touch/PIN prompt to warn
/// about. Only `key_label` per keyslot is printed (AC #2) — `credential_id`,
/// `created_at`, and `filesystem` stay internal.
fn run_info(path: PathBuf) {
    let adapter = ExecAdapter::default();

    if let Err(err) = preflight::check(&adapter, &adapter, &adapter) {
        eprintln!("{}", ux::translate(&err));
        std::process::exit(1);
    }

    match info::run(&path, &adapter, &adapter, &adapter) {
        Ok(keyslots) => {
            println!("Enrolled FIDO2 keys for {}:", path.display());
            for keyslot in keyslots {
                println!("  - {}", keyslot.key_label);
            }
        }
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
                user_verification,
            } => {
                let display_path = path.display().to_string();
                let target = CreateTarget::File { path, size };
                let selection = fido2_selection_for_create(fido2_device);
                run_create(
                    target,
                    filesystem.into(),
                    user_verification,
                    selection,
                    &display_path,
                    true,
                );
            }
            CreateMode::Device {
                path,
                size,
                filesystem,
                fido2_device,
                user_verification,
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
                    user_verification,
                    selection,
                    &display_path,
                    confirmed,
                );
            }
        },
        Commands::Unlock {
            path,
            read_only,
            skip_hooks,
        } => run_unlock(path, read_only, skip_hooks),
        Commands::Enroll {
            path,
            label,
            fido2_device,
            unlock_fido2_device,
            user_verification,
        } => {
            let selection = fido2_selection_for_enroll(fido2_device, unlock_fido2_device);
            run_enroll(path, label, selection, user_verification);
        }
        Commands::Revoke { path, label } => run_revoke(path, label),
        Commands::Close { path, skip_hooks } => run_close(path, skip_hooks),
        Commands::CloseAll { skip_hooks } => run_close_all(skip_hooks),
        Commands::Slam => run_slam(),
        Commands::Resize { path, size } => run_resize(path, size),
        Commands::Info { path } => run_info(path),
    }
}
