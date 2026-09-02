/*! Centralized application constants, grouped by functional area.
 *
 * Keeps every magic number / repeated literal out of the code so it can be
 * reviewed and changed in one place.
 */

pub mod app {
    /** Application identifier (GApplication id). */
    pub const APPLICATION_ID: &str = "io.github.davichus1303.MoviesOnOtherScreens";
    /** Application title shown in the header bar and the window. */
    pub const APP_TITLE: &str = "Movies on Other Screens";
    /** Default window width. */
    pub const WINDOW_DEFAULT_WIDTH: i32 = 1200;
    /** Default window height. */
    pub const WINDOW_DEFAULT_HEIGHT: i32 = 760;
    /** Initial position of the `Paned` divider (sidebar width). */
    pub const SIDEBAR_INITIAL_POSITION: i32 = 360;
    /** Spacing of the root box. */
    pub const ROOT_BOX_SPACING: i32 = 0;
    /** Spacing of the main (player + monitors) column. */
    pub const MAIN_COLUMN_SPACING: i32 = 4;
    /** Label of the embedded video frame. */
    pub const LABEL_VIDEO_FRAME: &str = "Vídeo";
}

pub mod wayland {
    /** Value of `XDG_SESSION_TYPE` identifying a Wayland session. */
    pub const SESSION_VALUE_WAYLAND: &str = "wayland";
    /** Value of `XDG_SESSION_TYPE` identifying an X11 session. */
    pub const SESSION_VALUE_X11: &str = "x11";
    /** Environment variable that signals a Wayland session. */
    pub const ENV_WAYLAND_DISPLAY: &str = "WAYLAND_DISPLAY";
    /** Environment variable that signals an X11 display. */
    pub const ENV_DISPLAY: &str = "DISPLAY";
    /** Environment variable with the session type. */
    pub const ENV_SESSION_TYPE: &str = "XDG_SESSION_TYPE";
}

pub mod hwaccel {
    /** DRM device directory that exposes render nodes. */
    pub const DRM_DEV_DIR: &str = "/dev/dri";
    /** Prefix of the DRM render node files. */
    pub const DRM_RENDER_NODE_PREFIX: &str = "renderD";
    /** mpv option that controls hardware decoding. */
    pub const OPT_HWDEC: &str = "hwdec";
    /** Value of `hwdec` that enables auto-detected hardware decoding. */
    pub const OPT_HWDEC_AUTO: &str = "auto";
    /** Value of `hwdec` that disables hardware decoding. */
    pub const OPT_HWDEC_NO: &str = "no";
}

pub mod monitors {
    /** Prefix of the logical monitor ids created by the app. */
    pub const ID_PREFIX: &str = "gdk-";
    /** Fallback label for a monitor without a model name. */
    pub const LABEL_MONITOR_DEFAULT: &str = "Monitor";
    /** Default primary index when no monitor holds the origin. */
    pub const DEFAULT_PRIMARY_INDEX: usize = 0;
    /** Title of the monitors section. */
    pub const LABEL_SECTION_TITLE: &str = "Monitores";
    /** Message shown when no monitor was detected. */
    pub const LABEL_NO_MONITORS: &str = "No se detectó ningún monitor.";
    /** Label of a primary monitor. */
    pub const LABEL_MONITOR_PRIMARY: &str = "Principal";
    /** Label of a secondary monitor. */
    pub const LABEL_MONITOR_SECONDARY: &str = "Secundario";
    /** Card format (id, kind and label). */
    pub const CARD_FORMAT: &str = "{id}\n{kind}\n{label}";
    /** CSS class of a monitor card. */
    pub const CSS_CARD: &str = "card";
    /** Label of the button that identifies the screens. */
    pub const LABEL_IDENTIFY_BUTTON: &str = "Identificar pantallas";

    /** Constants for the screen identification overlay. */
    pub mod identify {
        /** CSS class of the identifier badge. */
        pub const CSS_LABEL: &str = "identify-label";
        /** How long the identification stays visible (ms). */
        pub const DURATION_MS: u32 = 5_000;
        /** Emphasized font size (pt) of the identifier badge. */
        pub const FONT_SIZE: i32 = 48;
        /** Background color (RGBA) of the identifier badge. */
        pub const BG_COLOR: &str = "rgba(0, 0, 0, 0.75)";
        /** Text color (RGBA) of the identifier badge. */
        pub const FG_COLOR: &str = "rgba(255, 255, 255, 1)";
        /** Corner radius (px) of the identifier badge. */
        pub const RADIUS: i32 = 12;
        /** Padding (px) of the identifier badge. */
        pub const PADDING: i32 = 16;
        /** Top margin of the badge (px). */
        pub const MARGIN_TOP: i32 = 16;
        /** End (right) margin of the badge (px). */
        pub const MARGIN_END: i32 = 16;
    }

