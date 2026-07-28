use crate::domain::errors::DomainError;
use crate::domain::hooks::HookWarning;
use crate::domain::preflight;
use crate::domain::types::MapperHandle;
use crate::domain::workflows::close::close_mapping;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// One entry per mapping `LuksBackend::list_open_mappings` discovered,
/// paired with that mapping's own close outcome — never conflated with the
/// discovery call's own `Result` (AC #2's per-mapping fault tolerance is
/// distinct from a hard discovery failure).
pub type CloseAllResults = Vec<(MapperHandle, Result<(), DomainError>)>;

/// Closes every currently open/unlocked tomb in one batch (AD-17). `fido2` is
/// unused beyond `preflight::check` — same AD-4 uniform three-port gate
/// convention `close::run` already documents for its own unused `fido2`
/// parameter.
///
/// Discovers open mappings via `LuksBackend::list_open_mappings` (never a
/// stored registry) and applies `close::close_mapping`'s exact single-close
/// sequence to each — a discovery failure is this function's own `Err`,
/// distinct from a per-mapping close failure below. One mapping's failure
/// never stops the loop (AC #2): every discovered mapping's result, success
/// or failure, is collected into the returned `Vec`. An empty discovery
/// result simply yields `Ok(vec![])` (AC #3).
pub fn run(
    skip_hooks: bool,
    warn: &dyn Fn(HookWarning),
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<CloseAllResults, DomainError> {
    preflight::check(luks, fido2, fs)?;

    let mappings = luks.list_open_mappings()?;

    Ok(mappings
        .into_iter()
        .map(|mapper| {
            let result = close_mapping(&mapper, skip_hooks, warn, luks, fs);
            (mapper, result)
        })
        .collect())
}
