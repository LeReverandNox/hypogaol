use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::adapters::exec::ExecAdapter;
use crate::cli::ux;
use crate::domain::preflight;
use crate::domain::types::{CreateTarget, Filesystem};
use crate::domain::workflows::create::{self, MIN_TOMB_SIZE_BYTES};
use crate::domain::workflows::unlock;

// `name`/`version`/`about` are populated by clap from this crate's own
// `CARGO_PKG_*` metadata (AD-13) — never a hardcoded product-name literal.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
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
        #[arg(long)]
        path: PathBuf,
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
        #[arg(long)]
        path: PathBuf,

        /// Size to allocate for the backing file (e.g. 500M, 10G, or a plain
        /// byte count)
        #[arg(long, value_parser = parse_size)]
        size: u64,

        /// Filesystem to create inside the tomb
        #[arg(long, value_enum, default_value = "ext4")]
        filesystem: CliFilesystem,
    },

    /// Create a new tomb on an existing raw device or partition
    Device {
        /// Path to the target device or partition; must not already carry a
        /// LUKS2 header
        #[arg(long)]
        path: PathBuf,

        /// Size to use (e.g. 500M, 10G, or a plain byte count); defaults to
        /// the device's full capacity when omitted
        #[arg(long, value_parser = parse_size)]
        size: Option<u64>,

        /// Filesystem to create inside the tomb
        #[arg(long, value_enum, default_value = "ext4")]
        filesystem: CliFilesystem,
    },
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
fn run_create(target: CreateTarget, filesystem: Filesystem, display_path: &str, announce: bool) {
    let adapter = ExecAdapter::default();

    if announce {
        println!("Creating tomb at {display_path}...");
    }

    if let Err(err) = create::run(target, filesystem, &adapter, &adapter, &adapter) {
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

pub fn run() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Create { mode } => match mode {
            CreateMode::File {
                path,
                size,
                filesystem,
            } => {
                let display_path = path.display().to_string();
                let target = CreateTarget::File { path, size };
                run_create(target, filesystem.into(), &display_path, true);
            }
            CreateMode::Device {
                path,
                size,
                filesystem,
            } => {
                let confirmed = confirm_device_wipe(&path);
                let display_path = path.display().to_string();
                let target = CreateTarget::Device {
                    path,
                    size,
                    confirmed,
                };
                run_create(target, filesystem.into(), &display_path, confirmed);
            }
        },
        Commands::Unlock { path } => run_unlock(path),
    }
}