    pub mod layout {
        /** Spacing of the section box. */
        pub const SECTION_SPACING: i32 = 8;
        /** Top margin of the section. */
        pub const MARGIN_TOP: i32 = 8;
        /** Bottom margin of the section. */
        pub const MARGIN_BOTTOM: i32 = 12;
        /** Start margin of the section. */
        pub const MARGIN_START: i32 = 12;
        /** End margin of the section. */
        pub const MARGIN_END: i32 = 12;
        /** Max width (chars) of the hint label. */
        pub const HINT_MAX_WIDTH_CHARS: i32 = 60;
        /** Spacing of the cards row. */
        pub const CARD_ROW_SPACING: i32 = 8;
        /** Width of a monitor card. */
        pub const CARD_WIDTH: i32 = 150;
        /** Height of a monitor card. */
        pub const CARD_HEIGHT: i32 = 70;
    }
}

pub mod audio {
    /** Placeholder shown while detecting audio devices. */
    pub const LABEL_LOADING: &str = "Cargando…";
    /** Title of the audio section. */
    pub const LABEL_SECTION_TITLE: &str = "Salida de audio";
    /** Description hint shown under the audio title. */
    pub const HINT_DESCRIPTION: &str =
        "Dispositivo por el que se escucha la reproducción. Si no aparece \
         ninguno, se usa el predeterminado del sistema.";
    /** Item format of the combo box entries. */
    pub const ITEM_FORMAT: &str = "{} ({})";
    /** CSS class for the section title. */
    pub const CSS_TITLE: &str = "title-4";
    /** Detection interval (ms) between audio scans. */
    pub const DETECTION_INTERVAL_MS: u32 = 5000;

    pub mod mock {
        /** Mock speakers device id. */
        pub const DEVICE_SPEAKERS: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo";
        /** Mock speakers label. */
        pub const LABEL_SPEAKERS: &str = "Speakers";
        /** Mock HDMI device id. */
        pub const DEVICE_HDMI: &str = "alsa_output.pci-0000_01_00.1.hdmi-stereo";
        /** Mock HDMI label. */
        pub const LABEL_HDMI: &str = "HDMI";
    }

    pub mod layout {
        /** Spacing of the section box. */
        pub const SECTION_SPACING: i32 = 8;
        /** Top margin of the section. */
        pub const MARGIN_TOP: i32 = 8;
        /** Bottom margin of the section. */
        pub const MARGIN_BOTTOM: i32 = 12;
        /** Start margin of the section. */
        pub const MARGIN_START: i32 = 12;
        /** End margin of the section. */
        pub const MARGIN_END: i32 = 12;
        /** Max width (chars) of the hint label. */
        pub const HINT_MAX_WIDTH_CHARS: i32 = 60;
    }
}

pub mod sidebar {
    /** Title of the videos section. */
    pub const LABEL_VIDEOS_TITLE: &str = "Archivos de video";
    /** Label of the "add videos" button. */
    pub const LABEL_ADD_BUTTON: &str = "＋ Agregar videos";
    /** Label of the "clear" button. */
    pub const LABEL_CLEAR_BUTTON: &str = "Limpiar";
    /** CSS class for the section title. */
    pub const CSS_TITLE: &str = "title-4";
    /** Name of the file filter shown in the dialog. */
    pub const FILE_FILTER_NAME: &str = "Vídeos";
    /** Title of the open file dialog. */
    pub const DIALOG_TITLE_OPEN: &str = "Seleccionar vídeos";
    /** Accept label of the open file dialog. */
    pub const DIALOG_ACCEPT_LABEL: &str = "Seleccionar";

    /** Video file extensions accepted by the file dialog. */
    pub const VIDEO_EXTENSIONS: [&str; 9] = [
        "*.mp4", "*.mkv", "*.webm", "*.avi", "*.mov", "*.m4v", "*.ogv", "*.ts", "*.m2ts",
    ];

    pub mod layout {
        /** Spacing of the sidebar. */
        pub const SPACING: i32 = 8;
        /** Top margin of the sidebar. */
        pub const MARGIN_TOP: i32 = 12;
        /** Bottom margin of the sidebar. */
        pub const MARGIN_BOTTOM: i32 = 12;
        /** Start margin of the sidebar. */
        pub const MARGIN_START: i32 = 12;
        /** End margin of the sidebar. */
        pub const MARGIN_END: i32 = 12;
        /** Requested width of the sidebar. */
        pub const WIDTH_REQUEST: i32 = 280;
        /** Spacing of the buttons row. */
        pub const BUTTONS_ROW_SPACING: i32 = 6;
    }
}

