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

        /// Size in bytes to allocate for the backing file
        #[arg(long)]
        size: u64,

        /// Filesystem to create inside the tomb
        #[arg(long, value_enum)]
        filesystem: CliFilesystem,
    },
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
        Commands::Create { path, size, filesystem } => {
            let adapter = ExecAdapter::default();
            let target = CreateTarget::File { path, size };

            if let Err(err) = create::run(target, filesystem.into(), &adapter, &adapter, &adapter) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }
}
