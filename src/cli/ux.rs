//! Plain-language translation boundary (CAP-5): every `DomainError` is
//! rendered here into user-facing text — see `ARCHITECTURE-SPINE.md`'s
//! Design Paradigm. `domain` and `adapters` keep their raw, technical error
//! text untouched; only this module decides what the user actually reads.
//! Two deliberate exceptions pass raw text through as-is: `PreflightFailed`'s
//! per-item strings (already user-actionable, e.g. "cryptsetup binary not
//! found on PATH") and the `AdapterFailure` fallback's labeled "Technical
//! detail" line (kept for bug-report value when no category matches).

use crate::domain::errors::DomainError;
use crate::domain::hooks::{BindHookSkipReason, HookRejectionReason, HookWarning};
use crate::domain::progress::{CreateStage, ResizeStage};

/// Translates any `DomainError` reachable from `create`/`unlock`/`enroll`/
/// `revoke` into a plain-language message. Exhaustive by construction: a 9th
/// `DomainError` variant added by a later epic fails to compile here until
/// it's given a translation, so jargon can never silently leak through an
/// unhandled arm.
pub fn translate(err: &DomainError) -> String {
    match err {
        DomainError::DestinationExists(path) => format!(
            "A tomb already exists at {}. Choose a different location, or unlock the existing one instead.",
            path.display()
        ),
        DomainError::DeviceAlreadyFormatted(path) => format!(
            "{} already has an encrypted tomb on it. If you meant to unlock it, use the unlock command instead.",
            path.display()
        ),
        DomainError::DeviceConfirmationRequired => {
            "Creating a tomb on a device erases everything on it. Please confirm the warning to continue."
                .to_string()
        }
        DomainError::DeviceSizeExceedsCapacity {
            path,
            requested,
            capacity,
        } => format!(
            "You asked for {requested} bytes, but {} only has {capacity} bytes available.",
            path.display()
        ),
        DomainError::DeviceTooSmall { path, size } => format!(
            "{} would only have {size} bytes for a tomb — that's too small to be usable.",
            path.display()
        ),
        DomainError::ResizeMustGrow {
            path,
            requested,
            current_size,
        } => format!(
            "{} is already {current_size} bytes. You asked for {requested} bytes — resize can \
             only grow a tomb, never shrink it.",
            path.display()
        ),
        DomainError::PreflightFailed(missing) => {
            let mut message =
                String::from("Hypogaol can't run yet — a few things are missing:");
            for item in missing {
                message.push_str("\n  - ");
                message.push_str(item);
            }
            message
        }
        // Only `domain::workflows::revoke` produces this.
        DomainError::LastKeyslotGuard => {
            "That's the last key that can unlock this tomb — revoking it would lock you out \
             permanently, so this was refused."
                .to_string()
        }
        DomainError::KeyNotFound(label) => format!(
            "No enrolled key is labeled {label:?}. Check the label (case-sensitive) and try again."
        ),
        DomainError::AdapterFailure(inner) => translate_adapter_failure(inner),
        DomainError::HookRejected { path, reason } => {
            let clause = match reason {
                HookRejectionReason::NotARegularFile => {
                    "it isn't a regular file (or is a symlink)"
                }
                HookRejectionReason::NotExecutable => "it isn't marked executable",
                HookRejectionReason::WrongOwner => {
                    "it isn't owned by you or root"
                }
                HookRejectionReason::WorldWritable => "it's writable by anyone on this system",
            };
            format!(
                "Hypogaol refused to run this tomb's exec-hooks script ({}) because {clause}. \
                 Nothing has changed.",
                path.display()
            )
        }
        DomainError::RollbackCleanupAlsoFailed {
            original,
            close_detail,
        } => format!(
            "{} (On top of that, Hypogaol couldn't re-lock the LUKS2 mapping while cleaning \
             up: {close_detail} — it may have been left open; run `close` to check.)",
            translate(original)
        ),
    }
}