pub mod player_area {
    /** Maximum value of the progress scale (percent). */
    pub const SCALE_MAX_PERCENT: f64 = 100.0;
    /** Step of the progress scale. */
    pub const SCALE_STEP: f64 = 1.0;
    /** Initial / zero time label. */
    pub const TIME_LABEL_ZERO: &str = "00:00";
    /** CSS class for the time labels. */
    pub const CSS_CAPTION: &str = "caption";
    /** Lower clamp of the played fraction. */
    pub const FRAC_CLAMP_MIN: f64 = 0.0;
    /** Upper clamp of the played fraction. */
    pub const FRAC_CLAMP_MAX: f64 = 1.0;
    /** Seconds in an hour, for time formatting. */
    pub const SECS_PER_HOUR: u64 = 3600;
    /** Seconds in a minute, for time formatting. */
    pub const SECS_PER_MINUTE: u64 = 60;
    /** Format string for `hh:mm:ss` times. */
    pub const TIME_HMS_FORMAT: &str = "{h:02}:{m:02}:{s:02}";
    /** Format string for `mm:ss` times. */
    pub const TIME_MS_FORMAT: &str = "{m:02}:{s:02}";

    /** Icons of the transport control buttons. */
    pub mod controls {
        /** Seek-to-start icon. */
        pub const ICON_SEEK_START: &str = "⏮";
        /** Play icon. */
        pub const ICON_PLAY: &str = "▶";
        /** Pause icon. */
        pub const ICON_PAUSE: &str = "⏸";
        /** Stop icon. */
        pub const ICON_STOP: &str = "⏹";
    }

    pub mod layout {
        /** Spacing of the timeline row. */
        pub const ROW_SPACING: i32 = 8;
        /** Spacing of the controls row. */
        pub const CONTROLS_SPACING: i32 = 6;
        /** Top margin of the controls row. */
        pub const CONTROLS_MARGIN_TOP: i32 = 6;
        /** Bottom margin of the controls row. */
        pub const CONTROLS_MARGIN_BOTTOM: i32 = 6;
    }
}

pub mod mpv {
    /** mpv property with the current playback position. */
    pub const PROP_TIME_POS: &str = "time-pos";
    /** mpv property with the media duration. */
    pub const PROP_DURATION: &str = "duration";
    /** mpv property controlling playback pause. */
    pub const PROP_PAUSE: &str = "pause";
    /** mpv property with the output audio device. */
    pub const PROP_AUDIO_DEVICE: &str = "audio-device";
    /** mpv option that keeps the file open after it ends. */
    pub const OPT_KEEP_OPEN: &str = "keep-open";
    /** Value `yes` for boolean mpv options. */
    pub const VALUE_YES: &str = "yes";
    /** Value `no` for boolean mpv options. */
    pub const VALUE_NO: &str = "no";
    /** mpv option selecting the video output driver. */
    pub const OPT_VO: &str = "vo";
    /** Value of `vo` that embeds the output (`libmpv`). */
    pub const VALUE_VO_LIBMPV: &str = "libmpv";
    /** mpv option enabling/disabling audio. */
    pub const OPT_AUDIO: &str = "audio";
    /** mpv command to load/switch a file. */
    pub const CMD_LOADFILE: &str = "loadfile";
    /** mpv command to seek. */
    pub const CMD_SEEK: &str = "seek";
    /** mpv command to quit the core. */
    pub const CMD_QUIT: &str = "quit";
    /** mpv command to set the audio filter. */
    pub const CMD_AF: &str = "af";
    /** mpv command to set the video filter. */
    pub const CMD_VF: &str = "vf";
    /** Seek mode that jumps to an absolute position. */
    pub const SEEK_MODE_ABSOLUTE: &str = "absolute";
    /** Seek target representing the start of the media. */
    pub const SEEK_TO_START: &str = "0";
    /** Empty path used to detach the loaded file. */
    pub const EMPTY_LOAD_PATH: &str = "";
    /** Prefix that marks a PipeWire audio device id. */
    pub const AUDIO_PREFIX_PIPEWIRE: &str = "pipewire/";
    /** Prefix that marks a PulseAudio audio device id. */
    pub const AUDIO_PREFIX_PULSE: &str = "pulse/";
    /** Prefix that marks an ALSA audio device id. */
    pub const AUDIO_PREFIX_ALSA: &str = "alsa/";
    /** Template of the fade-in filter, without the duration. */
    pub const FADE_IN_TEMPLATE: &str = "fade in:st=0:d=";
}

