use std::path::Path;

/// Fixed prefix for every dm-crypt mapping name this tool creates, kept
/// independent of the product's own (placeholder) name (AD-13).
const MAPPING_NAME_PREFIX: &str = "vault";

/// FNV-1a: a plain, dependency-free, cross-toolchain-stable hash. Unlike
/// `std::collections::hash_map::DefaultHasher`, its output is not tied to a
/// particular Rust/std version, which matters here since the mapping name
/// must stay identical across upgrades (AD-12).
fn fnv1a_hash(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Single shared canonicalize+hash helper (AD-12), called by every workflow
/// that needs to derive the dm-crypt mapping name for a given device/file
/// path — `create` (naming it initially), `close`/`resize`/a later `unlock`
/// (reconstructing it) — so two independently-written call sites can never
/// compute divergent names for the same underlying path.
pub fn mapping_name(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let hash = fnv1a_hash(canonical.to_string_lossy().as_bytes());
    format!("{MAPPING_NAME_PREFIX}-{hash:016x}")
}
