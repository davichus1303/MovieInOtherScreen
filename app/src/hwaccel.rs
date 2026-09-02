/*
 * Hardware acceleration of video decoding, inspired by VLC's approach.
 *
 * VLC probes the available GPU (dedicated or integrated into the processor)
 * and, only if one exists, enables the hardware decoder for the video stream,
 * regardless of the codec. This replicates that behavior on the mpv engine:
 * this module is independent of the rest of the playback logic and is applied
 * to all mpv instances (player and mirrors) alike.
 */

/**
 * Probes whether a GPU is available, either dedicated or integrated into the
 * processor (iGPU).
 *
 * It checks for the presence of any DRM render node
 * (`/dev/dri/renderD*`), which both dedicated cards (AMD, NVIDIA) and
 * integrated ones (Intel/AMD iGPU) expose when the driver is loaded. It is the
 *  same mechanism libva/VA-API uses to locate the decoding device.
 */
use crate::constants::hwaccel::{
    DRM_DEV_DIR, DRM_RENDER_NODE_PREFIX, OPT_HWDEC, OPT_HWDEC_AUTO, OPT_HWDEC_NO,
};

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
 * Enables or disables hardware acceleration on an mpv `builder`.
 *
 * If there is a GPU (dedicated or integrated) `hwdec=auto` is enabled, which
 * auto-detects the backend (VA-API, VDPAU, NVDEC, ...) and applies it to any
 * codec without the format requiring it. If there is no GPU, `hwdec=no` is
 * forced to avoid costly fallbacks in the decoder.
 */
pub fn apply_to(builder: &mut mpv::MpvHandlerBuilder) -> mpv::Result<()> {
    if has_gpu() {
        builder.set_option(OPT_HWDEC, OPT_HWDEC_AUTO)
    } else {
        builder.set_option(OPT_HWDEC, OPT_HWDEC_NO)
    }
}