pub mod mirror {
    /** Name of the mirror thread. */
    pub const THREAD_NAME: &str = "mpv-mirror";
    /** Idle sleep (ms) of the mirror thread. */
    pub const IDLE_SLEEP_MS: u64 = 5;
    /** Event polling timeout (non-blocking). */
    pub const EVENT_POLL_TIMEOUT_SECS: f64 = 0.0;
    /** Suffix used when logging a mirror seek position. */
    pub const LOG_SEEK_SUFFIX: &str = "s";

    pub mod messages {
        /** Reported when the mirror thread cannot be created. */
        pub const THREAD_CREATE_FAIL: &str = "No se pudo crear el hilo del monitor espejo";
        /** Reported when the mirror thread ended before starting. */
        pub const TERMINATED_EARLY: &str = "El monitor espejo terminó antes de poder iniciarse";
        /** Reported when sending a command to a mirror fails. */
        pub const SEND_FAIL: &str = "No se pudo enviar comando al espejo: ";
        /** Reported when the mirror mpv core cannot be created. */
        pub const CORE_CREATE_FAIL: &str = "No se pudo crear el espejo de mpv: ";
        /** Reported when the mirror core cannot start. */
        pub const CORE_INIT_FAIL: &str = "No se pudo iniciar el espejo de mpv: ";
        /** Prefix of the "no handle" mirror message. */
        pub const NO_HANDLE_PREFIX: &str = "espejo ";
        /** Suffix of the "no handle" mirror message. */
        pub const NO_HANDLE_SUFFIX: &str = ": sin handle de mpv";
    }

    pub mod logs {
        /** Logged when the mirror core was created. */
        pub const CORE_CREATED: &str = "[mirror] core de mpv creado";
        /** Logged when the mirror core ended. */
        pub const CORE_ENDED: &str = "[mirror] core de mpv finalizado";
        /** Prefix of the seek-on-load log. */
        pub const LOAD_SEEK_PREFIX: &str = "[mirror] cargado, saltando a ";
        /** Prefix of the file load log. */
        pub const LOADING_PREFIX: &str = "[mirror] cargando ";
    }
}

pub mod engine {
    /** Name of the mpv engine thread. */
    pub const THREAD_NAME: &str = "mpv-engine";
    /** Idle sleep (ms) of the engine thread. */
    pub const IDLE_SLEEP_MS: u64 = 5;
    /** Event polling timeout (non-blocking seconds). */
    pub const EVENT_POLL_TIMEOUT_SECS: f64 = 0.0;
    /** Observe reply id for the `time-pos` property. */
    pub const OBSERVE_ID_TIME_POS: u32 = 1;
    /** Observe reply id for the `duration` property. */
    pub const OBSERVE_ID_DURATION: u32 = 2;
    /** Observe reply id for the `pause` property. */
    pub const OBSERVE_ID_PAUSE: u32 = 3;
    /** Duration (seconds) of the transition fade. */
    pub const TRANSITION_SECONDS: f64 = 3.0;
}

pub mod reporting {
    /** Separator between the user message base and its detail. */
    pub const DETAIL_SEPARATOR: &str = ": ";
    /** Format of the log tag prefix. */
    pub const LOG_TAG_FORMAT: &str = "[{}] {}";
    /** Toast timeout (seconds). */
    pub const TOAST_TIMEOUT_SECS: u32 = 4;

    pub mod user_messages {
        /** User message for a mirror error. */
        pub const MIRROR: &str = "Error con un monitor espejo";
        /** User message for a playback error. */
        pub const PLAYER: &str = "Error de reproducción";
        /** User message for a video load error. */
        pub const VIDEO: &str = "No se pudo cargar el vídeo";
        /** User message for a monitors error. */
        pub const MONITORS: &str = "Error con los monitores";
        /** User message for an audio error. */
        pub const AUDIO: &str = "Error de audio";
        /** User message for an internal error. */
        pub const INTERNAL: &str = "Error interno";
    }

    pub mod tags {
        /** Log tag for the mirror category. */
        pub const MIRROR: &str = "monitor";
        /** Log tag for the player category. */
        pub const PLAYER: &str = "player";
        /** Log tag for the video category. */
        pub const VIDEO: &str = "video";
        /** Log tag for the monitors category. */
        pub const MONITORS: &str = "monitors";
        /** Log tag for the audio category. */
        pub const AUDIO: &str = "audio";
        /** Log tag for the internal category. */
        pub const INTERNAL: &str = "internal";
    }

