//! The serializable device descriptors the UI consumes — engine truth zipped with the entry's labels —
//! and the connector-compatibility rule.

use serde::Serialize;

use engine::Domain;

/// A device type's full UI descriptor — everything the UI needs to list it in the catalog and draw
/// its panel. Built by [`descriptors`](super::descriptors) (numeric/domain fields from the node, labels from the entry).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDescriptor {
    /// Stable catalog type id — the `typeId` a scene's `DeviceInstance` names.
    pub type_id: String,
    /// Human display name for the catalog/panel.
    pub name: String,
    /// Physical form factor + size — the device's intrinsic dimensions, for spatial placement.
    pub form_factor: FormFactor,
    /// The device's smoothed control params, in exposed-id order.
    pub params: Vec<ParamDescriptor>,
    /// The device's ports (exposed inputs then outputs), each id scoped to its direction.
    pub ports: Vec<PortDescriptor>,
    /// The device's scalar readouts (meter values the host reads back), in exposed-id order. Empty
    /// for a device that measures nothing.
    pub readouts: Vec<ReadoutDescriptor>,
    /// The device's **structural** config toggles (e.g. a preamp's INST/hi-Z), which the UI renders
    /// and whose change triggers a rebuild. Empty for a device with no structural options.
    pub configs: Vec<ConfigDescriptor>,
}

/// One structural config option: its key, a UI label, the control kind, and the default value the
/// device builds with when unset. Hand-authored per catalog entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDescriptor {
    /// Structural config key — what a `ConfigSetting` addresses.
    pub key: String,
    /// Display label (e.g. "Inst 1").
    pub label: String,
    /// Suggested control widget for the structural toggle.
    pub kind: ConfigKind,
    /// Authored choices for a structural selector. Empty for a toggle.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<ConfigOption>,
    /// Value the device builds with when the instance leaves this key unset.
    pub default: f32,
}

/// Suggested control widget for a structural config option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigKind {
    /// A two-state structural switch (`0.0` / `1.0`) — e.g. a preamp's INST/hi-Z.
    Toggle,
    /// An enumerated structural selection; its authored numeric options ride on the descriptor.
    Select,
}

/// One labeled numeric option for a structural [`ConfigKind::Select`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOption {
    pub label: String,
    pub value: f32,
}

/// A device's physical form factor and size — intrinsic **content** (as fixed as its impedance),
/// authored per catalog entry and consumed by the UI's spatial world. It governs placement:
/// rackmount gear occupies contiguous U-slots in a rack; desktop gear places freely on a surface.
/// The UI derives the rendered footprint (and the 3-D box) from this; the engine never sees it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FormFactor {
    /// 19"-rack gear: occupies `rack_units` contiguous U-slots (1U ≈ 44.45 mm tall, 482.6 mm wide).
    #[serde(rename_all = "camelCase")]
    Rackmount { rack_units: u32 },
    /// Free-standing desktop/floor gear with an authored footprint box, in millimetres.
    #[serde(rename_all = "camelCase")]
    Desktop {
        width_mm: f32,
        height_mm: f32,
        depth_mm: f32,
    },
}

/// One control param: engine truth (`id`/`min`/`max`/`default`) + UI labels (`label`/`unit`/`kind`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamDescriptor {
    /// Device-level param id (its index in the exposed param list) — what a `ParamSetting` addresses.
    pub id: u32,
    /// Display label (e.g. "Level", "Attack").
    pub label: String,
    /// Unit string for the readout (e.g. "V", "ms", "" for unitless).
    pub unit: String,
    /// Suggested control widget.
    pub kind: ParamKind,
    /// How travel maps onto the value (linear vs dB-linear) — see [`ParamTaper`]. Omitted from the
    /// JS descriptor when `Linear` (the common case reads as `taper?: "log"`).
    #[serde(skip_serializing_if = "is_linear_taper")]
    pub taper: ParamTaper,
    /// Lower bound (from the node's `ParamDecl`).
    pub min: f32,
    /// Upper bound (from the node's `ParamDecl`).
    pub max: f32,
    /// Default / construction value (from the node's `ParamDecl`).
    pub default: f32,
}

