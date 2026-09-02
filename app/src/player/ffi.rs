/*!
 * Minimal FFI interface over libmpv for embedded rendering.
 *
 * Declares the symbols and structures from `libmpv/render.h` and
 * `libmpv/render_gl.h` that allow drawing the video output on a
 * custom OpenGL surface (GTK4 `gtk::GLArea`), instead of letting mpv
 * open its own window (`--force-window`). This is the approach used by
 * Celluloid (`vo=libmpv`). The ABI definitions are an exact copy of
 * `/usr/include/mpv/client.h` and `render_gl.h` (libmpv stable API).
 */

use std::os::raw::{c_char, c_int, c_void};

#[allow(non_camel_case_types)]
pub type mpv_handle = *mut c_void;

// --- Render API (libmpv/render.h + render_gl.h) ----------------------------

#[allow(non_camel_case_types)]
pub type mpv_render_context = c_void;

// mpv_render_param_type
pub const MPV_RENDER_PARAM_INVALID: c_int = 0;
pub const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
pub const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
pub const MPV_RENDER_PARAM_OPENGL_FBO: c_int = 3;
pub const MPV_RENDER_PARAM_FLIP_Y: c_int = 4;

/**
 * Bitflag from `mpv_render_context_update()`: a new video frame is
 * available and must be re-rendered.
 */
pub const MPV_RENDER_UPDATE_FRAME: u64 = 1;

/** OpenGL constant `GL_FRAMEBUFFER_BINDING`. */
pub const GL_FRAMEBUFFER_BINDING: c_int = 0x8CA6;

/** `char *` string that identifies the OpenGL render API. */
pub const MPV_RENDER_API_TYPE_OPENGL: &[u8] = b"opengl\0";

/** OpenGL backend initialization (`mpv_render_gl.h`). */
#[repr(C)]
pub struct mpv_opengl_init_params {
    pub get_proc_address:
        Option<unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char) -> *mut c_void>,
    pub get_proc_address_ctx: *mut c_void,
}

/** Render target (`mpv_render_gl.h`). */
#[repr(C)]
#[derive(Clone, Copy)]
pub struct mpv_opengl_fbo {
    pub fbo: c_int,
    pub w: c_int,
    pub h: c_int,
    pub internal_format: c_int,
}

/** Generic render parameter. */
#[repr(C)]
pub struct mpv_render_param {
    pub type_: c_int,
    pub data: *mut c_void,
}

/** Called when there is a new frame to draw. */
#[allow(non_camel_case_types)]
pub type mpv_render_update_fn = Option<unsafe extern "C" fn(cb_ctx: *mut c_void)>;

#[allow(non_camel_case_types)]
pub type mpv_render_context_handle = *mut mpv_render_context;

extern "C" {
    pub fn mpv_render_context_create(
        res: *mut mpv_render_context_handle,
        mpv: *mut c_void,
        params: *const mpv_render_param,
    ) -> c_int;
    pub fn mpv_render_context_render(
        ctx: mpv_render_context_handle,
        params: *mut mpv_render_param,
    ) -> c_int;
    pub fn mpv_render_context_set_update_callback(
        ctx: mpv_render_context_handle,
        callback: mpv_render_update_fn,
        callback_ctx: *mut c_void,
    );
    /**
     * Asks mpv for the pending render state (`MPV_RENDER_UPDATE_*` bits).
     * Must be called upon receiving the update callback, and is mandatory when
     * `ADVANCED_CONTROL` is active; otherwise the VO gets stuck.
     */
    pub fn mpv_render_context_update(ctx: mpv_render_context_handle) -> u64;
    /** Optional; notifies mpv that the presentation chain advanced (vsync). */
    pub fn mpv_render_context_report_swap(ctx: mpv_render_context_handle);
    pub fn mpv_render_context_free(ctx: mpv_render_context_handle);
}

/**
 * Calls `glGetIntegerv` (resolved via the GL loader) to read the framebuffer
 * currently bound by GTK/GLArea.
 */
pub unsafe fn gl_get_framebuffer_binding() -> c_int {
    let mut fbo: c_int = 0;
    unsafe {
        let get = resolve_gl_proc(b"glGetIntegerv\0".as_ptr() as *const c_char);
        if !get.is_null() {
            type GlGetIntegerv = unsafe extern "C" fn(pname: c_int, data: *mut c_int);
            let f: GlGetIntegerv = std::mem::transmute(get);
            f(GL_FRAMEBUFFER_BINDING, &mut fbo);
        }
    }
    fbo
}

#[link(name = "EGL")]
#[link(name = "GL")]
extern "C" {
    /** Resolves extension and standard functions from the EGL driver. */
    pub fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
    /** Resolves standard functions/extensions from `libGL` (GLX loader). */
    pub fn glXGetProcAddressARB(name: *const c_char) -> *mut c_void;
}

/**
 * `mpv_format` used for reading/writing floating-point properties.
 *
 * Matches libmpv's `mpv_format`: `MPV_FORMAT_DOUBLE = 5`
 * (`MPV_FORMAT_INT64 = 4` is a different value; using it here would read the
 * property as an integer and return garbage when reinterpreted as `f64`).
 */
pub const MPV_FORMAT_DOUBLE: c_int = 5;

extern "C" {
    /**
     * Reads a property in double format (`MPV_FORMAT_DOUBLE`). Returns
     * `>= 0` on success. Safe to read from any thread.
     */
    pub fn mpv_get_property(
        ctx: super::ffi::mpv_handle,
        name: *const c_char,
        format: c_int,
        data: *mut c_void,
    ) -> c_int;
}

/**
 * Resolves an OpenGL function pointer for the callback required by libmpv
 * (`mpv_opengl_init_params.get_proc_address`).
 *
 * GTK4 does not expose a name-based lookup (it removed
 * `gdk_gl_context_get_proc_address` from GTK3). On Wayland GTK4 uses an EGL
 * context, so resolution first tries `eglGetProcAddress` and, if it returns
 * NULL, falls back to `glXGetProcAddressARB` (both exported by
 * `libEGL`/`libGL`).
 */
pub unsafe fn resolve_gl_proc(name: *const c_char) -> *mut c_void {
    let mut ptr = eglGetProcAddress(name);
    if ptr.is_null() {
        ptr = glXGetProcAddressARB(name);
    }
    ptr
}

#[cfg(test)]
mod tests {
    use super::resolve_gl_proc;
    use std::ffi::CString;
    use std::os::raw::{c_char, c_void};

    fn resolve(name: &str) -> *mut c_void {
        let c = CString::new(name).unwrap();
        unsafe { resolve_gl_proc(c.as_ptr() as *const c_char) }
    }

    /**
     * The resolver must return valid pointers for standard GL functions
     * that libmpv queries when creating the OpenGL render context.
     */
    #[test]
    fn resuelve_funciones_gl_estandar() {
        for name in ["glClear", "glGetString", "glTexImage2D", "glCreateShader"] {
            let ptr = resolve(name);
            assert!(
                !ptr.is_null(),
                "no se resolvió la función estándar {name} vía EGL/GLX"
            );
        }
    }
}
