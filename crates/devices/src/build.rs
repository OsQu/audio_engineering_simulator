//! Assemble a runnable [`Patch`] into a compiled engine [`Schedule`].
//!
//! This is where the per-device chassis seam ([`crate::catalog::instantiate_described`]) meets the
//! whole scene. [`build_patch`]:
//! 1. **instantiates** each device into a fresh `Graph` (1..N nodes + internal edges), keying its
//!    [`BuiltDevice`] map by the scene device id;
//! 2. **remaps** each scene [`Connection`] — addressed by `(device, device-port)` — through those maps
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
use std::fmt;

use crate::assets::{AssetBuildError, BuildContext, SoundFontBankLibrary};
use engine::{
    AnalogRate, Cable, CompileError, DawControl, EventInputId, Farads, Graph, NodeId, Ohms,
    ParamHandle, ReadoutHandle, Schedule, compile,
};

use crate::cable_matches_connectors;
use crate::catalog::{
    BuiltDevice, Connector, DeviceConfig, DeviceDescriptor, PortDescriptor, PortDirection,
    PortDomain, connectors_compatible, instantiate_described,
};
use crate::scene::{CableSpec, Connection, Patch, PortRef};

/// Why assembling a [`Patch`] failed — all caught off the audio thread (the hot path never sees a
/// patch). Each variant names the offending scene element so the UI can point at it.
#[derive(Debug)]
pub enum BuildError {
    /// An asset-backed device could not resolve its required parsed bank or construct its source.
    Asset(AssetBuildError),
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
    /// The output tap resolves to a **non-analog** port. The engine renders the tap as a voltage, so it
    /// must be an analog output — a digital (or events) tap would fault the render (`unreachable`), which
    /// is session-fatal. Rejected here at build so `load_patch` keeps the running scene instead of
    /// trapping. Tap an analog output, or route the digital output through a DA first (a monitor chain).
    OutputTapNotAnalog { device: String, port: u32 },
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
    /// A connection is flagged **duplex** but one of its endpoints isn't a duplex jack (no
    /// `duplex_partner`), so there's no paired port to carry the reverse leg. Names the offending port.
    NotDuplex { device: String, port: u32 },
    /// The engine rejected the assembled graph (domain mismatch, cycle, indivisible rate, …).
    Compile(CompileError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Asset(error) => write!(f, "asset build error: {error}"),
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
            Self::OutputTapNotAnalog { device, port } => {
                write!(
                    f,
                    "output tap {device:?} port {port} is not an analog output"
                )
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
            Self::NotDuplex { device, port } => {
                write!(f, "device {device:?} port {port} is not a duplex jack")
            }
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

impl From<AssetBuildError> for BuildError {
    fn from(error: AssetBuildError) -> Self {
        Self::Asset(error)
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
    /// device id → the [`NodeId`] of its DAW node (the one whose [`daw`](engine::Node::daw) is `Some`),
    /// for devices that carry one (only the `computer`). The transport/track/byte seam the schedule's
    /// handle stores can't reach — resolved by probing each device's nodes at build (see
    /// [`daw`](Self::daw)).
    daw_nodes: BTreeMap<String, NodeId>,
    /// Per scene [`Connection`] (same index as `patch.connections`): the edge's
    /// **loading loss** in dB (`20·log10` of the baked resistive divider gain, so ≤ 0 dB — an
    /// attenuation), or `None` for a digital/event connection (ideal, no resistive loading). The
    /// static analog-domain readout the UI shows per cable.
    connection_losses: Vec<Option<f32>>,
    /// device id → its **per-instance** [`DeviceDescriptor`], described from the same expansion that
    /// built the device (so a config-driven face — the computer's `usb_sends`/`usb_returns` — reports
    /// its actual ports/params/readouts). The host reads these instead of re-describing each device.
    descriptors: BTreeMap<String, DeviceDescriptor>,
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

    /// Resolve `device` to its live [`DawControl`] surface — the off-block seam onto its transport,
    /// tracks, and file-byte streams — or `None` if the device isn't a DAW. Routes `device → node →
    /// daw()` through the compiled schedule. Off the hot path (a host gesture between blocks).
    pub fn daw(&mut self, device: &str) -> Option<&mut dyn DawControl> {
        // Copy the `NodeId` out first so the immutable `daw_nodes` borrow ends before the mutable
        // `schedule` borrow (`NodeId` is `Copy`).
        let node = *self.daw_nodes.get(device)?;
        self.schedule.node_mut(node)?.daw()
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

    /// Every device's per-instance descriptor, keyed by scene device id — each sized to that
    /// instance's own config, built once alongside the device.
    #[must_use]
    pub fn device_descriptors(&self) -> &BTreeMap<String, DeviceDescriptor> {
        &self.descriptors
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
/// `descs` is keyed by scene device id: each instance's own descriptor, so two instances of a type
/// with different configs are each checked against their own face.
fn port_descriptor<'a>(
    descs: &'a BTreeMap<String, DeviceDescriptor>,
    device: &str,
    direction: PortDirection,
    port: u32,
) -> Option<&'a PortDescriptor> {
    descs
        .get(device)?
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

/// Resolve and add one directed edge (output `from` → input `to`) to the graph: the same-domain
/// connector-shape and digital channel-count checks, then dispatch to the right `connect_*` — delayed
/// if the source is a round-trip-latency output, cabled if a cable is given, else an ideal wire. Shared
/// by an ordinary connection and by each leg of a duplex link.
fn add_edge(
    graph: &mut Graph,
    devices: &BTreeMap<String, BuiltDevice>,
    descs: &BTreeMap<String, DeviceDescriptor>,
    from: &PortRef,
    to: &PortRef,
    cable: &Option<CableSpec>,
) -> Result<(), BuildError> {
    let (from_node, from_port) = resolve_output(devices, from)?;
    let (to_node, to_port) = resolve_input(devices, to)?;

    // Same-domain physical-fit checks (a cross-domain wire is the engine's `DomainMismatch` at compile,
    // mirroring the UI's domain-then-connector precedence). Both ports resolved above.
    if let (Some(fp), Some(tp)) = (
        port_descriptor(descs, &from.device, PortDirection::Output, from.port),
        port_descriptor(descs, &to.device, PortDirection::Input, to.port),
    ) && fp.domain == tp.domain
    {
        // Connector-shape compatibility — a hard mechanical constraint (an XLR won't seat in a ¼" jack).
        let cable_adapts = cable.as_ref().is_some_and(|spec| {
            spec.type_id.as_deref().is_some_and(|type_id| {
                cable_matches_connectors(type_id, fp.connector, tp.connector)
            })
        });
        if !connectors_compatible(fp.connector, tp.connector) && !cable_adapts {
            return Err(BuildError::ConnectorMismatch {
                from: from.device.clone(),
                to: to.device.clone(),
                from_connector: fp.connector,
                to_connector: tp.connector,
            });
        }
        // A digital link must carry the same channel count on both ends (an 8-wide send can't feed a
        // 2-wide return). The engine enforces this too (`LaneCountMismatch`); surfacing it here gives a
        // legible, pre-compile error the web mirror can also show live.
        if fp.domain == PortDomain::Digital && fp.channels != tp.channels {
            return Err(BuildError::ChannelCountMismatch {
                from: from.device.clone(),
                to: to.device.clone(),
                from_channels: fp.channels,
                to_channels: tp.channels,
            });
        }
    }

    // A **round-trip-latency** output (a computer/DAW's playback, one block behind its input) wires
    // through a delayed edge — the *hint* that places the block of latency on the physically-correct
    // leg; `compile` would otherwise auto-break the cycle at some digital edge. Latency is a digital
    // round-trip, so it overrides any cable (a delayed edge is an ideal copy).
    let delayed = port_descriptor(descs, &from.device, PortDirection::Output, from.port)
        .is_some_and(|fp| fp.delayed);

    match (delayed, cable) {
        (true, _) => graph.connect_delayed(from_node, from_port, to_node, to_port),
        (false, Some(cable)) => graph.connect_cabled(
            from_node,
            from_port,
            to_node,
            to_port,
            Cable::new(
                Ohms::new(cable.resistance_ohms),
                Farads::new(cable.capacitance_farads),
            ),
        ),
        (false, None) => graph.connect_ideal(from_node, from_port, to_node, to_port),
    }
    Ok(())
}

/// The **reverse leg** of a duplex connection. Given `conn` (device X's output → device Y's input, each
/// half of a duplex jack), returns the refs for the return edge: device Y's paired **output** → device
/// X's paired **input**. Errors [`BuildError::NotDuplex`] if either endpoint isn't a duplex jack.
fn duplex_reverse(
    descs: &BTreeMap<String, DeviceDescriptor>,
    conn: &Connection,
) -> Result<(PortRef, PortRef), BuildError> {
    let x_in = port_descriptor(
        descs,
        &conn.from.device,
        PortDirection::Output,
        conn.from.port,
    )
    .and_then(|p| p.duplex_partner)
    .ok_or_else(|| BuildError::NotDuplex {
        device: conn.from.device.clone(),
        port: conn.from.port,
    })?;
    let y_out = port_descriptor(descs, &conn.to.device, PortDirection::Input, conn.to.port)
        .and_then(|p| p.duplex_partner)
        .ok_or_else(|| BuildError::NotDuplex {
            device: conn.to.device.clone(),
            port: conn.to.port,
        })?;
    Ok((
        PortRef {
            device: conn.to.device.clone(),
            port: y_out,
        },
        PortRef {
            device: conn.from.device.clone(),
            port: x_in,
        },
    ))
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
    let assets = SoundFontBankLibrary::default();
    build_patch_with_assets(patch, block_len, rate, seed, &assets)
}

/// Assemble a patch with the parsed assets its asset-backed devices may borrow. The library is an
/// explicit cold-path input: it is never stored in the schedule or consulted on the audio path.
pub fn build_patch_with_assets(
    patch: &Patch,
    block_len: usize,
    rate: AnalogRate,
    seed: u64,
    soundfonts: &SoundFontBankLibrary,
) -> Result<BuiltScene, BuildError> {
    let context = BuildContext::new(soundfonts);
    let mut graph = Graph::new();

    // 1. Instantiate every device, keyed by its (unique) scene id, together with its per-instance
    //    descriptor (from the same expansion — each device is built once). The descriptors drive the
    //    connector / channel-count / duplex / output-tap checks below, against each instance's own
    //    config-driven face.
    let mut devices: BTreeMap<String, BuiltDevice> = BTreeMap::new();
    let mut descs: BTreeMap<String, DeviceDescriptor> = BTreeMap::new();
    for device in &patch.devices {
        if devices.contains_key(&device.id) {
            return Err(BuildError::DuplicateDevice {
                id: device.id.clone(),
            });
        }
        let config = DeviceConfig::new(&device.config);
        let (built, descriptor) =
            instantiate_described(&device.type_id, &config, &mut graph, &context)?.ok_or_else(
                || BuildError::UnknownType {
                    id: device.id.clone(),
                    type_id: device.type_id.clone(),
                },
            )?;
        devices.insert(device.id.clone(), built);
        descs.insert(device.id.clone(), descriptor);
    }

    // 2. Remap each scene connection (and the output tap) through the device maps to graph edges,
    //    recording each connection's graph edge index (edges are appended in call order, after the
    //    devices' internal edges) so its baked loading loss can be read back after compile.
    let mut connection_edges: Vec<usize> = Vec::with_capacity(patch.connections.len());
    for conn in &patch.connections {
        // Record the **forward** edge's index so its baked loading loss reads back 1:1 with
        // `patch.connections`. A duplex link's reverse leg is added to the graph below but not tracked
        // here (it's digital — no resistive loss to report), keeping the loss vector aligned.
        connection_edges.push(graph.connection_count());
        add_edge(
            &mut graph,
            &devices,
            &descs,
            &conn.from,
            &conn.to,
            &conn.cable,
        )?;

        // A duplex connector carries both directions over one physical cable: add the return leg
        // (device Y's paired output → device X's paired input). The two edges form a digital cycle,
        // which `compile` breaks with one block of round-trip latency.
        if conn.duplex {
            let (rev_from, rev_to) = duplex_reverse(&descs, conn)?;
            add_edge(
                &mut graph,
                &devices,
                &descs,
                &rev_from,
                &rev_to,
                &conn.cable,
            )?;
        }
    }
    let (out_node, out_port) = resolve_output(&devices, &patch.output)?;
    // The output tap is rendered as a voltage, so it must resolve to an **analog** output port. A digital
    // (or events) tap would fault the engine's render (`unreachable`) — session-fatal — so reject it here
    // at build; `load_patch` then keeps the running scene rather than trapping. (The port exists — it just
    // resolved — so its descriptor is present; a missing descriptor can't happen, and falling through in
    // that impossible case only defers to the pre-existing render behavior.)
    if let Some(pd) = port_descriptor(
        &descs,
        &patch.output.device,
        PortDirection::Output,
        patch.output.port,
    ) && pd.domain != PortDomain::Analog
    {
        return Err(BuildError::OutputTapNotAnalog {
            device: patch.output.device.clone(),
            port: patch.output.port,
        });
    }
    graph.set_output(out_node, out_port);

    // 3. Compile (fixed seed → reproducible). Engine validation (domain, cycles, rates) lands here.
    let mut schedule = compile(graph, block_len, rate, seed)?;

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

    // 5. Resolve each device's DAW node (if any) by probing its nodes for the one whose `daw()` hook
    //    answers `Some` — the transport/track/byte seam that lives inside the node, not in a handle
    //    store. Needs `&mut schedule` (the hook is `&mut self`), so it's its own pass after the
    //    immutable handle resolution above.
    let mut daw_nodes = BTreeMap::new();
    for device in &patch.devices {
        let built = &devices[&device.id];
        if let Some(&node) = built.nodes.iter().find(|&&node| {
            schedule
                .node_mut(node)
                .and_then(engine::Node::daw)
                .is_some()
        }) {
            daw_nodes.insert(device.id.clone(), node);
        }
    }

    Ok(BuiltScene {
        schedule,
        params,
        events,
        readouts,
        connection_losses,
        daw_nodes,
        descriptors: descs,
    })
}

#[cfg(test)]
mod tests;
