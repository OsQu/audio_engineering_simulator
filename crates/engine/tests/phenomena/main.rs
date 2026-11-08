//! End-to-end physics on compiled patches: each module builds a real graph, compiles it, runs it,
//! and asserts the emergent behaviour (noise floors, DC, clipping, balanced rejection, phantom,
//! smoothing, a playable voice) against hand-computed numbers. They drive the engine through its
//! public API only; compile/process mechanics, and the physics tests that inspect `Schedule`
//! internals, stay in the crate's `schedule` unit tests.

mod support;

mod balanced;
mod clipping;
mod cmrr;
mod dc;
mod hum;
mod noise;
mod param;
mod phantom;
mod pickup;
mod playable_voice;
mod unbalanced_into_balanced;
