//! Assemble a runnable [`Patch`] into a compiled engine [`Schedule`].
//!
//! This is where the per-device chassis seam ([`instantiate`]) meets the whole scene. [`build_patch`]:
//! 1. **instantiates** each device into a fresh `Graph` (1..N nodes + internal edges), keying its
//!    [`BuiltDevice`] map by the scene device id;
//! 2. **remaps** each scene [`Connection`](crate::Connection) — addressed by `(device, device-port)` — through those maps
//!    to concrete node-port edges, and likewise the output tap;
//! 3. **compiles** (fixed seed → reproducible); and
//! 4. **resolves** the generic control surface: `(device, param id) → ParamHandle` and
//!    `device → EventInputId`, so the host can drive params/notes by device id.
//!
//! Everything fallible lives here, off the audio thread: an unknown type, a dangling device reference,
//! a port out of range, or an engine [`CompileError`] (domain mismatch, cycle, …) becomes a
//! [`BuildError`] — never a panic. The caller (the worklet) turns that into a legible message.
//!
//! Scene **param *values*** (`ParamSetting`s) are not applied here — `build_patch` resolves the
//! *handles*; the caller pushes the saved values onto its param queue, so they de-zipper
//! in like any control change.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt;

use engine::{
    AnalogRate, Cable, CompileError, EventInputId, Farads, Graph, NodeId, Ohms, ParamHandle,
    ReadoutHandle, Schedule, compile,
};

use crate::catalog::{
    BuiltDevice, Connector, DeviceConfig, DeviceDescriptor, PortDescriptor, PortDirection,
    PortDomain, connectors_compatible, descriptors, instantiate,
};
use crate::scene::{Patch, PortRef};

