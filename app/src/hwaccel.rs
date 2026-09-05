/*
 * Hardware acceleration of video decoding, inspired by VLC's approach.
 *
 * VLC probes the available GPU (dedicated or integrated into the processor)
 * and, only if one exists, enables the hardware decoder for the video stream,
 * regardless of the codec. This replicates that behavior on the mpv engine:
 * this module is independent of the rest of the playback logic and is applied
 * to all mpv instances (player and mirrors) alike.
 *
 * Two adjustments are made on top of VLC's base approach, both learned from
 * crashes:
 *
 * 1. Intel driver selection. On Intel systems there are two VA-API drivers
 *    installed (the legacy `i965` and the modern `iHD`); libva guesses which
 *    to use and can fall back to `i965`, which fails outright on Iris Xe.
 *    When an Intel GPU is detected we force `LIBVA_DRIVER_NAME=iHD` (the
 *    driver that handles VP8/VP9/AV1) before mpv opens the device.
 * 2. AV1 excluded from hardware decoding. Even with `iHD`, this iGPU cannot
 *    decode AV1 reliably: the driver fails mid-stream (internal decoding
 *    error 23) and mpv segfaults instead of falling back to software.
 *    `hwdec-codecs` therefore restricts hardware decoding to the other common
 *    codecs, letting AV1 use the (verified working) software path.
 * 3. Software AV1 decoder. Every mpv decoder open prefers VideoLAN's `libdav1d`
 *    (video `vd` priority list). dav1d is the fastest AV1 software decoder and,
 *    being purely CPU, it keeps AV1 out of the VA-API driver no matter what.
 *    For the other codecs it does not match, mpv keeps its normal selection:
 *    `vd` is a *priority list*, not an exclusive one.
 */

use crate::constants::hwaccel::{
    DECODER_DAV1D, DRM_DEVICE_SUBDIR, DRM_DEV_DIR, DRM_RENDER_NODE_PREFIX, HWDEC_CODECS_NO_AV1,
    INTEL_VENDOR_ID, LIBVA_DRIVER_NAME_ENV, LIBVA_DRIVER_NAME_IHD, OPT_HWDEC, OPT_HWDEC_AUTO,
    OPT_HWDEC_CODECS, OPT_HWDEC_NO, OPT_VD, SYSFS_DRM_DIR, SYSFS_VENDOR_FILE,
};

/**
 * Probes whether a GPU is available, either dedicated or integrated into the
 * processor (iGPU).
 *
 * It checks for the presence of any DRM render node
 * (`/dev/dri/renderD*`), which both dedicated cards (AMD, NVIDIA) and
 * integrated ones (Intel/AMD iGPU) expose when the driver is loaded. It is the
 *  same mechanism libva/VA-API uses to locate the decoding device.
 */
pub fn has_gpu() -> bool {
    std::fs::read_dir(DRM_DEV_DIR)
        .map(|entries| {
            entries.filter_map(Result::ok).any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(DRM_RENDER_NODE_PREFIX)
            })
        })
        .unwrap_or(false)
}

/**
 * Reports whether any detected GPU is from Intel (`0x8086`).
 *
 * Intel maps each DRM card to a sysfs entry
 * (`/sys/class/drm/cardN/device/vendor`). A matching value means the VA-API
 * driver selection for Intel applies. It returns `false` (no forcing) if the
 * sysfs information is unavailable or does not match Intel.
 */
pub fn has_intel_gpu() -> bool {
    has_intel_gpu_in(SYSFS_DRM_DIR)
}

/**
 * Scans the given `sysfs_drm_dir` looking for a card whose vendor is Intel.
 *
 * Split from [`has_intel_gpu`] so the scan can be unit-tested against a
 * synthetic, mutable directory without reading the real sysfs. A card is only
 * considered when its entry name follows the `cardN` (numeric) convention.
 */
pub(crate) fn has_intel_gpu_in(sysfs_drm_dir: &str) -> bool {
    let Ok(cards) = std::fs::read_dir(sysfs_drm_dir) else {
        return false;
    };
    cards.filter_map(Result::ok).any(|card| {
        let name = card.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card")
            || !name
                .get(4..)
                .map(|tail| tail.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false)
        {
            return false;
        }
        let vendor = card.path().join(DRM_DEVICE_SUBDIR).join(SYSFS_VENDOR_FILE);
        std::fs::read_to_string(vendor)
            .map(|v| v.trim() == INTEL_VENDOR_ID)
            .unwrap_or(false)
    })
}

/**
 * Selects the modern Intel VA-API driver (`iHD`) for an Intel GPU.
 *
 * libva picks its driver from the `LIBVA_DRIVER_NAME` environment variable,
 * otherwise it guesses and may fall back to the legacy `i965` driver, which
 * fails to initialize at all on Iris Xe. Selecting `iHD` explicitly (the
 * driver that supports VP8/VP9/AV1) is the mpv/libva counterpart of VLC's
 * `--avcodec-hw=vaapi`.
 *
 * **Must only be called on `main`, before any thread (GTK, engine, mirror)
 * starts.** `LIBVA_DRIVER_NAME` is written through the process environment,
 * and writing it while other threads are alive is a data race (it reads and
 * reallocates `environ` non-atomically), which can crash the app at startup.
 * Because it runs before mpv opens the video device, the value is already in
 * place when any mpv instance is created.
 */
