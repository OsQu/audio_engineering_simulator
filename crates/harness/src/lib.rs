//! Render/CLI test harness for driving the engine offline — the library surface.
//!
//! Both the binary (`main.rs`) and the integration tests consume this. It is **test/render
//! tooling, not a second engine**: rendering is a loop over [`engine::Schedule`]'s `process_io`
//! plus the pieces that turn the result into a file. Story 2.1 (see `IMPLEMENTATION_PLAN.md`)
//! fills it in — the implicit capture, the WAV writer, and the offline render driver.

pub mod capture;
