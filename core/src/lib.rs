//! Lógica de dominio de la aplicación, en Rust puro y sin dependencias de GTK.
//!
//! Esta es la capa "núcleo" del principio arquitectónico: separa las reglas de
//! negocio (navegación, selección de monitores, selección de audio, control de
//! reproducción, persistencia) de la UI y de los backends concretos. Al no
//! depender de GTK, se puede probar de forma aislada y rápida con `cargo test`.

pub mod audio;
pub mod config;
pub mod monitors;
pub mod player;
pub mod segments;
pub mod video_list;
