//! The runnable **patch** — the engine-facing projection of a UI scene.
//!
//! A *scene* is the whole studio the user builds: devices, their settings, how they're wired, **and**
//! UI-only data (where each device sits, which rack/space it belongs to). That full scene is owned and
//! persisted by the TypeScript UI as **versioned JSON** (`{ schemaVersion, ui, patch }`) — debuggable,
//! diffable, and migrated forward on load. The engine never sees the file, the version, or the UI data.
//!
//! What the engine *does* see is the [`Patch`]: the **runnable projection** — just the devices, their
//! param values, the connections, and the output tap. The UI produces it (after migrating an old save)
//! and posts it to the worklet, where `wasm-bindings` deserializes it (`serde-wasm-bindgen`, the
//! `JsValue` bridge that stays in the glue crate) into the structs below, which the scene builder
//! turns into a `Graph` and `compile`.
//!
//! **Ingress is deserialize-only and total.** Parsing a patch is the one fallible step on the way in;
//! it returns a `Result` so a malformed patch surfaces a legible error instead of panicking on the
//! audio thread. The structs also derive `Serialize` so tests can round-trip them through JSON
//! (`serde-wasm-bindgen` needs a JS realm, so the round-trip oracle runs over `serde_json` instead).
//!
//! Field names are **camelCase on the JS side** (`#[serde(rename_all = "camelCase")]`) so the TS UI
//! stays idiomatic while the Rust fields stay snake_case. The hand-written TS mirror lives in
//! `web/src/scene.ts`; keep the two in sync.

use serde::{Deserialize, Serialize};

/// A runnable patch: the devices, the connections between them, and the output tap to render.
///
/// This is the engine-facing projection of a scene — no placement, no spaces, no version. Build a
/// `Graph` from it and `compile`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    /// The device instances in the patch. Each is one catalog type at a chosen id.
    pub devices: Vec<DeviceInstance>,
    /// The connections wiring device ports together.
    pub connections: Vec<Connection>,
    /// The device port whose voltage is the engine's output tap.
    pub output: PortRef,
}

/// One placed device: a catalog **type** at a stable instance **id**, with any non-default param
/// values to apply. The id is UI-assigned and is what [`Connection`]s and the output [`PortRef`]
/// address; it maps to the one-or-many engine nodes the device expands into.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInstance {
    /// Stable instance id (UI-assigned), referenced by connections and the output tap.
    pub id: String,
    /// Catalog type id — selects the descriptor + builder.
    pub type_id: String,
    /// Param values to apply after build. Omitted ⇒ the device keeps its construction defaults.
    #[serde(default)]
    pub params: Vec<ParamSetting>,
    /// **Structural** config values, by key (e.g. a preamp's `"inst1"` hi-Z toggle). Unlike a
    /// `ParamSetting` (a smoothed runtime value), a config selects *how the device is built* — its
    /// node topology or a baked electrical value — so a change recompiles (the UI rebuilds the patch).
    /// Omitted ⇒ the device builds with its catalog defaults.
    #[serde(default)]
    pub config: Vec<ConfigSetting>,
}

/// A value for one of a device's **structural** config keys (matches `ConfigDescriptor.key`). A
/// scalar — a toggle is `0.0`/`1.0`. Applied at *build* (it selects which node/impedance is
/// constructed), not smoothed at runtime; changing one recompiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSetting {
    /// Structural config key (matches `ConfigDescriptor.key`).
    pub key: String,
    /// Value selecting the structural choice; a toggle is `0.0` (off) / `1.0` (on).
    pub value: f32,
}

/// A value for one of a device's smoothed control params, addressed by the exposed param id — its
/// position in the device's exposed param list (what `BuiltScene::param` indexes), *not* the
/// node-local `ParamId`. For a single-node device the two coincide; for a multi-node device they
/// differ (each stage's `ParamId`s restart at 0, but the exposed positions run 0..n across stages).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSetting {
    /// Exposed param id — its position in the device's exposed param list (matches `ParamDescriptor.id`).
    pub id: u32,
    /// Target value; clamped to the param's declared range when applied.
    pub value: f32,
}