/// Translates each `HookWarning` (AD-19's callback-seam precedent) into
/// plain-language text — what `unlock`/`close`'s `warn` closure calls.
/// Exhaustive by construction, same guarantee as `translate` above.
pub fn translate_hook_warning(w: &HookWarning) -> String {
    match w {
        HookWarning::BindHookSkipped {
            source,
            dest,
            reason,
        } => {
            let clause = match reason {
                BindHookSkipReason::SourceMissing => "its source path doesn't exist",
                BindHookSkipReason::DestMissing => "its destination path doesn't exist",
                BindHookSkipReason::SourceEscapesTombRoot => {
                    "its source path escapes the tomb"
                }
                BindHookSkipReason::DestEscapesHome => {
                    "its destination path escapes your home directory"
                }
                BindHookSkipReason::BindMountFailed => "the bind-mount itself failed",
            };
            format!("Skipped bind-hooks entry \"{source} -> {dest}\": {clause}.")
        }
        HookWarning::ExecHookNonZeroExit { path, exit_code } => match exit_code {
            Some(code) => format!(
                "This tomb's exec-hooks script ({}) exited with status {code} — continuing anyway.",
                path.display()
            ),
            None => format!(
                "This tomb's exec-hooks script ({}) was terminated by a signal — continuing anyway.",
                path.display()
            ),
        },
        HookWarning::BindHooksFileUnreadable { path } => format!(
            "Couldn't read this tomb's bind-hooks file ({}) — skipping all bind-hooks entries.",
            path.display()
        ),
    }
}

/// Translates each real `create` stage boundary (AD-19) into plain-language
/// text. Exhaustive by construction, same guarantee as `translate` above.
pub fn translate_create_stage(stage: &CreateStage) -> &'static str {
    match stage {
        CreateStage::AllocatingBackingFile => "Allocating the backing file...",
        CreateStage::FormattingLuks2 => "Formatting as LUKS2...",
        CreateStage::EnrollingFido2Key => {
            "Enrolling your FIDO2 key — touch it now (you may also be asked for its PIN)..."
        }
        CreateStage::CreatingFilesystem => "Creating the filesystem...",
    }
}

/// Translates each real `resize` stage boundary (AD-19) into plain-language
/// text. Exhaustive by construction, same guarantee as `translate` above.
pub fn translate_resize_stage(stage: &ResizeStage) -> &'static str {
    match stage {
        ResizeStage::GrowingBackingFile => "Growing the backing file...",
        ResizeStage::ResizingLuks2Mapping => "Resizing the LUKS2 mapping...",
        ResizeStage::GrowingFilesystem => "Growing the filesystem...",
    }
}

