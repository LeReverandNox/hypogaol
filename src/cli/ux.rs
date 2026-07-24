//! Plain-language translation boundary (CAP-5): every `DomainError` is
//! rendered here into user-facing text with no cryptsetup/systemd/FIDO2
//! jargon — see `ARCHITECTURE-SPINE.md`'s Design Paradigm. `domain` and
//! `adapters` keep their raw, technical error text untouched; only this
//! module decides what the user actually reads.

use crate::domain::errors::DomainError;

/// Translates any `DomainError` reachable from `create`/`unlock` into a
/// plain-language message. Exhaustive by construction: a 9th `DomainError`
/// variant added by a later epic fails to compile here until it's given a
/// translation, so jargon can never silently leak through an unhandled arm.
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
            "The requested size of {size} bytes for {} is too small to hold a usable tomb.",
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
        // Not reachable from `create`/`unlock` today — only
        // `domain::workflows::revoke` (Story 2.2) produces this — but the
        // exhaustive match still requires a translation.
        DomainError::LastKeyslotGuard => {
            "That's the last key that can unlock this tomb — revoking it would lock you out \
             permanently, so this was refused."
                .to_string()
        }
        DomainError::AdapterFailure(inner) => translate_adapter_failure(inner),
    }
}

/// `AdapterFailure`'s message text varies by call site (it is the one
/// `DomainError` variant with no structured fields). Scope is bounded to
/// `create`'s and `unlock`'s own call graphs, so the reachable message shapes
/// are finite — each category below is matched by markers that appear
/// verbatim in the real call sites (`src/adapters/exec/mod.rs`,
/// `src/domain/mapping_name.rs`) and translated as a whole, rather than
/// chasing a bespoke rewrite of every exact string.
fn translate_adapter_failure(inner: &str) -> String {
    // FIDO2 enrollment failures (`enroll_fido2_key` and its token
    // export/import/parse helpers) — checked first since some of these
    // messages also contain "cryptsetup" (e.g. "cryptsetup token export
    // failed"), which would otherwise be misclassified as a bootstrap/open
    // subprocess failure below.
    const ENROLLMENT_MARKERS: [&str; 8] = [
        "systemd-cryptenroll",
        "token export",
        "token import",
        "token JSON",
        "systemd-fido2 token",
        "fido2-credential",
        "luksDump",
        "keyslot id",
    ];
    if ENROLLMENT_MARKERS
        .iter()
        .any(|marker| inner.contains(marker))
    {
        return "Enrolling your security key didn't complete. Make sure it's plugged in and \
                touch it when prompted, then try again."
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

    // Mount/filesystem failures (`mkfs`, `mount`, `chmod`, mount-point
    // create/remove).
    if inner.contains("mount") || inner.contains("mkfs") {
        return "Your tomb unlocked, but tomb-fido2 couldn't mount its filesystem.".to_string();
    }

    // `mapping_name::mapping_name`'s canonicalization failure.
    if let Some(path) = inner.strip_prefix("failed to canonicalize ") {
        let path = path.split(": ").next().unwrap_or(path);
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
