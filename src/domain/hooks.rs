//! Pure-logic support for per-volume `bind-hooks`/`exec-hooks` automation
//! (AD-14). Mirrors `keyslot_guard.rs`'s role as a focused, pure-logic
//! `domain` module — no I/O beyond `resolve_bind_hook_entry`'s direct
//! `std::fs::canonicalize` call, the same precedent `mapping_name.rs`
//! already sets for a plain, non-privileged, non-subprocess fs call.

use std::path::{Path, PathBuf};

use crate::domain::types::HookFileMeta;
use crate::ports::filesystem_backend::FilesystemBackend;

/// One parsed line from a volume's `bind-hooks` file: a volume-root-relative
/// source and a `$HOME`-relative destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindHookEntry {
    pub source_relative: String,
    pub dest_relative: String,
}

/// Why a `bind-hooks` entry was skipped (AC #2) — not a hard failure, only
/// ever surfaced via `HookWarning::BindHookSkipped`. The first four variants
/// come from `resolve_bind_hook_entry`; `BindMountFailed` is set by
/// `unlock::run` itself when `resolve_bind_hook_entry` succeeded but the
/// subsequent `FilesystemBackend::bind_mount` call failed — kept as its own
/// honest variant rather than folding it into an unrelated reason, since
/// this codebase's `HookWarning`s are typed for exactly this reason (AD-19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindHookSkipReason {
    SourceMissing,
    DestMissing,
    SourceEscapesVolumeRoot,
    DestEscapesHome,
    BindMountFailed,
}

/// A non-fatal, informational event during `unlock`/`close`'s hooks step
/// (same category as `CreateStage`/`ResizeStage`, AD-19) — rendered by
/// `cli::ux::translate_hook_warning`, never composed as text inside `domain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookWarning {
    BindHookSkipped {
        source: String,
        dest: String,
        reason: BindHookSkipReason,
    },
    ExecHookNonZeroExit {
        path: PathBuf,
        exit_code: Option<i32>,
    },
    BindHooksFileUnreadable {
        path: PathBuf,
    },
}

/// Why an `exec-hooks` file was rejected outright (AC #3/#4) — a hard error,
/// checked in this exact order (AC #3's enumeration order); the *first*
/// violation found is returned, not all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookRejectionReason {
    NotARegularFile,
    NotExecutable,
    WrongOwner,
    WorldWritable,
}

/// Parses a `bind-hooks` file's contents: each non-blank line splits on
/// whitespace into exactly two tokens (source, dest). A line with anything
/// other than exactly two whitespace-separated tokens is silently dropped —
/// not a documented case in hooks.md, treated as malformed/ignorable rather
/// than erroring the whole open over a stray blank or malformed line.
pub fn parse_bind_hooks(content: &str) -> Vec<BindHookEntry> {
    content
        .lines()
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            let source = tokens.next()?;
            let dest = tokens.next()?;
            if tokens.next().is_some() {
                return None;
            }
            Some(BindHookEntry {
                source_relative: source.to_string(),
                dest_relative: dest.to_string(),
            })
        })
        .collect()
}

/// AC #3/#4's guardrail: checks `meta` in the exact order AC #3 enumerates
/// them, returning the first violation found.
pub fn exec_hook_rejection(meta: &HookFileMeta) -> Option<HookRejectionReason> {
    if meta.is_symlink || !meta.is_regular_file {
        Some(HookRejectionReason::NotARegularFile)
    } else if !meta.is_executable {
        Some(HookRejectionReason::NotExecutable)
    } else if !meta.owned_by_invoking_user_or_root {
        Some(HookRejectionReason::WrongOwner)
    } else if meta.is_world_writable {
        Some(HookRejectionReason::WorldWritable)
    } else {
        None
    }
}

/// Resolves one `bind-hooks` entry to concrete, containment-checked
/// `(source, dest)` paths (AC #1/#2). Joins `entry.source_relative` onto
/// `volume_root` and `entry.dest_relative` onto `home_dir`; checks
/// `fs.path_exists` on both before canonicalizing (reusing the existing AD-9
/// method, not a redundant new one); canonicalizes both directly via
/// `std::fs::canonicalize` (same precedent as `mapping_name::mapping_name`'s
/// direct call); confirms the canonicalized source starts with the
/// canonicalized `volume_root` and the canonicalized dest starts with the
/// canonicalized `home_dir` — canonicalize resolves `..`/symlinks before the
/// `starts_with` check runs, so an escaping entry canonicalizes to a path
/// outside the root and fails containment, rejecting `..`/absolute-path
/// escapes without needing separate string detection.
pub fn resolve_bind_hook_entry(
    entry: &BindHookEntry,
    volume_root: &Path,
    home_dir: &Path,
    fs: &dyn FilesystemBackend,
) -> Result<(PathBuf, PathBuf), BindHookSkipReason> {
    let source = volume_root.join(&entry.source_relative);
    let dest = home_dir.join(&entry.dest_relative);

    if !fs.path_exists(&source) {
        return Err(BindHookSkipReason::SourceMissing);
    }
    if !fs.path_exists(&dest) {
        return Err(BindHookSkipReason::DestMissing);
    }

    let canonical_source =
        std::fs::canonicalize(&source).map_err(|_| BindHookSkipReason::SourceMissing)?;
    let canonical_dest =
        std::fs::canonicalize(&dest).map_err(|_| BindHookSkipReason::DestMissing)?;

    let canonical_volume_root = std::fs::canonicalize(volume_root)
        .map_err(|_| BindHookSkipReason::SourceEscapesVolumeRoot)?;
    let canonical_home_dir =
        std::fs::canonicalize(home_dir).map_err(|_| BindHookSkipReason::DestEscapesHome)?;

    if !canonical_source.starts_with(&canonical_volume_root) {
        return Err(BindHookSkipReason::SourceEscapesVolumeRoot);
    }
    if !canonical_dest.starts_with(&canonical_home_dir) {
        return Err(BindHookSkipReason::DestEscapesHome);
    }

    Ok((canonical_source, canonical_dest))
}