/// One port: direction + carrier domain (engine truth) + a UI label and connector kind.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortDescriptor {
    /// Device-level port id **within its direction** — exposed inputs are 0..n_in, outputs 0..n_out,
    /// exactly as a scene's `PortRef` and the engine's `connect(from, out_port, to, in_port)` index them.
    pub id: u32,
    /// Display label (e.g. "In", "Out", "MIDI").
    pub label: String,
    /// Input or output.
    pub direction: PortDirection,
    /// Carrier domain (analog voltage / digital audio / events) — from the node's port face.
    pub domain: PortDomain,
    /// Lane count — the port's [`lane_count`](engine::InputPort::lane_count): digital **channels**
    /// (1 mono, N for a multichannel connector), analog conductors (1 unbalanced, 2 balanced), 1 for
    /// events. Engine truth (derived, can't drift). The UI badges a digital jack with `channels > 1`.
    pub channels: u16,
    /// Connector kind for the UI (mic/line/instrument/speaker/digital/MIDI jack styling).
    pub kind: PortKind,
    /// Physical connector shape — the hard constraint on what may plug in (see [`Connector`]).
    pub connector: Connector,
    /// **Round-trip latency** output: an edge *from* this output carries one block of latency (built
    /// via [`engine::Graph::connect_delayed`]). Its physical meaning is a device that trails its input
    /// by a buffer — a computer/DAW, whose playback is one block behind what it records — so a
    /// monitoring loop *through* it (interface → DAW → interface) can close without a same-block
    /// feedback cycle. Always `false` for inputs and for ordinary outputs.
    pub delayed: bool,
    /// The port on the **other** direction that shares this port's physical connector — a **duplex**
    /// jack (USB-C, Ethernet), which carries data both ways over one connector. For an output it is the
    /// paired input's id; for an input, the paired output's id. `None` for an ordinary one-way jack. A
    /// duplex [`scene::Connection`](crate::scene::Connection) between two duplex jacks expands to the
    /// two directed engine edges; the UI draws one jack and one cable. Omitted from the JS descriptor
    /// for an ordinary jack (so it reads as `duplexPartner?: number`, not `null`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplex_partner: Option<u32>,
}

/// One scalar readout a device exposes for the host to display (a meter value read back over the
/// node→host lane). Engine truth is the `id` (its position in the device's exposed readout list);
/// `label`/`unit` are hand-authored UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadoutDescriptor {
    /// Device-level readout id — its index in the exposed readout list, what the host reads back.
    pub id: u32,
    /// Display label (e.g. "VU", "Peak").
    pub label: String,
    /// Unit string for the reading (e.g. "VU", "dBu", "dBFS").
    pub unit: String,
}

/// Suggested control widget for a param.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ParamKind {
    Knob,
    Fader,
    Switch,
}

/// How a continuous control's **rotation/travel maps onto its value** — a UI/control-law hint, not
/// engine truth (the value sent to the engine is unchanged; `min`/`max` still come from the node's
/// `ParamDecl`). A voltage-gain multiplier spans a huge linear range (`0..1000` ≈ 0×→+60 dB), so a
/// **linear** knob crams the whole usable range into the first sliver of travel — quarter-turn is
/// already +48 dB. [`Log`](Self::Log) instead maps travel **dB-linearly** (equal rotation = equal dB
/// step), the way a real gain pot is marked, so the readout is shown in dB and the low end is usable.
/// The web widget owns the exact curve (silence at the very bottom for controls whose `min` is 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ParamTaper {
    /// Value is linear in travel: `value = min + t·(max − min)`. The default for every control.
    #[default]
    Linear,
    /// Value is logarithmic (dB-linear) in travel — for voltage-gain knobs displayed in dB.
    Log,
}

/// Serde skip predicate: a `Linear` taper is the default, omitted from the descriptor JSON.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if signature"
)]
fn is_linear_taper(taper: &ParamTaper) -> bool {
    matches!(taper, ParamTaper::Linear)
}

