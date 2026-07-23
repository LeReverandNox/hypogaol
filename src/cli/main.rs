use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::adapters::exec::ExecAdapter;
use crate::domain::types::{CreateTarget, Filesystem};
use crate::domain::workflows::create;

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
    /// Create a new file-backed tomb
    Create {
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
}

/// Parses a size string with an optional K/M/G/T suffix (binary, powers of
/// 1024 — matching `resize2fs`/`lvreduce` convention) into a byte count. No
/// suffix means a plain byte count.
fn parse_size(input: &str) -> Result<u64, String> {
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

    number
        .checked_mul(multiplier)
        .ok_or_else(|| format!("size {input:?} is too large"))
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

pub fn run() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Create {
            path,
            size,
            filesystem,
        } => {
            let adapter = ExecAdapter::default();
            let display_path = path.display().to_string();
            let target = CreateTarget::File { path, size };

            println!("Creating tomb at {display_path}...");

            if let Err(err) = create::run(target, filesystem.into(), &adapter, &adapter, &adapter) {
                eprintln!("{err}");
                std::process::exit(1);
            }

            println!("Tomb created at {display_path}.");
        }
    }
}