/// Why assembling a [`Patch`] failed — all caught off the audio thread (the hot path never sees a
/// patch). Each variant names the offending scene element so the UI can point at it.
#[derive(Debug)]
pub enum BuildError {
    /// Two devices share an id; ids must be unique within a patch.
    DuplicateDevice { id: String },
    /// A device names a `type_id` that isn't in the catalog.
    UnknownType { id: String, type_id: String },
    /// A connection or the output tap references a device id that isn't in the patch.
    UnknownDevice { id: String },
    /// A connection's source / the output tap names an output port beyond the device's exposed outputs.
    OutputPortOutOfRange { device: String, port: u32 },
    /// A connection's destination names an input port beyond the device's exposed inputs.
    InputPortOutOfRange { device: String, port: u32 },
    /// Two **same-domain** ports present incompatible physical connectors (e.g. an XLR jack into a ¼"
    /// jack) — they can't be joined. A hard mechanical constraint, checked before the engine's domain
    /// check (a cross-domain wire is a [`CompileError::DomainMismatch`] instead, mirroring the UI's
    /// domain-then-connector precedence).
    ConnectorMismatch {
        from: String,
        to: String,
        from_connector: Connector,
        to_connector: Connector,
    },
    /// Two digital ports carry different **channel counts** (e.g. an 8-wide USB send into a 2-wide
    /// return) — no wire bridges the mismatch. The devices-layer mirror of the engine's
    /// [`CompileError::LaneCountMismatch`], surfaced here for a legible, pre-compile error.
    ChannelCountMismatch {
        from: String,
        to: String,
        from_channels: u16,
        to_channels: u16,
    },
    /// The engine rejected the assembled graph (domain mismatch, cycle, indivisible rate, …).
    Compile(CompileError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDevice { id } => write!(f, "duplicate device id {id:?}"),
            Self::UnknownType { id, type_id } => {
                write!(f, "device {id:?} has unknown type {type_id:?}")
            }
            Self::UnknownDevice { id } => write!(f, "connection references unknown device {id:?}"),
            Self::OutputPortOutOfRange { device, port } => {
                write!(f, "device {device:?} has no output port {port}")
            }
            Self::InputPortOutOfRange { device, port } => {
                write!(f, "device {device:?} has no input port {port}")
            }
            Self::ConnectorMismatch {
                from,
                to,
                from_connector,
                to_connector,
            } => write!(
                f,
                "incompatible connectors: {from:?} ({from_connector:?}) -> {to:?} ({to_connector:?})"
            ),
            Self::ChannelCountMismatch {
                from,
                to,
                from_channels,
                to_channels,
            } => write!(
                f,
                "digital channel-count mismatch: {from:?} ({from_channels} ch) -> {to:?} ({to_channels} ch)"
            ),
            Self::Compile(e) => write!(f, "compile error: {e:?}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<CompileError> for BuildError {
    fn from(e: CompileError) -> Self {
        Self::Compile(e)
    }
}

/// A compiled scene: the runnable [`Schedule`] plus the resolved control surface keyed by scene device
/// id. The host renders through [`schedule_mut`](Self::schedule_mut) and drives control via
/// [`param`](Self::param) / [`event_input`](Self::event_input). Built by [`build_patch`].
pub struct BuiltScene {
    schedule: Schedule,
    /// device id → its `ParamHandle`s, indexed by **device-level param id**. Each param id maps to a
    /// *slice* of handles: one for an ungrouped param, N for a device-level param group (a value set
    /// on that id fans out to every handle).
    params: BTreeMap<String, Vec<Vec<ParamHandle>>>,
    /// device id → its (first) event input, for note routing. Absent for devices with no event input.
    events: BTreeMap<String, EventInputId>,
    /// device id → its `ReadoutHandle`s, indexed by **device-level readout id**. Absent for devices
    /// that expose no readouts (measure nothing) — the node→host mirror of `params`.
    readouts: BTreeMap<String, Vec<ReadoutHandle>>,
    /// Per scene [`Connection`](crate::Connection) (same index as `patch.connections`): the edge's
    /// **loading loss** in dB (`20·log10` of the baked resistive divider gain, so ≤ 0 dB — an
    /// attenuation), or `None` for a digital/event connection (ideal, no resistive loading). The
    /// static analog-domain readout the UI shows per cable.
    connection_losses: Vec<Option<f32>>,
}

impl fmt::Debug for BuiltScene {
    // `Schedule` isn't `Debug` (and printing it would be noise); show the resolved control surface.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuiltScene")
            .field("params", &self.params.keys().collect::<Vec<_>>())
            .field("events", &self.events.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl BuiltScene {
    /// The compiled schedule, mutable — the host renders one block at a time through it.
    pub fn schedule_mut(&mut self) -> &mut Schedule {
        &mut self.schedule
    }

    /// The schedule, by shared reference (e.g. to read group delay / block length).
    #[must_use]
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// Resolve `(device, device-level param id)` to the [`ParamHandle`]s it drives for smoothed
    /// control — one for an ungrouped param, several for a device-level param group (a device power
    /// switch). Empty if the device or param id is unknown. The host sets one value on **all** of them.
    #[must_use]
    pub fn param(&self, device: &str, param_id: u32) -> &[ParamHandle] {
        match self
            .params
            .get(device)
            .and_then(|handles| handles.get(param_id as usize))
        {
            Some(handles) => handles,
            None => &[],
        }
    }

    /// Resolve `device` to its event input (for note-on/off), or `None` if it has none.
    #[must_use]
    pub fn event_input(&self, device: &str) -> Option<EventInputId> {
        self.events.get(device).copied()
    }

    /// Resolve `(device, device-level readout id)` to its [`ReadoutHandle`], or `None` if the device
    /// or readout id is unknown. The node→host mirror of [`param`](Self::param).
    #[must_use]
    pub fn readout(&self, device: &str, readout_id: u32) -> Option<ReadoutHandle> {
        self.readouts
            .get(device)
            .and_then(|handles| handles.get(readout_id as usize))
            .copied()
    }

    /// A snapshot of every metering device's current readings: `(device id, values in readout-id
    /// order)` from the most recently processed block. Only devices that expose readouts appear.
    /// Cheap (a handful of scalars) — the host polls it each meter frame to drive its screens.
    #[must_use]
    pub fn readout_snapshot(&self) -> Vec<(String, Vec<f32>)> {
        self.readouts
            .iter()
            .map(|(device, handles)| {
                let values = handles
                    .iter()
                    .map(|&h| self.schedule.readout_value(h).unwrap_or(0.0))
                    .collect();
                (device.clone(), values)
            })
            .collect()
    }

    /// Every scene connection's loading loss in dB, by connection index (`None` for digital/event
    /// connections). The static analog-domain readout the UI surfaces per cable and in the levels panel.
    #[must_use]
    pub fn connection_losses(&self) -> &[Option<f32>] {
        &self.connection_losses
    }

    /// The **loading loss** of scene connection `index` (its position in the patch's connection
    /// list), in dB — `20·log10` of the baked resistive divider gain, so ≤ 0 dB (an attenuation).
    /// `None` if the index is out of range or the connection is digital/event (ideal, no resistive
    /// loading). This is the frequency-independent divider loss; a cable's treble rolloff is a
    /// separate effect not folded in.
    #[must_use]
    pub fn connection_loading_loss(&self, index: usize) -> Option<f32> {
        self.connection_losses.get(index).copied().flatten()
    }
}

/// A linear voltage gain as decibels: `20·log10(gain)`. For a loading divider (gain ≤ 1) this is
/// ≤ 0 — the loss. Done in `f64` for precision.
fn gain_to_db(gain: f32) -> f32 {
    (20.0 * f64::from(gain).log10()) as f32
}

/// Resolve a device output `PortRef` to a concrete `(node, output port)`.
fn resolve_output(
    devices: &BTreeMap<String, BuiltDevice>,
    r: &PortRef,
) -> Result<(NodeId, usize), BuildError> {
    let built = devices
        .get(&r.device)
        .ok_or_else(|| BuildError::UnknownDevice {
            id: r.device.clone(),
        })?;
    built
        .outputs
        .get(r.port as usize)
        .copied()
        .ok_or_else(|| BuildError::OutputPortOutOfRange {
            device: r.device.clone(),
            port: r.port,
        })
}

/// The [`PortDescriptor`] for a scene device's `direction` port `port`, or `None` if the device/port
/// isn't found — used to read a port's physical connector (and domain) for connector-compatibility.
/// `descs` is keyed by type-id, `types` maps scene device id → type-id.
fn port_descriptor<'a>(
    descs: &'a BTreeMap<String, DeviceDescriptor>,
    types: &BTreeMap<&str, &str>,
    device: &str,
    direction: PortDirection,
    port: u32,
) -> Option<&'a PortDescriptor> {
    let type_id = types.get(device)?;
    descs
        .get(*type_id)?
        .ports
        .iter()
        .find(|p| p.direction == direction && p.id == port)
}

/// Resolve a device input `PortRef` to a concrete `(node, input port)`.
fn resolve_input(
    devices: &BTreeMap<String, BuiltDevice>,
    r: &PortRef,
) -> Result<(NodeId, usize), BuildError> {
    let built = devices
        .get(&r.device)
        .ok_or_else(|| BuildError::UnknownDevice {
            id: r.device.clone(),
        })?;
    built
        .inputs
        .get(r.port as usize)
        .copied()
        .ok_or_else(|| BuildError::InputPortOutOfRange {
            device: r.device.clone(),
            port: r.port,
        })
}

/// Assemble a runnable [`Patch`] into a compiled [`BuiltScene`] at the given block length, analog
/// `rate`, and `seed`. See the module docs for the four steps; all failures surface as [`BuildError`].
///
/// # Errors
/// Returns a [`BuildError`] for an unknown device type, a dangling device reference, a port out of
/// range, or an engine [`CompileError`].
pub fn build_patch(
    patch: &Patch,
    block_len: usize,
    rate: AnalogRate,
    seed: u64,
) -> Result<BuiltScene, BuildError> {
    let mut graph = Graph::new();

    // 1. Instantiate every device, keyed by its (unique) scene id.
    let mut devices: BTreeMap<String, BuiltDevice> = BTreeMap::new();
    for device in &patch.devices {
        let config = DeviceConfig::new(&device.config);
        let built = instantiate(&device.type_id, &config, &mut graph).ok_or_else(|| {
            BuildError::UnknownType {
                id: device.id.clone(),
                type_id: device.type_id.clone(),
            }
        })?;
        match devices.entry(device.id.clone()) {
            Entry::Occupied(_) => {
                return Err(BuildError::DuplicateDevice {
                    id: device.id.clone(),
                });
            }
            Entry::Vacant(slot) => slot.insert(built),
        };
    }

    // Connector taxonomy (per device type) + scene-id → type-id, for the connector-compatibility gate
    // below. Cold path (a user gesture), so rebuilding the catalog descriptors here is fine.
    let descs: BTreeMap<String, DeviceDescriptor> = descriptors()
        .into_iter()
        .map(|d| (d.type_id.clone(), d))
        .collect();
    let types: BTreeMap<&str, &str> = patch
        .devices
        .iter()
        .map(|d| (d.id.as_str(), d.type_id.as_str()))
        .collect();

    // 2. Remap each scene connection (and the output tap) through the device maps to graph edges,
    //    recording each connection's graph edge index (edges are appended in call order, after the
    //    devices' internal edges) so its baked loading loss can be read back after compile.
    let mut connection_edges: Vec<usize> = Vec::with_capacity(patch.connections.len());
    for conn in &patch.connections {
        let (from_node, from_port) = resolve_output(&devices, &conn.from)?;
        let (to_node, to_port) = resolve_input(&devices, &conn.to)?;

        // Same-domain physical-fit checks (a cross-domain wire is the engine's `DomainMismatch` at
        // compile below, mirroring the UI's domain-then-connector precedence). Both ports resolved
        // above, so the descriptors are present.
        if let (Some(fp), Some(tp)) = (
            port_descriptor(
                &descs,
                &types,
                &conn.from.device,
                PortDirection::Output,
                conn.from.port,
            ),
            port_descriptor(
                &descs,
                &types,
                &conn.to.device,
                PortDirection::Input,
                conn.to.port,
            ),
        ) && fp.domain == tp.domain
        {
            // Connector-shape compatibility — a hard mechanical constraint (an XLR won't seat in a ¼"
            // jack).
            if !connectors_compatible(fp.connector, tp.connector) {
                return Err(BuildError::ConnectorMismatch {
                    from: conn.from.device.clone(),
                    to: conn.to.device.clone(),
                    from_connector: fp.connector,
                    to_connector: tp.connector,
                });
            }
            // A digital link must carry the same number of channels on both ends (an 8-wide send can't
            // feed a 2-wide return). The engine enforces this too (`LaneCountMismatch`); surfacing it
            // here gives a legible, pre-compile error that the web mirror can also show live.
            if fp.domain == PortDomain::Digital && fp.channels != tp.channels {
                return Err(BuildError::ChannelCountMismatch {
                    from: conn.from.device.clone(),
                    to: conn.to.device.clone(),
                    from_channels: fp.channels,
                    to_channels: tp.channels,
                });
            }
        }

        connection_edges.push(graph.connection_count());
        match &conn.cable {
            Some(cable) => graph.connect_cabled(
                from_node,
                from_port,
                to_node,
                to_port,
                Cable::new(
                    Ohms::new(cable.resistance_ohms),
                    Farads::new(cable.capacitance_farads),
                ),
            ),
            None => graph.connect_ideal(from_node, from_port, to_node, to_port),
        }
    }
    let (out_node, out_port) = resolve_output(&devices, &patch.output)?;
    graph.set_output(out_node, out_port);

    // 3. Compile (fixed seed → reproducible). Engine validation (domain, cycles, rates) lands here.
    let schedule = compile(graph, block_len, rate, seed)?;

    // 3b. Read back each connection's baked loading loss (analog only; digital/event → None), by the
    //     graph edge index recorded above — the static analog-domain readout the UI surfaces.
    let connection_losses: Vec<Option<f32>> = connection_edges
        .iter()
        .map(|&ei| schedule.edge_gain(ei).map(gain_to_db))
        .collect();

    // 4. Resolve the generic control surface against the compiled schedule.
    let mut params = BTreeMap::new();
    let mut events = BTreeMap::new();
    let mut readouts = BTreeMap::new();
    for device in &patch.devices {
        let built = &devices[&device.id];
        // Each exposed param resolves to one or more `ParamHandle`s (one per group target); the host
        // fans a single value out to all of them.
        let handles: Vec<Vec<ParamHandle>> = built
            .params
            .iter()
            .map(|targets| {
                targets
                    .iter()
                    .map(|&(node, id)| {
                        schedule
                            .param(node, id)
                            .expect("a freshly built device's param resolves in its own schedule")
                    })
                    .collect()
            })
            .collect();
        params.insert(device.id.clone(), handles);

        // A device's event input is its first exposed input that the schedule recognizes as one.
        if let Some(ev) = built
            .inputs
            .iter()
            .find_map(|&(node, port)| schedule.event_input(node, port))
        {
            events.insert(device.id.clone(), ev);
        }

        // Readout handles (node→host lane), in device readout-id order. Only metering devices have any.
        if !built.readouts.is_empty() {
            let handles: Vec<ReadoutHandle> = built
                .readouts
                .iter()
                .map(|&(node, id)| {
                    schedule
                        .readout(node, id)
                        .expect("a freshly built device's readout resolves in its own schedule")
                })
                .collect();
            readouts.insert(device.id.clone(), handles);
        }
    }

    Ok(BuiltScene {
        schedule,
        params,
        events,
        readouts,
        connection_losses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Connection, DeviceInstance};
    use engine::{
        AdConverter, BitDepth, DaConverter, EventMessage, EventQueue, Graph, InputZ, Ohms,
        ParamQueue, SampleRate, Speaker, SynthVoice, VoltageBuffer, Volts, compile,
    };

    const BLOCK_LEN: usize = 384; // 384 / M=8 = 48 host samples; divisible, as compile requires.
    const NOTE: u8 = 69; // A4

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn conn(from: &str, from_port: u32, to: &str, to_port: u32) -> Connection {
        Connection {
            from: PortRef {
                device: from.into(),
                port: from_port,
            },
            to: PortRef {
                device: to.into(),
                port: to_port,
            },
            cable: None,
        }
    }

    fn device(id: &str, type_id: &str) -> DeviceInstance {
        DeviceInstance {
            id: id.into(),
            type_id: type_id.into(),
            params: vec![],
            config: vec![],
        }
    }

    /// The pinned canonical patch as a scene: `synth → AD → DA → speaker`, tapped at the speaker.
    fn canonical_patch() -> Patch {
        Patch {
            devices: vec![
                device("synth", "synth_voice"),
                device("ad", "ad_converter"),
                device("da", "da_converter"),
                device("spk", "speaker"),
            ],
            connections: vec![
                conn("synth", 0, "ad", 0),
                conn("ad", 0, "da", 0),
                conn("da", 0, "spk", 0),
            ],
            output: PortRef {
                device: "spk".into(),
                port: 0,
            },
        }
    }

    /// Peak `|speaker volts|` over `blocks` blocks after striking a note on `device`.
    fn peak_after_note(scene: &mut BuiltScene, device: &str, blocks: usize) -> f32 {
        let ev = scene
            .event_input(device)
            .expect("device has an event input");
        let mut events = EventQueue::with_capacity(4);
        events.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: NOTE,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
        let mut peak = 0.0_f32;
        for _ in 0..blocks {
            scene
                .schedule_mut()
                .process_with_events(&mut out, &mut events);
            peak = out.as_slice().iter().fold(peak, |p, &v| p.max(v.abs()));
        }
        peak
    }

    /// The canonical patch, assembled from a scene, is **byte-for-byte identical** to the same chain
    /// built by hand directly on the engine — output parity. Same device order ⇒ same node indices ⇒
    /// same per-node seeding ⇒ identical dither, so equality (not just "close") holds. This is the
    /// oracle that `build_patch` wires, taps, and orders the graph exactly like a hand-built engine.
    #[test]
    fn canonical_patch_matches_a_hand_built_graph() {
        // Assembled from the scene.
        let patch = canonical_patch();
        let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("canonical patch builds");
        let scene_ev = scene.event_input("synth").expect("synth event input");

        // The same chain, built directly — identical configs and order to the catalog's builders.
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        let ad = g.add(AdConverter::new(
            SampleRate::new(48_000.0),
            BitDepth::new(16),
            Volts::new(1.0),
            Ohms::new(1_000_000.0),
        ));
        let da = g.add(DaConverter::new(
            SampleRate::new(48_000.0),
            BitDepth::new(16),
            Volts::new(1.0),
            Ohms::new(150.0),
        ));
        let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
        g.connect_ideal(voice, 0, ad, 0);
        g.connect_ideal(ad, 0, da, 0);
        g.connect_ideal(da, 0, spk, 0);
        g.set_output(spk, 0);
        let mut hand = compile(g, BLOCK_LEN, rate(), 0).expect("hand graph compiles");
        let hand_ev = hand.event_input(voice, 0).expect("voice event input");

        // Strike the same note on both and compare every sample over several blocks.
        let mut scene_events = EventQueue::with_capacity(4);
        scene_events.push(
            0,
            scene_ev,
            EventMessage::NoteOn {
                note: NOTE,
                velocity: 100,
            },
        );
        let mut hand_events = EventQueue::with_capacity(4);
        hand_events.push(
            0,
            hand_ev,
            EventMessage::NoteOn {
                note: NOTE,
                velocity: 100,
            },
        );

        let mut a = VoltageBuffer::zeros(BLOCK_LEN, rate());
        let mut b = VoltageBuffer::zeros(BLOCK_LEN, rate());
        for _ in 0..16 {
            scene
                .schedule_mut()
                .process_with_events(&mut a, &mut scene_events);
            hand.process_with_events(&mut b, &mut hand_events);
            assert_eq!(
                a.as_slice(),
                b.as_slice(),
                "scene-built output must match the hand-built graph"
            );
        }
    }

    /// End-to-end: the scene-built canonical patch is silent until a note, then audible — wiring,
    /// output tap, compile, and event-input resolution all work.
    #[test]
    fn scene_built_patch_is_silent_then_audible() {
        let mut scene = build_patch(&canonical_patch(), BLOCK_LEN, rate(), 0).expect("builds");

        let mut idle = VoltageBuffer::zeros(BLOCK_LEN, rate());
        let mut none = EventQueue::with_capacity(1);
        let mut idle_peak = 0.0_f32;
        for _ in 0..8 {
            scene
                .schedule_mut()
                .process_with_events(&mut idle, &mut none);
            idle_peak = idle
                .as_slice()
                .iter()
                .fold(idle_peak, |p, &v| p.max(v.abs()));
        }
        assert!(idle_peak < 0.01, "silent before any note, got {idle_peak}");

        let sounding = peak_after_note(&mut scene, "synth", 32);
        assert!(sounding > 0.05, "audible after note, got {sounding}");
    }

    /// The resolved `ParamHandle` controls the right node: driving the synth's LEVEL (device param 0)
    /// to zero silences it even with a note held — so generic `(device, param id)` addressing lands on
    /// the actual smoother.
    #[test]
    fn resolved_param_handle_controls_its_node() {
        let mut scene = build_patch(&canonical_patch(), BLOCK_LEN, rate(), 0).expect("builds");
        let level = *scene
            .param("synth", 0)
            .first()
            .expect("synth has param 0 (LEVEL)");

        let ev = scene.event_input("synth").expect("synth event input");
        let mut events = EventQueue::with_capacity(4);
        events.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: NOTE,
                velocity: 100,
            },
        );
        let mut params = ParamQueue::with_capacity(1);
        params.set(level, 0.0); // glide LEVEL from its 1.0 default to 0 (over ~5 ms)

        // Measure the *tail* only: LEVEL glides to 0 over ~5 ms, so the first blocks still sound while
        // it ramps down. By block 16 (~16 ms here) the glide is long settled — steady state is silence.
        let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
        let mut tail_peak = 0.0_f32;
        for block in 0..64 {
            scene
                .schedule_mut()
                .process_io(&mut out, &mut params, &mut events);
            if block >= 16 {
                tail_peak = out
                    .as_slice()
                    .iter()
                    .fold(tail_peak, |p, &v| p.max(v.abs()));
            }
        }
        assert!(
            tail_peak < 0.01,
            "LEVEL=0 should silence the voice, got {tail_peak}"
        );
    }

    /// Control resolution is total: unknown device, unknown param id, and a no-event device all return
    /// an empty handle slice / `None` rather than panicking.
    #[test]
    fn control_resolution_is_total() {
        let scene = build_patch(&canonical_patch(), BLOCK_LEN, rate(), 0).expect("builds");
        assert!(!scene.param("synth", 0).is_empty());
        assert!(scene.param("synth", 99).is_empty(), "param id out of range");
        assert!(scene.param("nope", 0).is_empty(), "unknown device");
        assert!(scene.event_input("synth").is_some());
        assert!(
            scene.event_input("spk").is_none(),
            "speaker has no event input"
        );
    }

    /// A multi-node device (the two-stage `channel_strip`) assembles inside a full patch: connections
    /// to its exposed ports remap onto the right internal nodes, and the chain still sounds.
    #[test]
    fn multi_node_device_in_a_patch() {
        let patch = Patch {
            devices: vec![
                device("synth", "synth_voice"),
                device("strip", "channel_strip"),
                device("ad", "ad_converter"),
                device("da", "da_converter"),
                device("spk", "speaker"),
            ],
            connections: vec![
                conn("synth", 0, "strip", 0),
                conn("strip", 0, "ad", 0),
                conn("ad", 0, "da", 0),
                conn("da", 0, "spk", 0),
            ],
            output: PortRef {
                device: "spk".into(),
                port: 0,
            },
        };
        let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("multi-node patch builds");
        // The strip exposes each stage's gain + power, concatenated in node order: device params
        // 0..=3 (in_gain, in_power, out_gain, out_power); 4 is past the face.
        assert!(!scene.param("strip", 0).is_empty());
        assert!(!scene.param("strip", 3).is_empty());
        assert!(scene.param("strip", 4).is_empty());
        let sounding = peak_after_note(&mut scene, "synth", 32);
        assert!(sounding > 0.05, "audible through the strip, got {sounding}");
    }

    /// A device-level param group resolves, through `build_patch`, to **all** its target handles: the
    /// 8i6's single Power control (exposed param id 8, after the preamps' gain/pad/air and the two
    /// monitor/phones gains) drives every stage's `powered` — seven handles from one id — while an
    /// ungrouped gain drives exactly one. This is the plumbing the wasm `set_param` fans a value over.
    #[test]
    fn device_power_group_fans_out_to_every_stage() {
        let patch = Patch {
            devices: vec![device("if", "scarlett_8i6")],
            connections: vec![],
            output: PortRef {
                device: "if".into(),
                port: 2, // the monitor line out (analog)
            },
        };
        let scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("8i6 patch builds");
        assert_eq!(
            scene.param("if", 8).len(),
            7,
            "the Power group drives all seven stages"
        );
        assert_eq!(
            scene.param("if", 0).len(),
            1,
            "an ungrouped gain drives exactly one stage"
        );
    }

    /// The INST/hi-Z structural config toggles a preamp's input impedance, which the loading divider
    /// bakes at compile — so the *same* patch, built with `inst1` off vs on, yields a **different**
    /// connection loss on a high-output-impedance source feeding preamp 1. A synth (≈1 Ω Zout) is too
    /// stiff to show it, so this drives the preamp through a lossy cable (its series R stands in for a
    /// high source impedance): line-Z (10 kΩ) loads it hard; inst-Z (1.5 MΩ) barely at all.
    #[test]
    fn inst_config_changes_the_baked_loading_divider() {
        let patch = |inst: f32| Patch {
            devices: vec![
                device("synth", "synth_voice"),
                DeviceInstance {
                    id: "if".into(),
                    type_id: "scarlett_8i6".into(),
                    params: vec![],
                    config: vec![crate::scene::ConfigSetting {
                        key: "inst1".into(),
                        value: inst,
                    }],
                },
            ],
            // synth out → preamp 1 in, through a 10 kΩ cable so the input impedance matters.
            connections: vec![Connection {
                from: PortRef {
                    device: "synth".into(),
                    port: 0,
                },
                to: PortRef {
                    device: "if".into(),
                    port: 0,
                },
                cable: Some(crate::scene::CableSpec {
                    resistance_ohms: 10_000.0,
                    capacitance_farads: 0.0,
                }),
            }],
            output: PortRef {
                device: "if".into(),
                port: 2,
            },
        };
        let loss = |inst: f32| {
            build_patch(&patch(inst), BLOCK_LEN, rate(), 0)
                .expect("builds")
                .connection_loading_loss(0)
                .expect("analog connection has a loss")
        };
        let line = loss(0.0); // 10 kΩ input: divider ≈ 10k/(1+10k+10k) ≈ 0.5 ⇒ ≈ −6 dB
        let inst = loss(1.0); // 1.5 MΩ input: divider ≈ 1.5M/(1+10k+1.5M) ≈ 0.993 ⇒ ≈ −0.06 dB
        assert!(line < -5.0, "line-Z loads the source hard, got {line} dB");
        assert!(inst > -0.2, "inst-Z barely loads it, got {inst} dB");
    }

    /// A cabled connection assembles (the cable's R·C rides the edge). Smoke test that the cable path
    /// builds and runs; the electrical effect itself is the engine's own tested concern.
    #[test]
    fn cabled_connection_builds() {
        let mut patch = canonical_patch();
        patch.connections[0].cable = Some(crate::scene::CableSpec {
            resistance_ohms: 150.0,
            capacitance_farads: 1e-9,
        });
        let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("cabled patch builds");
        assert!(peak_after_note(&mut scene, "synth", 32) > 0.05);
    }

    /// The static loading-loss readout reads each connection's baked divider. In the canonical patch
    /// `synth(1 Ω)→ad(1 MΩ)` [conn 0], `ad→da` digital [conn 1], `da(150 Ω)→spk(10 kΩ)` [conn 2]:
    /// hand calc for conn 2 is 10000/(150+10000) = 0.985222 → 20·log10 = −0.1293 dB; conn 0 is
    /// essentially unloaded; conn 1 is digital (ideal, no loss).
    #[test]
    fn connection_loading_loss_reads_the_baked_divider() {
        let scene = build_patch(&canonical_patch(), BLOCK_LEN, rate(), 0).expect("builds");

        let loss2 = scene.connection_loading_loss(2).expect("da→spk is analog");
        assert!((loss2 - (-0.1293)).abs() < 1e-3, "conn 2 loss {loss2}");

        let loss0 = scene
            .connection_loading_loss(0)
            .expect("synth→ad is analog");
        assert!(
            loss0.abs() < 1e-3,
            "1 Ω into 1 MΩ is ~unloaded, got {loss0}"
        );

        assert!(
            scene.connection_loading_loss(1).is_none(),
            "digital edge has no resistive loading loss"
        );
        assert!(
            scene.connection_loading_loss(9).is_none(),
            "out-of-range connection index"
        );
    }

    /// A cable's series resistance joins the divider and deepens the loading loss. A 1 kΩ cable on
    /// `da(150 Ω)→spk(10 kΩ)` gives 10000/(150+1000+10000) = 0.89686 → −0.9458 dB, well past the
    /// ideal wire's −0.1293 dB.
    #[test]
    fn a_cable_deepens_the_loading_loss() {
        let mut patch = canonical_patch();
        patch.connections[2].cable = Some(crate::scene::CableSpec {
            resistance_ohms: 1000.0,
            capacitance_farads: 1e-9,
        });
        let scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("builds");
        let loss = scene.connection_loading_loss(2).expect("analog");
        assert!((loss - (-0.9458)).abs() < 1e-3, "cabled conn 2 loss {loss}");
    }

    /// A metering device's readouts resolve by `(device, id)` and appear in the snapshot. A silent
    /// scene reads the VU meter's floor; resolution is total (out-of-range id, non-meter device,
    /// unknown device all → `None`).
    #[test]
    fn meter_readout_resolves_and_snapshots() {
        // synth → vu (inline analog meter) → ad → da → spk.
        let patch = Patch {
            devices: vec![
                device("synth", "synth_voice"),
                device("vu", "vu_meter"),
                device("ad", "ad_converter"),
                device("da", "da_converter"),
                device("spk", "speaker"),
            ],
            connections: vec![
                conn("synth", 0, "vu", 0),
                conn("vu", 0, "ad", 0),
                conn("ad", 0, "da", 0),
                conn("da", 0, "spk", 0),
            ],
            output: PortRef {
                device: "spk".into(),
                port: 0,
            },
        };
        let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("builds");

        assert!(scene.readout("vu", 0).is_some(), "VU readout");
        assert!(scene.readout("vu", 1).is_some(), "peak-dBu readout");
        assert!(scene.readout("vu", 2).is_none(), "readout id out of range");
        assert!(
            scene.readout("synth", 0).is_none(),
            "the synth exposes no readouts"
        );
        assert!(scene.readout("nope", 0).is_none(), "unknown device");

        // Render a few silent blocks; the VU meter settles at its reading floor.
        let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
        for _ in 0..8 {
            scene.schedule_mut().process(&mut out);
        }
        let snapshot = scene.readout_snapshot();
        let vu = snapshot
            .iter()
            .find(|(d, _)| d == "vu")
            .expect("vu in snapshot");
        assert_eq!(vu.1.len(), 2, "VU + peak");
        assert!(vu.1[0] < -50.0, "silent VU near floor, got {}", vu.1[0]);
    }

    #[test]
    fn unknown_type_is_an_error() {
        let patch = Patch {
            devices: vec![device("x", "no_such_device")],
            connections: vec![],
            output: PortRef {
                device: "x".into(),
                port: 0,
            },
        };
        let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
        assert!(matches!(err, BuildError::UnknownType { .. }), "got {err:?}");
    }

    #[test]
    fn duplicate_device_id_is_an_error() {
        let patch = Patch {
            devices: vec![device("dup", "speaker"), device("dup", "speaker")],
            connections: vec![],
            output: PortRef {
                device: "dup".into(),
                port: 0,
            },
        };
        let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
        assert!(
            matches!(err, BuildError::DuplicateDevice { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn unknown_device_reference_is_an_error() {
        let mut patch = canonical_patch();
        patch.connections.push(conn("ghost", 0, "spk", 0));
        let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
        assert!(
            matches!(err, BuildError::UnknownDevice { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn port_out_of_range_is_an_error() {
        let mut patch = canonical_patch();
        patch.output.port = 5; // speaker has only output port 0
        let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
        assert!(
            matches!(err, BuildError::OutputPortOutOfRange { .. }),
            "got {err:?}"
        );
    }

    /// `ConnectorMismatch` renders a legible message (the worklet surfaces build errors as text).
    /// The rejection path itself needs two **same-domain** ports with different connectors, which the
    /// current all-¼" analog catalog can't yet produce — that integration test arrives with Epic-5 gear
    /// (XLR mics / speakON). Until then the gate is covered by `connectors_compatible` (catalog) + the
    /// TS `evaluateConnection` mirror; here we at least pin the error's Display.
    #[test]
    fn connector_mismatch_displays_legibly() {
        let e = BuildError::ConnectorMismatch {
            from: "amp".into(),
            to: "spk".into(),
            from_connector: Connector::Xlr,
            to_connector: Connector::QuarterInch,
        };
        let msg = format!("{e}");
        assert!(msg.contains("incompatible connectors"), "got {msg}");
        assert!(msg.contains("amp") && msg.contains("spk"), "names in {msg}");
    }

    /// A cross-domain wire (digital output → analog input) is left to the engine's domain check and
    /// surfaces as `Compile(DomainMismatch)`, not a panic and not a `ConnectorMismatch` — the connector
    /// gate is domain-scoped, so cross-domain falls through to compile (domain-then-connector precedence).
    #[test]
    fn domain_mismatch_surfaces_as_compile_error() {
        let patch = Patch {
            devices: vec![device("ad", "ad_converter"), device("spk", "speaker")],
            // AD output is digital; speaker input is analog → DomainMismatch at compile.
            connections: vec![conn("ad", 0, "spk", 0)],
            output: PortRef {
                device: "spk".into(),
                port: 0,
            },
        };
        let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
        assert!(
            matches!(
                err,
                BuildError::Compile(CompileError::DomainMismatch { .. })
            ),
            "got {err:?}"
        );
    }
}
