//! Render/CLI test harness for driving the engine offline — the library surface.
//!
//! Both the binary (`main.rs`) and the integration tests consume this. It is **test/render
//! tooling, not a second engine**: rendering is a loop over [`engine::Schedule`]'s `process_io`
//! plus the pieces that turn the result into a file (the WAV writer and the offline render
//! driver). The implicit capture lives in the shared [`capture`] crate so the browser bindings
//! can reuse it.

pub mod render;
pub mod wav;
