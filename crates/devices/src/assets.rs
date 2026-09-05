//! Cold-path assets supplied explicitly by the host when building a scene.
//!
//! The library owns already-parsed immutable banks. It deliberately has no byte-loading API: browser
//! delivery and parsing belong to the host/WASM seam, while device construction only borrows a bank.

use std::collections::BTreeMap;
use std::fmt;

use engine::{SoundFontBank, SoundFontError};

/// Stable-id collection of immutable parsed SoundFont banks available to device builders.
#[derive(Default)]
pub struct SoundFontBankLibrary {
    banks: BTreeMap<String, SoundFontBank>,
}

impl SoundFontBankLibrary {
    /// Add or replace the bank stored under `id`. Replacement is cold-path host work; schedules that
    /// already borrowed an earlier bank retain their own `Arc` handle.
    pub fn insert(&mut self, id: impl Into<String>, bank: SoundFontBank) -> Option<SoundFontBank> {
        self.banks.insert(id.into(), bank)
    }

    /// Borrow the parsed bank registered under `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SoundFontBank> {
        self.banks.get(id)
    }
}

/// Explicit construction context passed from patch assembly to asset-backed device builders.
pub struct BuildContext<'a> {
    soundfonts: &'a SoundFontBankLibrary,
}

impl<'a> BuildContext<'a> {
    /// Construct a context over the host's parsed SoundFont library.
    #[must_use]
    pub fn new(soundfonts: &'a SoundFontBankLibrary) -> Self {
        Self { soundfonts }
    }

    /// Resolve a SoundFont bank by its stable asset id.
    #[must_use]
    pub fn soundfont(&self, id: &str) -> Option<&SoundFontBank> {
        self.soundfonts.get(id)
    }
}

/// Cold-path failure specific to an asset-backed device builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetBuildError {
    /// The patch/device requested a bank the host did not supply.
    MissingSoundFont { asset_id: String },
    /// A fresh synth could not be constructed over an otherwise available parsed bank.
    SoundFont(SoundFontError),
}

impl fmt::Display for AssetBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSoundFont { asset_id } => {
                write!(f, "missing SoundFont asset {asset_id:?}")
            }
            Self::SoundFont(error) => write!(f, "could not build SoundFont source: {error}"),
        }
    }
}

impl std::error::Error for AssetBuildError {}

impl From<SoundFontError> for AssetBuildError {
    fn from(error: SoundFontError) -> Self {
        Self::SoundFont(error)
    }
}
