//! Aceleración por hardware de la decodificación de vídeo, inspirada en el
//! enfoque de VLC.
//!
//! VLC sondea la GPU disponible (dedicada o integrada en el procesador) y, solo
//! si existe, activa el decodificador por hardware para el flujo de vídeo,
//! independientemente del códec. Aquí se replica ese comportamiento sobre el
//! motor mpv: este módulo es independiente del resto de la lógica de
//! reproducción y se aplica a todas las instancias de mpv (reproductor y
//! espejos) por igual.

/// Sondea si existe una GPU disponible, ya sea dedicada o integrada en el
/// procesador (iGPU).
///
/// Se comprueba la presencia de algún nodo de render de DRM
/// (`/dev/dri/renderD*`), que tanto las tarjetas dedicadas (AMD, NVIDIA) como
/// las integradas (Intel/AMD iGPU) exponen cuando el controlador está cargado.
/// Es el mismo mecanismo que libva/VA-API usa para localizar el dispositivo de
/// decodificación.
pub fn has_gpu() -> bool {
    std::fs::read_dir("/dev/dri")
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|e| e.file_name().to_string_lossy().starts_with("renderD"))
        })
        .unwrap_or(false)
}

/// Activa o desactiva la aceleración por hardware en un `builder` de mpv.
///
/// Si hay GPU (dedicada o integrada) se habilita `hwdec=auto`, que auto-detecta
/// el backend (VA-API, VDPAU, NVDEC, ...) y lo aplica a cualquier códec sin que
/// el formato lo exija. Si no hay GPU, se fuerza `hwdec=no` para evitar
/// fallbacks costosos en el decodificador.
pub fn apply_to(builder: &mut mpv::MpvHandlerBuilder) -> mpv::Result<()> {
    if has_gpu() {
        builder.set_option("hwdec", "auto")
    } else {
        builder.set_option("hwdec", "no")
    }
}
