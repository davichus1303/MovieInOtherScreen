/*! Application domain logic, in pure Rust with no GTK dependencies.
 *
 * This is the "core" layer of the architectural principle: it separates business
 * rules (monitor identification and playlist handling) from the UI and concrete
 * backends. By not depending on GTK, it can be tested in isolation and quickly
 * with `cargo test`.
 */

pub mod monitors;
pub mod video_list;
