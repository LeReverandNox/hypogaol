//! Plain-language translation boundary (CAP-5): every `DomainError` is
//! rendered here into user-facing text — see `ARCHITECTURE-SPINE.md`'s
//! Design Paradigm. `domain` and `adapters` keep their raw, technical error
//! text untouched; only this module decides what the user actually reads.
//! Two deliberate exceptions pass raw text through as-is: `PreflightFailed`'s
//! per-item strings (already user-actionable, e.g. "cryptsetup binary not
//! found on PATH") and the `AdapterFailure` fallback's labeled "Technical
//! detail" line (kept for bug-report value when no category matches).

use crate::domain::errors::DomainError;

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
                String::from("tomb-fido2 can't run yet — a few things are missing:");
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
        return "tomb-fido2 couldn't read this tomb's key information. Make sure the path points \
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

    // `close`'s own `luks.close` failures (`cryptsetup close failed: ...`) —
    // checked before the generic `cryptsetup` bucket below for the same
    // "marker bleed" reason as the `luksKillSlot`/`token remove` guard above:
    // that bucket's touch/PIN-entry framing is meaningless for `close`, which
    // never touches a FIDO2 key (review finding, 2026-07-26).
    if inner.contains("cryptsetup close") {
        return "tomb-fido2 couldn't re-lock this tomb's LUKS2 volume. Make sure nothing is \
                still using it, then try again."
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
        return "tomb-fido2 couldn't unmount this tomb's filesystem. Make sure nothing is still \
                using it, then try again."
            .to_string();
    }

    // Mount/filesystem failures (`mkfs`, `mount`, `chmod`, mount-point
    // create/remove).
    if inner.contains("mount") || inner.contains("mkfs") {
        return "Your tomb unlocked, but tomb-fido2 couldn't mount its filesystem.".to_string();
    }

    // `mapping_name::mapping_name`'s canonicalization failure. The message is
    // `"failed to canonicalize {path}: {e}"` — split on the LAST ": " rather
    // than the first, so a path that itself legally contains ": " isn't
    // truncated at that embedded colon.
    if let Some(path) = inner.strip_prefix("failed to canonicalize ") {
        let path = path.rsplit_once(": ").map_or(path, |(path, _)| path);
        return format!("tomb-fido2 couldn't find {path}. Check the path and try again.");
    }

    // Device/file sizing failures (`blockdev --getsize64`,
    // `set_backing_file_size`, `remove_backing_file`).
    if inner.contains("blockdev")
        || inner.contains("failed to size")
        || inner.contains("failed to create")
        || inner.contains("failed to remove")
    {
        return "tomb-fido2 couldn't determine or set the size needed for this tomb.".to_string();
    }

    // Fallback: keeps AC #2's "no jargon leaks" promise for the primary line
    // while still surfacing the original text as a labeled technical detail,
    // rather than discarding information a bug report would need.
    format!(
        "Something unexpected happened while working with your tomb.\nTechnical detail: {inner}"
    )
}
