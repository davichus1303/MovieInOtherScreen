//! Registro de actividad de la aplicación en un fichero de texto.
//!
//! Escribe entradas con marca de tiempo a un fichero bajo el directorio de
//! datos XDG (`$XDG_DATA_HOME` o `~/.local/share`), lo que permite revisar
//! después si la reproducción se inició correctamente y, en caso contrario,
//! qué error impidió reproducir.
//!
//! La escritura está serializada con un `Mutex` global para poder usarla
//! desde el hilo del motor mpv (que corre en paralelo a la UI) sin pisar
//! entradas. Nunca provoca fallos de la aplicación: si el fichero no puede
//! abrirse o escribirse, el error se ignora (el log es diagnóstico, no una
//! parte crítica del flujo).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

/// Nombre del fichero de log dentro del directorio de datos.
const LOG_FILE_NAME: &str = "movies-on-other-screens.log";

/// Serializa las escrituras al fichero entre hilos.
static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Nivel de severidad de una entrada de log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Información normal (p. ej. "reproducción iniciada").
    Info,
    /// Algo inesperado pero no fatal.
    Warning,
    /// Un fallo que impide completar la acción solicitada.
    Error,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warning => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// Ubicación del fichero de log en el sistema (directorio de datos XDG).
///
/// Prioriza `$XDG_DATA_HOME`; si no está definida, usa `~/.local/share`.
pub fn log_file_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".local").join("share")
        });
    base.join("movies-on-other-screens").join(LOG_FILE_NAME)
}

/// Escribe una entrada de log con nivel y mensaje dados.
///
/// Devuelve `false` si el fichero no se pudo abrir o escribir (diagnóstico),
/// pero nunca falla la aplicación.
pub fn log(level: Level, message: &str) -> bool {
    let lock = LOG_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = match lock.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{now}] {}: {message}\n", level.tag());

    let path = log_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => {
            let result = file.write_all(line.as_bytes());
            let _ = file.flush();
            result.is_ok()
        }
        Err(_) => false,
    }
}

/// Registra un mensaje informativo (estado de la reproducción).
pub fn info(message: impl AsRef<str>) -> bool {
    log(Level::Info, message.as_ref())
}

/// Registra una advertencia.
pub fn warn(message: impl AsRef<str>) -> bool {
    log(Level::Warning, message.as_ref())
}

/// Registra un error.
pub fn error(message: impl AsRef<str>) -> bool {
    log(Level::Error, message.as_ref())
}