/// `AdapterFailure`'s message text varies by call site (it is the one
/// `DomainError` variant with no structured fields). Scope is bounded to
/// `create`'s, `unlock`'s, `enroll`'s, and `revoke`'s own call graphs, so the
/// reachable message shapes are finite — each category below is matched by
/// markers that appear verbatim in the real call sites
/// (`src/adapters/exec/mod.rs`, `src/domain/mapping_name.rs`) and translated
/// as a whole, rather than chasing a bespoke rewrite of every exact string.
fn translate_adapter_failure(inner: &str) -> String {
    // FIDO2 device-enumeration failures (`fido2-token -L`, via
    // `list_fido2_devices`/`wait_for_enough_fido2_devices`) — shared by
    // `create`'s bootstrap enroll, `enroll`'s own device-selection, AND
    // `unlock`'s presence-wait (`LuksBackend::open`), so this gets its own
    // workflow-neutral message rather than living inside `ENROLLMENT_MARKERS`
    // below (which would wrongly say "Enrolling..." for a plain `unlock`
    // failure). Checked first for the same reason `ENROLLMENT_MARKERS` is:
    // some of these messages don't contain "cryptsetup" at all, but none
    // should fall through to the generic buckets below either.
    if inner.contains("fido2-token") {
        return "Couldn't find your security key. Make sure it's plugged in, then try again."
            .to_string();
    }

    // `luksDump`/`dump_json_metadata` read failures — shared by `enroll`'s
    // label-uniqueness check AND `revoke`'s `list_fido2_keyslots` call, so
    // this gets its own workflow-neutral message rather than living inside
    // `ENROLLMENT_MARKERS` below (which would wrongly say "Enrolling..." for
    // a plain `revoke` failure). Checked first for the same reason as the
    // `fido2-token` bucket above.
    if inner.contains("luksDump") {
        return "Hypogaol couldn't read this tomb's key information. Make sure the path points \
                at a valid tomb, then try again."
            .to_string();
    }

    // FIDO2 enrollment failures (`enroll_fido2_key`'s own body — its
    // transient temp key file, its `systemd-cryptenroll` call, and its token
    // export/import/parse helpers) — checked first since several of these
    // messages also contain "cryptsetup" (e.g. "cryptsetup token export
    // failed", or the token-import call's `{cmd:?}` Debug-quoted
    // `"token" "import"`), which would otherwise be misclassified as a
    // bootstrap/open subprocess failure below.
    const ENROLLMENT_MARKERS: [&str; 8] = [
        "systemd-cryptenroll",
        "temporary key file",
        "transient bootstrap passphrase",
        "token export",
        "\"token\" \"import\"",
        "token JSON",
        "systemd-fido2 token",
        "fido2-credential",
    ];
    if ENROLLMENT_MARKERS
        .iter()
        .any(|marker| inner.contains(marker))
    {
        return "Enrolling your security key didn't complete. Make sure it's plugged in and \
                touch it when prompted, then try again."
            .to_string();
    }

    // `revoke`'s own `remove_key` failures (`cryptsetup token remove`/
    // `luksKillSlot`) — a non-interactive header edit, so the generic
    // `cryptsetup` bucket below's touch/PIN-entry framing would be
    // meaningless here. Checked before it for that reason.
    if inner.contains("luksKillSlot") || inner.contains("token remove") {
        return "Revoking that key didn't complete. Nothing has been changed — try again."
            .to_string();
    }

    // `resize`'s own success-path close failure
    // (`domain::workflows::resize::grow_succeeded_close_failed`) — the grow
    // itself already succeeded by the time this fires, so it needs its own
    // distinct message rather than the generic `cryptsetup close` bucket
    // below, which would wrongly suggest the whole resize failed. Checked
    // before that bucket since the wrapped detail still contains
    // "cryptsetup close" (review finding, 2026-07-26).
    if inner.contains("but failed to re-lock afterward") {
        return "Hypogaol grew this tomb successfully, but couldn't re-lock its LUKS2 volume \
                afterward. Your data and the new capacity are safe — run `close` to finish, or \
                try `resize` again."
            .to_string();
    }

    // `close`'s own `luks.close` failures (`cryptsetup close failed: ...`) —
    // checked before the generic `cryptsetup` bucket below for the same
    // "marker bleed" reason as the `luksKillSlot`/`token remove` guard above:
    // that bucket's touch/PIN-entry framing is meaningless for `close`, which
    // never touches a FIDO2 key (review finding, 2026-07-26).
    if inner.contains("cryptsetup close") {
        return "Hypogaol couldn't re-lock this tomb's LUKS2 volume. Make sure nothing is \
                still using it, then try again."
            .to_string();
    }

    // `resize`'s own `luks.resize` failures (`cryptsetup resize --token-only
    // failed for ...` / `failed to run cryptsetup resize: ...`) — checked
    // before the generic `cryptsetup` bucket below since both of these
    // messages also contain "cryptsetup" (marker-bleed guard, the same class
    // of bug the Epic 2 retro flagged: 3 real bugs from this pattern so far).
    // A resize failure gets its own message rather than that bucket's
    // create/unlock-flavored framing, even though it's also a touch/PIN
    // timing issue (this call re-authenticates via the FIDO2 token, Task 0's
    // spike finding).
    if inner.contains("cryptsetup resize") {
        return "Hypogaol couldn't resize this tomb's LUKS2 volume — your security key or its \
                PIN may not have been accepted in time."
            .to_string();
    }

    // `luksOpen`'s own "no remaining space for data" failure — reachable if
    // `MIN_TOMB_SIZE_BYTES` is ever loosened again without re-verifying it
    // leaves real payload room behind the LUKS2 header (confirmed
    // empirically, 2026-07-27: a tomb sized exactly to the header offset
    // passes size validation but fails here). Checked before the generic
    // `cryptsetup` bucket below for the same marker-bleed reason as every
    // other dedicated branch in this function — this is a sizing problem,
    // not a touch/PIN timing issue.
    if inner.contains("too small for activation") {
        return "This tomb's size leaves no room for a filesystem once the encryption header is \
                accounted for. Choose a larger size and try again."
            .to_string();
    }

    // `cryptsetup` subprocess failures during create's bootstrap
    // (`luksFormat`/`luksOpen`/`resize`) and unlock's `open` — includes the
    // `{cmd:?}` Debug-format argv dump, the single most jargon-dense string
    // reachable from `create`.
    if inner.contains("cryptsetup") {
        return "Something went wrong while unlocking or creating your tomb — your security key \
                or its PIN may not have been accepted in time."
            .to_string();
    }

    // `close`'s own `umount` failures (`ExecAdapter::umount`'s `findmnt`/
    // `umount` calls) — checked before the `mount`/`mkfs` bucket below since
    // "umount".contains("mount") is true as a plain substring, which would
    // otherwise misclassify these as unlock's own mount failure (the
    // "marker bleed" bug class the Epic 2 retro flagged: 3 real bugs from
    // this pattern already). "No active mapping" (the tomb was never
    // unlocked, or a prior `close` already fully completed) and "not
    // currently mounted" (unmounted already, but still open — the
    // partial-failure retry case `close::run` now recovers from) each get
    // their own distinct message rather than reading like a generic close
    // failure (review finding, 2026-07-26).
    if inner.contains("no active mapping") {
        return "This tomb doesn't appear to be unlocked right now — run unlock first.".to_string();
    }
    if inner.contains("not currently mounted") {
        return "This tomb doesn't look like it's currently mounted.".to_string();
    }
    if inner.contains("umount") || inner.contains("findmnt") {
        return "Hypogaol couldn't unmount this tomb's filesystem. Make sure nothing is still \
                using it, then try again."
            .to_string();
    }

    // `resize`'s own `fs.growfs` failures (`resize2fs failed: ...` /
    // `failed to run resize2fs: ...`) — checked before the generic
    // `mount`/`mkfs` bucket below since "resize2fs" contains neither
    // "mount" nor "mkfs" and would otherwise fall all the way through to
    // the unhelpful generic fallback (marker-bleed guard).
    if inner.contains("resize2fs") {
        return "Hypogaol grew this tomb's volume, but couldn't grow its filesystem to match."
            .to_string();
    }

    // `resize`'s own `growfs`'s `e2fsck` pre-check failures (`e2fsck -f
    // failed: ...` / `failed to run e2fsck: ...`) — checked before the
    // generic `mount`/`mkfs` bucket and fallback below since neither string
    // contains "resize2fs", "mount", or "mkfs" and would otherwise fall all
    // the way through to the unhelpful generic fallback (marker-bleed
    // guard; review finding, 2026-07-26).
    if inner.contains("e2fsck") {
        return "Hypogaol grew this tomb's volume, but couldn't check its filesystem before \
                growing it to match."
            .to_string();
    }

    // Mount/filesystem failures (`mkfs`, `mount`, `chmod`, mount-point
    // create/remove).
    if inner.contains("mount") || inner.contains("mkfs") {
        return "Your tomb unlocked, but Hypogaol couldn't mount its filesystem.".to_string();
    }

    // `mapping_name::mapping_name`'s canonicalization failure. The message is
    // `"failed to canonicalize {path}: {e}"` — split on the LAST ": " rather
    // than the first, so a path that itself legally contains ": " isn't
    // truncated at that embedded colon.
    if let Some(path) = inner.strip_prefix("failed to canonicalize ") {
        let path = path.rsplit_once(": ").map_or(path, |(path, _)| path);
        return format!("Hypogaol couldn't find {path}. Check the path and try again.");
    }

    // Device/file sizing failures (`blockdev --getsize64`,
    // `set_backing_file_size`, `remove_backing_file`, `is_block_device`'s
    // own stat, and `resize`'s pre-grow symlink/regular-file check — added
    // for Story 3.2's grow-branch, review finding, 2026-07-26).
    if inner.contains("blockdev")
        || inner.contains("failed to size")
        || inner.contains("failed to create")
        || inner.contains("failed to remove")
        || inner.contains("failed to stat")
        || inner.contains("is not a regular file")
        || inner.contains("failed to open")
    {
        return "Hypogaol couldn't determine or set the size needed for this tomb.".to_string();
    }

    // Fallback: keeps AC #2's "no jargon leaks" promise for the primary line
    // while still surfacing the original text as a labeled technical detail,
    // rather than discarding information a bug report would need.
    format!(
        "Something unexpected happened while working with your tomb.\nTechnical detail: {inner}"
    )
}
