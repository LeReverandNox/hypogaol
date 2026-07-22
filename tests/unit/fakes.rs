use tomb_fido2::ports::fido2_backend::Fido2Backend;
use tomb_fido2::ports::filesystem_backend::FilesystemBackend;
use tomb_fido2::ports::luks_backend::LuksBackend;

fn missing(deps: &[&str]) -> Vec<String> {
    deps.iter().map(|dep| dep.to_string()).collect()
}

pub struct FakeLuksBackend(Result<(), Vec<String>>);

impl FakeLuksBackend {
    pub fn passing() -> Self {
        Self(Ok(()))
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self(Err(missing(missing_deps)))
    }
}

impl LuksBackend for FakeLuksBackend {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        self.0.clone()
    }
}

pub struct FakeFido2Backend(Result<(), Vec<String>>);

impl FakeFido2Backend {
    pub fn passing() -> Self {
        Self(Ok(()))
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self(Err(missing(missing_deps)))
    }
}

impl Fido2Backend for FakeFido2Backend {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        self.0.clone()
    }
}

pub struct FakeFilesystemBackend(Result<(), Vec<String>>);

impl FakeFilesystemBackend {
    pub fn passing() -> Self {
        Self(Ok(()))
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self(Err(missing(missing_deps)))
    }
}

impl FilesystemBackend for FakeFilesystemBackend {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        self.0.clone()
    }
}
