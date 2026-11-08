//! Shared pieces of the two SoundFont guitars: GM presets, the source/DA builders and their constants.

use super::spec::{ConfigOptionUi, DeviceConfig, NodeFace};
use crate::assets::{AssetBuildError, BuildContext};
use engine::{
    BitDepth, DaConverter, InputZ, Node, Ohms, PoweredMonitor, SampleRate, SoundFontSource, Volts,
};

pub(super) const TIM_GM_6MB: &str = "tim_gm_6mb";
pub(super) const GUITAR_PRESET_KEY: &str = "preset";
pub(super) const ELECTRIC_GUITAR_PRESET: f32 = 27.0;
pub(super) const ACOUSTIC_GUITAR_PRESET: f32 = 25.0;
pub(super) const GUITAR_DA_RATE_HZ: f64 = 48_000.0;
pub(super) const GUITAR_DA_BITS: u32 = 24;
pub(super) const ELECTRIC_GUITAR_REFERENCE_V: f32 = 1.0;
pub(super) const ELECTRIC_GUITAR_Z_OUT_OHMS: f32 = 100_000.0;
pub(super) const ACOUSTIC_GUITAR_REFERENCE_V: f32 = 1.0;
pub(super) const ACOUSTIC_GUITAR_MONITOR_SENSITIVITY_PA_PER_V: f32 = 1.0;

/// GM's two acoustic guitar voices. An acoustic guitar radiates through its body and must be mic'd,
/// so its selector deliberately cannot choose electric pickup tones.
pub(super) const ACOUSTIC_GUITAR_PRESET_OPTIONS: &[ConfigOptionUi] = &[
    ConfigOptionUi {
        label: "Nylon Guitar",
        value: 24.0,
    },
    ConfigOptionUi {
        label: "Steel Guitar",
        value: 25.0,
    },
];

/// GM's electric guitar voices. These leave the electric guitar's high-impedance instrument output.
pub(super) const ELECTRIC_GUITAR_PRESET_OPTIONS: &[ConfigOptionUi] = &[
    ConfigOptionUi {
        label: "Jazz Guitar",
        value: 26.0,
    },
    ConfigOptionUi {
        label: "Clean Guitar",
        value: 27.0,
    },
    ConfigOptionUi {
        label: "Muted Guitar",
        value: 28.0,
    },
    ConfigOptionUi {
        label: "Overdrive Guitar",
        value: 29.0,
    },
    ConfigOptionUi {
        label: "Distortion Guitar",
        value: 30.0,
    },
    ConfigOptionUi {
        label: "Guitar Harmonics",
        value: 31.0,
    },
];

/// Normalize scalar structural config to a GM program number. Finite values round to the nearest
/// integer then clamp to `0..=127`; non-finite values use the normalized default.
#[must_use]
pub(crate) fn gm_preset(config: &DeviceConfig, key: &str, default: f32) -> u8 {
    let normalize = |value: f32| {
        if value.is_finite() {
            value.round().clamp(0.0, 127.0) as u8
        } else {
            default.round().clamp(0.0, 127.0) as u8
        }
    };
    normalize(config.get_or(key, default))
}

pub(super) fn guitar_source_face(_config: &DeviceConfig) -> NodeFace {
    NodeFace {
        inputs: SoundFontSource::input_face().to_vec(),
        outputs: SoundFontSource::output_face().to_vec(),
        params: Vec::new(),
        readouts: Vec::new(),
    }
}

pub(super) fn guitar_source(
    config: &DeviceConfig,
    context: &BuildContext,
    default_preset: f32,
) -> Result<Box<dyn Node>, AssetBuildError> {
    let bank = context
        .soundfont(TIM_GM_6MB)
        .ok_or_else(|| AssetBuildError::MissingSoundFont {
            asset_id: TIM_GM_6MB.into(),
        })?;
    Ok(Box::new(SoundFontSource::from_bank(
        bank,
        gm_preset(config, GUITAR_PRESET_KEY, default_preset),
    )?))
}

pub(super) fn electric_guitar_source(
    config: &DeviceConfig,
    context: &BuildContext,
) -> Result<Box<dyn Node>, AssetBuildError> {
    guitar_source(config, context, ELECTRIC_GUITAR_PRESET)
}

pub(super) fn acoustic_guitar_source(
    config: &DeviceConfig,
    context: &BuildContext,
) -> Result<Box<dyn Node>, AssetBuildError> {
    guitar_source(config, context, ACOUSTIC_GUITAR_PRESET)
}

pub(super) fn electric_guitar_da(_config: &DeviceConfig) -> Box<dyn Node> {
    Box::new(DaConverter::new(
        SampleRate::new(GUITAR_DA_RATE_HZ),
        BitDepth::new(GUITAR_DA_BITS),
        Volts::new(ELECTRIC_GUITAR_REFERENCE_V),
        Ohms::new(ELECTRIC_GUITAR_Z_OUT_OHMS),
    ))
}

pub(super) fn acoustic_guitar_da(_config: &DeviceConfig) -> Box<dyn Node> {
    Box::new(DaConverter::new(
        SampleRate::new(GUITAR_DA_RATE_HZ),
        BitDepth::new(GUITAR_DA_BITS),
        Volts::new(ACOUSTIC_GUITAR_REFERENCE_V),
        Ohms::new(150.0),
    ))
}

pub(super) fn acoustic_guitar_monitor(_config: &DeviceConfig) -> Box<dyn Node> {
    Box::new(PoweredMonitor::new(
        ACOUSTIC_GUITAR_MONITOR_SENSITIVITY_PA_PER_V,
        InputZ::new(Ohms::new(10_000.0)),
    ))
}
