use std::path::PathBuf;

use hypogaol::domain::hooks::{
    exec_hook_rejection, parse_bind_hooks, resolve_bind_hook_entry, BindHookEntry,
    BindHookSkipReason, HookRejectionReason,
};
use hypogaol::domain::types::HookFileMeta;

use crate::fakes::FakeFilesystemBackend;

fn passing_meta() -> HookFileMeta {
    HookFileMeta {
        is_regular_file: true,
        is_symlink: false,
        is_executable: true,
        owned_by_invoking_user_or_root: true,
        is_world_writable: false,
    }
}

#[test]
fn parse_bind_hooks_reads_two_column_whitespace_separated_lines() {
    let content = ".gnupg .gnupg\nProjects/secret  Documents/secret\n";
    let entries = parse_bind_hooks(content);

    assert_eq!(
        entries,
        vec![
            BindHookEntry {
                source_relative: ".gnupg".to_string(),
                dest_relative: ".gnupg".to_string(),
            },
            BindHookEntry {
                source_relative: "Projects/secret".to_string(),
                dest_relative: "Documents/secret".to_string(),
            },
        ]
    );
}

#[test]
fn parse_bind_hooks_skips_blank_and_malformed_lines() {
    let content = "\n.gnupg .gnupg\n   \none-token-only\nsource dest extra-token\n.ssh .ssh\n";
    let entries = parse_bind_hooks(content);

    assert_eq!(
        entries,
        vec![
            BindHookEntry {
                source_relative: ".gnupg".to_string(),
                dest_relative: ".gnupg".to_string(),
            },
            BindHookEntry {
                source_relative: ".ssh".to_string(),
                dest_relative: ".ssh".to_string(),
            },
        ]
    );
}

#[test]
fn exec_hook_rejection_none_when_all_checks_pass() {
    assert_eq!(exec_hook_rejection(&passing_meta()), None);
}

#[test]
fn exec_hook_rejection_symlink_before_other_checks() {
    let meta = HookFileMeta {
        is_symlink: true,
        ..passing_meta()
    };
    assert_eq!(
        exec_hook_rejection(&meta),
        Some(HookRejectionReason::NotARegularFile)
    );
}

#[test]
fn exec_hook_rejection_not_a_regular_file() {
    let meta = HookFileMeta {
        is_regular_file: false,
        ..passing_meta()
    };
    assert_eq!(
        exec_hook_rejection(&meta),
        Some(HookRejectionReason::NotARegularFile)
    );
}

#[test]
fn exec_hook_rejection_not_executable() {
    let meta = HookFileMeta {
        is_executable: false,
        ..passing_meta()
    };
    assert_eq!(
        exec_hook_rejection(&meta),
        Some(HookRejectionReason::NotExecutable)
    );
}

#[test]
fn exec_hook_rejection_wrong_owner() {
    let meta = HookFileMeta {
        owned_by_invoking_user_or_root: false,
        ..passing_meta()
    };
    assert_eq!(
        exec_hook_rejection(&meta),
        Some(HookRejectionReason::WrongOwner)
    );
}

#[test]
fn exec_hook_rejection_world_writable() {
    let meta = HookFileMeta {
        is_world_writable: true,
        ..passing_meta()
    };
    assert_eq!(
        exec_hook_rejection(&meta),
        Some(HookRejectionReason::WorldWritable)
    );
}

// AC #3's checks fire in this exact enumeration order (not-a-regular-file,
// not-executable, wrong-owner, world-writable) — with every violation
// simultaneously true, only the first is ever reported.
#[test]
fn exec_hook_rejection_checks_fire_in_ac3_order_when_multiple_violations_are_true() {
    let meta = HookFileMeta {
        is_regular_file: false,
        is_symlink: true,
        is_executable: false,
        owned_by_invoking_user_or_root: false,
        is_world_writable: true,
    };
    assert_eq!(
        exec_hook_rejection(&meta),
        Some(HookRejectionReason::NotARegularFile)
    );
}

struct RealFixtureDir(PathBuf);

impl RealFixtureDir {
    fn create(unique_name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("tomb-fido2-unit-test-hooks-{unique_name}"));
        std::fs::create_dir_all(&path).expect("failed to create test fixture dir");
        Self(path)
    }

    fn subdir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).expect("failed to create test fixture subdir");
        path
    }
}

impl Drop for RealFixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn resolve_bind_hook_entry_rejects_dot_dot_escaping_volume_root() {
    let volume_root = RealFixtureDir::create("dot-dot-volume-root");
    let home = RealFixtureDir::create("dot-dot-home");
    home.subdir("dest");

    // ".." off the volume root resolves to its real parent (the shared temp
    // dir) — a real, existing path, but outside `volume_root`.
    let entry = BindHookEntry {
        source_relative: "..".to_string(),
        dest_relative: "dest".to_string(),
    };
    let fs = FakeFilesystemBackend::passing().with_path_exists(true);

    let result = resolve_bind_hook_entry(&entry, &volume_root.0, &home.0, &fs);

    assert_eq!(result, Err(BindHookSkipReason::SourceEscapesVolumeRoot));
}

#[test]
fn resolve_bind_hook_entry_rejects_absolute_path_escaping_home() {
    let volume_root = RealFixtureDir::create("abs-path-volume-root");
    volume_root.subdir("source");
    let home = RealFixtureDir::create("abs-path-home");
    let outside = RealFixtureDir::create("abs-path-outside");

    // An absolute `dest_relative` makes `Path::join` discard `home_dir`
    // entirely and resolve straight to the absolute path — the same
    // mechanism that rejects an absolute-path escape in practice.
    let entry = BindHookEntry {
        source_relative: "source".to_string(),
        dest_relative: outside.0.to_string_lossy().into_owned(),
    };
    let fs = FakeFilesystemBackend::passing().with_path_exists(true);

    let result = resolve_bind_hook_entry(&entry, &volume_root.0, &home.0, &fs);

    assert_eq!(result, Err(BindHookSkipReason::DestEscapesHome));
}

#[test]
fn resolve_bind_hook_entry_rejects_missing_source() {
    let volume_root = RealFixtureDir::create("missing-source-volume-root");
    let home = RealFixtureDir::create("missing-source-home");
    home.subdir("dest");

    let entry = BindHookEntry {
        source_relative: "does-not-exist".to_string(),
        dest_relative: "dest".to_string(),
    };
    // Source check (first) reports missing — dest is never reached.
    let fs = FakeFilesystemBackend::passing().with_path_exists_sequence(vec![false]);

    let result = resolve_bind_hook_entry(&entry, &volume_root.0, &home.0, &fs);

    assert_eq!(result, Err(BindHookSkipReason::SourceMissing));
}

#[test]
fn resolve_bind_hook_entry_rejects_missing_dest() {
    let volume_root = RealFixtureDir::create("missing-dest-volume-root");
    volume_root.subdir("source");
    let home = RealFixtureDir::create("missing-dest-home");

    let entry = BindHookEntry {
        source_relative: "source".to_string(),
        dest_relative: "does-not-exist".to_string(),
    };
    // Source check (first) reports present, dest check (second) reports
    // missing — proves source is checked before dest, per Task 1's method
    // doc ("checks `fs.path_exists` on both ... before canonicalizing").
    let fs = FakeFilesystemBackend::passing().with_path_exists_sequence(vec![true, false]);

    let result = resolve_bind_hook_entry(&entry, &volume_root.0, &home.0, &fs);

    assert_eq!(result, Err(BindHookSkipReason::DestMissing));
}
