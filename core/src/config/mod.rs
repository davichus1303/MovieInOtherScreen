/*! Configuration persistence in a simple text file.
 *
 * Only stores what adds real value to the user (e.g. the audio device).
 * Uses a simple `key=value` format, consistent with Linux configuration
 * file conventions; a database is not justified.
 */

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::constants::config::{
    COMMENT_PREFIX, ESCAPE_CHAR, FILE_NAME, KEY_VALUE_SEPARATOR,
};

/** Known configuration keys. */
pub mod keys {
    pub const AUDIO_DEVICE: &str = "audio_device";
}

/** In-memory configuration, as an ordered list of key/value pairs. */
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    entries: Vec<(String, String)>,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        if let Some(slot) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value.into();
        } else {
            self.entries.push((key, value.into()));
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/** Configuration file location following XDG conventions. */
pub fn config_file_path(xdg_config_home: impl AsRef<Path>) -> PathBuf {
    let base = xdg_config_home.as_ref();
    let name = FILE_NAME.to_string();
    base.join(name)
}

/** Saves the configuration to the specified file (`key=value` format). */
pub fn save_to(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut content = String::new();
    for (key, value) in &config.entries {
        content.push_str(key);
        content.push(KEY_VALUE_SEPARATOR);
        content.push_str(&escape(value));
        content.push('\n');
    }
    std::fs::write(path, content)
}

/**
 * Loads the configuration from the specified file.
 *
 * A file that does not exist returns an empty configuration (not an error).
 */
pub fn load_from(path: &Path) -> io::Result<Config> {
    if !path.exists() {
        return Ok(Config::new());
    }
    let content = std::fs::read_to_string(path)?;
    let mut config = Config::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(COMMENT_PREFIX) {
            continue;
        }
        let Some((key, value)) = line.split_once(KEY_VALUE_SEPARATOR) else {
            // Línea malformada: se ignora, no se oculta el resto de la config.
            continue;
        };
        config.set(key.trim(), unescape(value.trim()));
    }
    Ok(config)
}

fn escape(value: &str) -> String {
    value.replace(ESCAPE_CHAR, "\\\\").replace('\n', "\\n")
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == ESCAPE_CHAR {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, value) in &self.entries {
            writeln!(f, "{key}={}", escape(value))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn set_y_get() {
        let mut c = Config::new();
        c.set("a", "1");
        c.set("a", "2");
        assert_eq!(c.get("a"), Some("2"));
        assert_eq!(c.get("b"), None);
    }

    #[test]
    fn round_trip_fichero() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sub").join("app.conf");
        let mut c = Config::new();
        c.set(keys::AUDIO_DEVICE, "hdmi-output");
        save_to(&path, &c).unwrap();

        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.get(keys::AUDIO_DEVICE), Some("hdmi-output"));
    }

    #[test]
    fn fichero_inexistente_devuelve_vacio() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("no-existe.conf");
        let loaded = load_from(&path).unwrap();
        assert!(loaded.get(keys::AUDIO_DEVICE).is_none());
    }

    #[test]
    fn ignora_lineas_malformadas_y_comentarios() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("c.conf");
        std::fs::write(&path, "# comentario\nsin-igual\nclave=valor\n").unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.get("clave"), Some("valor"));
    }
}
