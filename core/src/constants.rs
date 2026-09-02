/*! Centralized domain constants of the `mos-core` crate, grouped by area. */

pub mod config {
    /** Name of the configuration file inside the data directory. */
    pub const FILE_NAME: &str = "movies-on-other-screens.conf";
    /** Separator between a config key and its value. */
    pub const KEY_VALUE_SEPARATOR: char = '=';
    /** Prefix that marks a comment line in the config file. */
    pub const COMMENT_PREFIX: char = '#';
    /** Escape character used in config values. */
    pub const ESCAPE_CHAR: char = '\\';
}

pub mod segments {
    /** Upper clamp to keep a fraction below `1.0`. */
    pub const MAX_FRACTION_CLAMP: f64 = 0.999_999;
}

pub mod playback {
    /** Name of the playback engine thread. */
    pub const ENGINE_THREAD_NAME: &str = "playback-engine";
}
