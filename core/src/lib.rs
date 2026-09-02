/*! Application domain logic, in pure Rust with no GTK dependencies.
 *
 * This is the "core" layer of the architectural principle: it separates business
 * rules (navigation, monitor selection, audio selection, playback control,
 * persistence) from the UI and concrete backends. By not depending on GTK, it
 * can be tested in isolation and quickly with `cargo test`.
 */

pub mod audio;
pub mod config;
pub mod mirror;
pub mod monitors;
pub mod playback;
pub mod player;
pub mod segments;
pub mod video_list;