    pub mod warnings {
        /** Warning when the reporting channel is closed. */
        pub const CHANNEL_CLOSED: &str =
            "No se pudo notificar el error a la interfaz (canal cerrado)";
        /** Warning when the UI is not attached yet. */
        pub const NOT_ATTACHED: &str = "Interfaz aún no registrada; error solo en logs";
    }
}

pub mod logging {
    /** Name of the log file inside the data directory. */
    pub const FILE_NAME: &str = "movies-on-other-screens.log";
    /** Subdirectory of the app data inside the XDG data dir. */
    pub const APP_DATA_DIR: &str = "movies-on-other-screens";
    /** `.local` segment of the fallback data path. */
    pub const DIR_LOCAL: &str = ".local";
    /** `share` segment of the fallback data path. */
    pub const DIR_SHARE: &str = "share";
    /** Format of a log line (the level tag is passed as an argument). */
    pub const LINE_FORMAT: &str = "[{now}] {}: {message}\n";

    pub mod levels {
        /** Level tag for `Info`. */
        pub const INFO: &str = "INFO";
        /** Level tag for `Warning`. */
        pub const WARN: &str = "WARN";
        /** Level tag for `Error`. */
        pub const ERROR: &str = "ERROR";
    }

    pub mod env {
        /** `$XDG_DATA_HOME` environment variable. */
        pub const XDG_DATA_HOME: &str = "XDG_DATA_HOME";
        /** `$HOME` environment variable. */
        pub const HOME: &str = "HOME";
    }
}

pub mod main_app {
    /** `LC_NUMERIC` locale category (POSIX). */
    pub const LC_NUMERIC: i32 = 1;
    /** Locale required by libmpv (`C`). */
    pub const LOCALE_C: &str = "C";
    /** Exit code used when the Wayland requirement is not met. */
    pub const EXIT_CODE_REQUIREMENT: i32 = 1;
    /** Fallback message of an unknown panic payload. */
    pub const MSG_UNKNOWN_PANIC: &str = "pánico desconocido";
    /** Name of the main thread of the mpv engine. */
    pub const ENGINE_THREAD_NAME: &str = "mpv-engine";

    pub mod messages {
        /** Logged when the app starts. */
        pub const LOG_STARTING: &str = "Iniciando Movies on Other Screens (PID {})";
        /** Warning when the environment is not Wayland. */
        pub const WARN_NON_WAYLAND: &str =
            "Entorno gráfico no compatible con Wayland; la app sale.";
        /** Logged when Wayland is detected. */
        pub const LOG_WAYLAND_OK: &str = "Entorno Wayland detectado.";
    }

    /** The full message shown when Wayland is required but absent. */
    pub const REQUIREMENT_MESSAGE: &str = "\
Movies on Other Screens requiere Wayland para funcionar.

Se ha detectado que la aplicación se está ejecutando bajo X11 o en un \
entorno sin Wayland. No se ofrece compatibilidad parcial con X11.

Inicia una sesión de GNOME sobre Wayland y vuelve a intentarlo.";
}

pub mod player {
    /** `LC_NUMERIC` locale category (POSIX). */
    pub const LC_NUMERIC: i32 = 1;
    /** Locale required by libmpv (`C`). */
    pub const LOCALE_C: &str = "C";

    pub mod messages {
        /** Message when the player cannot be initialized. */
        pub const INIT_FAIL: &str = "No se pudo inicializar el reproductor: ";
        /** Message when loading a video fails. */
        pub const LOAD_FAIL: &str = "Error al cargar el vídeo '{path}': ";
    }

    pub mod logs {
        /** Logged when the engine starts. */
        pub const ENGINE_STARTED: &str = "Motor mpv inicializado correctamente.";
        /** Prefix of the load log. */
        pub const LOAD_PREFIX: &str = "Cargando vídeo en el motor mpv: ";
    }
}

pub mod playback {
    pub mod logs {
        /** Prefix of the queued-playback log. */
        pub const QUEUED_PREFIX: &str = "Reproducción encolada: ";
        /** Prefix of the out-of-range index log. */
        pub const MISSING_INDEX_PREFIX: &str = "Índice fuera de rango al reproducir: ";
        /** Message when a send to the engine fails. */
        pub const SEND_FAIL: &str = "No se pudo enviar la orden de carga al motor mpv: ";
        /** Message when playback is requested with no selection. */
        pub const NO_SELECTION: &str = "Reproducir solicitado sin vídeo seleccionado";
    }
}

pub mod events {
    /** Interval (ms) of the event bridge (~30 fps). */
    pub const BRIDGE_INTERVAL_MS: u32 = 33;
}
