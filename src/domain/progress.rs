/// Every variant is bare and payload-free by design (AD-19, architecture
/// adversarial review Finding 5): the enum discriminant alone is the entire
/// message. A payload-carrying variant at `FormattingLuks2`/`EnrollingFido2Key`
/// could smuggle the still-live transient bootstrap passphrase (AD-3) past its
/// wipe point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateStage {
    AllocatingBackingFile,
    FormattingLuks2,
    EnrollingFido2Key,
    CreatingFilesystem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeStage {
    GrowingBackingFile,
    ResizingLuks2Mapping,
    GrowingFilesystem,
}
