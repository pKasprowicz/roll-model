//! The engine's own clock, handle and mixer, compiled into the example.
//!
//! They sit in a file of their own because a `#[path]` inside an inline `mod` is
//! resolved through a directory named after that module, which does not exist
//! (Linux refuses to walk through it; Windows does not mind).
#[path = "../../src/audio/clock.rs"]
pub mod clock;
#[path = "../../src/audio/handle.rs"]
pub mod handle;
#[path = "../../src/audio/mixer.rs"]
pub mod mixer;