pub fn init() {
    if has_intel_gpu() {
        unsafe {
            // Equivalente de `setenv(3)` a nivel de proceso, ejecutado en el
            // hilo único de `main` (sin carreras de datos). Añadimos el enlace
            // dinámico a la libc y la llamada manual siguiendo el patrón usado
            // para `setlocale` en este mismo crate.
            setenv(
                LIBVA_DRIVER_NAME_ENV.as_ptr(),
                LIBVA_DRIVER_NAME_IHD.as_ptr(),
                1,
            );
        }
    }
}

#[link(name = "c")]
unsafe extern "C" {
    /// `setenv(3)`: sets/overrides an environment variable in the process.
    fn setenv(name: *const u8, value: *const u8, overwrite: i32) -> i32;
}

/**
 * Enables or disables hardware acceleration on an mpv `builder`.
 *
 * If there is a GPU (dedicated or integrated) `hwdec=auto` is enabled, which
 * auto-detects the backend (VA-API, VDPAU, NVDEC, ...) and applies it to any
 * codec without the format requiring it. If there is no GPU, `hwdec=no` is
 * forced to avoid costly fallbacks in the decoder.
 *
 * When hardware decoding is on, `hwdec-codecs` restricts it to the supported
 * high codecs and **excludes AV1**: the VA-API driver fails to decode AV1 on
 * several Intel iGPUs mid-stream and mpv crashes instead of falling back.
 *
 * AV1 is handled in software: `vd` prefers VideoLAN's `libdav1d`, the fastest
 * AV1 software decoder. It is a priority list, so for every other codec mpv
 * keeps its normal selection (hardware when `hwdec` allows it); for AV1 it
 * guarantees the crash never reaches the GPU. If `libdav1d` is missing from
 * the linked ffmpeg, mpv logs a warning and falls back to auto-selection.
 */
pub fn apply_to(builder: &mut mpv::MpvHandlerBuilder) -> mpv::Result<()> {
    builder.set_option(OPT_VD, DECODER_DAV1D)?;
    if has_gpu() {
        builder.set_option(OPT_HWDEC_CODECS, HWDEC_CODECS_NO_AV1)?;
        builder.set_option(OPT_HWDEC, OPT_HWDEC_AUTO)
    } else {
        builder.set_option(OPT_HWDEC, OPT_HWDEC_NO)
    }
}

#[cfg(test)]
mod tests {
    use super::has_intel_gpu_in;
    use crate::constants::hwaccel::{DRM_DEVICE_SUBDIR, INTEL_VENDOR_ID, SYSFS_VENDOR_FILE};
    use std::fs;
    use std::path::Path;

    /// Writes a fake sysfs tree under `base/drm` with the given cards.
    fn write_cards(base: &Path, cards: &[(&str, &str)]) {
        let drm = base.join("drm");
        for (name, vendor) in cards {
            let card = drm.join(name);
            fs::create_dir_all(card.join(DRM_DEVICE_SUBDIR)).unwrap();
            fs::write(
                card.join(DRM_DEVICE_SUBDIR).join(SYSFS_VENDOR_FILE),
                format!("{vendor}\n"),
            )
            .unwrap();
        }
    }

    /// The scan is non-destructive and must never panic without a sysfs dir.
    #[test]
    fn intel_detection_nunca_panica_sin_sysfs() {
        let out =
            std::panic::catch_unwind(|| has_intel_gpu_in("/ruta/que/no/existe-al-menos/aqui"));
        assert!(out.is_ok());
    }

    /// A directory without numeric card entries is not misreported as Intel.
    #[test]
    fn ignora_entradas_que_no_son_cards_numericas() {
        let base = tempfile::tempdir().unwrap();
        let drm = base.path().join("drm");
        fs::create_dir_all(&drm).unwrap();
        fs::write(drm.join("renderD128"), "nodo").unwrap();
        fs::write(drm.join("version"), "x").unwrap();
        assert!(!has_intel_gpu_in(drm.to_str().unwrap()));
    }

    /// Detects the Intel card and ignores AMD/NVIDIA ones.
    #[test]
    fn detecta_la_card_intel_y_descarta_las_dem_as() {
        let base = tempfile::tempdir().unwrap();
        write_cards(
            base.path(),
            &[
                ("card0", INTEL_VENDOR_ID),
                ("card1", "0x1002"),
                ("card2", "0x10de"),
            ],
        );
        assert!(has_intel_gpu_in(base.path().join("drm").to_str().unwrap()));
    }

    /// A sysfs tree with no Intel vendor is not reported as Intel.
    #[test]
    fn sin_card_intel_devuelve_falso() {
        let base = tempfile::tempdir().unwrap();
        write_cards(base.path(), &[("card0", "0x1002")]);
        assert!(!has_intel_gpu_in(base.path().join("drm").to_str().unwrap()));
    }
}