/// A connection from one device port to another, optionally through a cable (series R + shunt C).
/// No cable ⇒ an ideal wire (no loss, no rolloff).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// The source device's output port.
    pub from: PortRef,
    /// The destination device's input port.
    pub to: PortRef,
    /// Optional cable on this edge; omitted ⇒ ideal wire.
    #[serde(default)]
    pub cable: Option<CableSpec>,
}

/// A reference to one **device-level** port: a device instance id plus the port index on that
/// device's exposed face. For a single-node device this is the node's own port; for a multi-node
/// device it maps to a port on one of its internal nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortRef {
    /// The target device instance id (matches a [`DeviceInstance::id`]).
    pub device: String,
    /// The device-level port index.
    pub port: u32,
}

/// A cable's electrical spec: series resistance and shunt capacitance, in SI units. Mapped to the
/// engine's `Cable` at build; the loading divider and treble rolloff emerge from it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CableSpec {
    /// Series resistance, ohms.
    pub resistance_ohms: f32,
    /// Shunt capacitance, farads.
    pub capacitance_farads: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small representative patch: two devices, one cabled connection, an output tap — enough to
    /// exercise every field (params, an optional cable, port refs).
    fn sample_patch() -> Patch {
        Patch {
            devices: vec![
                DeviceInstance {
                    id: "synth".into(),
                    type_id: "synth_voice".into(),
                    params: vec![ParamSetting { id: 0, value: 1.0 }],
                    // A config setting too, so the round-trip / camelCase oracles cover the field.
                    config: vec![ConfigSetting {
                        key: "inst1".into(),
                        value: 1.0,
                    }],
                },
                DeviceInstance {
                    id: "spk".into(),
                    type_id: "speaker".into(),
                    params: vec![],
                    config: vec![],
                },
            ],
            connections: vec![Connection {
                from: PortRef {
                    device: "synth".into(),
                    port: 0,
                },
                to: PortRef {
                    device: "spk".into(),
                    port: 0,
                },
                cable: Some(CableSpec {
                    resistance_ohms: 150.0,
                    capacitance_farads: 0.5,
                }),
            }],
            output: PortRef {
                device: "spk".into(),
                port: 0,
            },
        }
    }

    /// The patch IR round-trips through JSON unchanged — the property the save/load + ingress path
    /// relies on. (serde-wasm-bindgen can't run natively; JSON exercises the same derives.)
    #[test]
    fn patch_round_trips_through_json() {
        let patch = sample_patch();
        let json = serde_json::to_string(&patch).expect("serialize");
        let back: Patch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(patch, back);
    }

    /// JS-side camelCase is the wire contract: a `typeId` field deserializes, a snake_case `type_id`
    /// does not (it would leave the required field missing). Pins the `rename_all` so the TS mirror
    /// and the Rust struct can't silently drift.
    #[test]
    fn wire_format_is_camel_case() {
        let ok = r#"{"id":"a","typeId":"speaker","params":[]}"#;
        serde_json::from_str::<DeviceInstance>(ok).expect("camelCase deserializes");

        let snake = r#"{"id":"a","type_id":"speaker","params":[]}"#;
        assert!(
            serde_json::from_str::<DeviceInstance>(snake).is_err(),
            "snake_case keys must not satisfy the camelCase contract"
        );
    }

    /// A malformed patch (a required field missing) errors cleanly — never a panic. This is the
    /// behavior the worklet depends on to keep a bad load off the audio thread's crash path.
    #[test]
    fn malformed_patch_errors_cleanly() {
        let missing_output = r#"{"devices":[],"connections":[]}"#;
        assert!(serde_json::from_str::<Patch>(missing_output).is_err());
    }

    /// Optional fields default: a device may omit `params`, a connection may omit `cable`.
    #[test]
    fn optional_fields_default_when_omitted() {
        let device: DeviceInstance =
            serde_json::from_str(r#"{"id":"a","typeId":"speaker"}"#).expect("params defaults");
        assert!(device.params.is_empty());
        assert!(device.config.is_empty(), "config defaults to empty");

        let conn: Connection = serde_json::from_str(
            r#"{"from":{"device":"a","port":0},"to":{"device":"b","port":0}}"#,
        )
        .expect("cable defaults");
        assert!(conn.cable.is_none());
    }
}