/// Whether a port is an input or an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortDirection {
    Input,
    Output,
}

/// A port's carrier domain — the UI mirror of the engine's `Domain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortDomain {
    Acoustic,
    Analog,
    Digital,
    Events,
}

/// Connector kind, for jack styling and connection-legality hints. This is the *signal class*
/// (mic/line/instrument/…) a jack presents, which drives colour/labelling — **not** the physical
/// connector shape. Whether two jacks can actually be joined is governed by [`Connector`], not this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortKind {
    Mic,
    Line,
    Instrument,
    Speaker,
    Digital,
    Midi,
    /// Acoustic sound-pressure port (a mic capsule, a sound source). Styled by the web layer;
    /// declared here so a mic's acoustic input appears on its descriptor face.
    Acoustic,
}

/// The **physical connector shape** a port (or cable end) presents — the hard, mechanical constraint on
/// what can plug into what, distinct from the signal-class [`PortKind`]. Two ports may only be joined
/// when their connectors are [compatible](connectors_compatible); a level/signal-class mismatch (a mic
/// into a line input) is *not* rejected here — it stays emergent from the voltage physics, per the
/// project's "don't flag what should emerge" rule. Only shape incompatibility (an XLR into a ¼" hole)
/// is a genuine impossibility and rejected.
///
/// Deliberately coarse and extensible — only the connectors today's catalog needs. `QuarterInch`
/// unifies TS and TRS: they share the same jack (a TS plug seats in a TRS socket, just unbalanced), so
/// they are one connector here. `Combo` is the XLR+TRS combo jack real interfaces use for their front
/// inputs — it physically accepts an XLR *or* a ¼" plug, so it's compatible with both. `Digital` stays
/// the generic digital connector for abstract lab converters; `Usb`/`Spdif` are the specific physical
/// digital connectors an interface presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Connector {
    /// ¼" (6.35 mm) phone jack — TS and TRS unified (same hole). Instrument + line gear.
    QuarterInch,
    /// 3-pin XLR — balanced mic / line.
    Xlr,
    /// XLR+TRS **combo** jack — accepts an XLR or a ¼" plug (an interface's front input).
    Combo,
    /// speakON / binding-post speaker connector.
    Speakon,
    /// 5-pin DIN — MIDI (events domain).
    Din5,
    /// A generic digital-audio connector (abstract lab converters/EQ/meters).
    Digital,
    /// USB — the interface↔computer multichannel digital link.
    Usb,
    /// S/PDIF (RCA coax) — a stereo digital in/out on an interface.
    Spdif,
    /// The acoustic "connector" — the ideal-air link between a sound source and a mic capsule. Not a
    /// physical plug; it mates only with itself (the acoustic cable, drawn by the web layer). It
    /// exists so acoustic ports declare a connector like every other port.
    Acoustic,
}

/// Whether two connectors can be physically joined. Same-connector is always fine; the one asymmetric
/// case is the **combo** jack, which physically accepts an XLR *or* a ¼" plug — so `Combo` is
/// compatible with [`Xlr`](Connector::Xlr) and [`QuarterInch`](Connector::QuarterInch) (and itself).
/// Everything else is equality (TS/TRS already unified under `QuarterInch`; other adapters/hybrid leads
/// are out of scope). Domain + digital channel-count compatibility are *separate* checks.
#[must_use]
pub fn connectors_compatible(a: Connector, b: Connector) -> bool {
    // A combo jack mates with an XLR or a ¼" plug, in either direction.
    let combo_mates = |x: Connector, y: Connector| {
        x == Connector::Combo && matches!(y, Connector::Xlr | Connector::QuarterInch)
    };
    a == b || combo_mates(a, b) || combo_mates(b, a)
}

impl From<Domain> for PortDomain {
    fn from(d: Domain) -> Self {
        match d {
            Domain::Acoustic => Self::Acoustic,
            Domain::Analog => Self::Analog,
            Domain::DigitalAudio => Self::Digital,
            Domain::Events => Self::Events,
        }
    }
}
