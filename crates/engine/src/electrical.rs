//! Electrical primitives and the local solve.
//!
//! We model **between** devices, not inside them (PROJECT_PLAN §5.3). Each output is a
//! Thévenin source ([`Thevenin`]: ideal voltage + series output impedance), each input
//! has an input impedance ([`InputZ`]), and impedances are [`Ohms`]. Because pro devices
//! buffer their I/O, connections solve **locally** — a voltage divider, no global nodal
//! solve — and fan-out is parallel input impedances.
//!
//! Impedance is **resistive (real)** here; the only reactive element, the cable's shunt
//! capacitance, is modeled as a separate one-pole filter rather than a complex `Ohms`.
//! Higher-order/reactive behavior is not modeled; the connection seam is kept open for it.
//!
//! A face is **unbalanced** (one conductor) or **balanced** (two conductors, V+/V−); see
//! [`InputZ::balanced`] / [`OutputZ::balanced`]. For a balanced face the stored impedance is the
//! **differential** impedance, and the schedule applies the resulting divider gain to each
//! conductor of the edge. The impedances themselves are still scalar [`Ohms`].

mod cable;
mod divider;
mod farads;
mod input_z;
mod ohms;
mod output_z;
mod phantom;
mod thevenin;

pub use cable::{Cable, OnePole};
pub use divider::divider_gain;
pub(crate) use divider::fan_out_gains;
pub use farads::Farads;
pub use input_z::InputZ;
pub use ohms::Ohms;
pub use output_z::OutputZ;
pub use phantom::{PhantomLoad, PhantomSupply};
pub use thevenin::Thevenin;
