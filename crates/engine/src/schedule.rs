//! Compiling a [`Graph`](crate::Graph) into a runnable schedule, and running it.
//!
//! `compile` (Task 1.3.5) will validate the graph, order it, allocate its buffers, and bake
//! each connection's local solve into a runnable schedule whose `process` is the hot path.
//! For now this module owns the [`topo`] sort the compiler builds on.

mod topo;
