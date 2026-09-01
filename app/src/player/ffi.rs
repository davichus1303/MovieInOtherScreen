//! Interfaz FFI mínima sobre libmpv para la renderización embebida.
//!
//! Declara los símbolos y estructuras de `libmpv/render.h` y
//! `libmpv/render_gl.h` que permiten dibujar la salida de vídeo en una
//! superficie OpenGL propia (GTK4 `gtk::GLArea`), en lugar de dejar que mpv
//! abra su propia ventana (`--force-window`). Es el enfoque que usa Celluloid
//! (`vo=libmpv`). Las definiciones ABI copian exactamente
//! `/usr/include/mpv/client.h` y `render_gl.h` (API estable de libmpv).

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

/// Bitflag de `mpv_render_context_update()`: un frame de vídeo nuevo está
/// disponible y hay que re-renderizar.
pub const MPV_RENDER_UPDATE_FRAME: u64 = 1;

/// Constante OpenGL `GL_FRAMEBUFFER_BINDING`.
pub const GL_FRAMEBUFFER_BINDING: c_int = 0x8CA6;

/// Cadena `char *` que identifica el render API OpenGL.
pub const MPV_RENDER_API_TYPE_OPENGL: &[u8] = b"opengl\0";

/// Inicialización del backend OpenGL (`mpv_render_gl.h`).
#[repr(C)]
pub struct mpv_opengl_init_params {
    pub get_proc_address:
        Option<unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char) -> *mut c_void>,
    pub get_proc_address_ctx: *mut c_void,
}

/// Objetivo de renderizado (`mpv_render_gl.h`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct mpv_opengl_fbo {
    pub fbo: c_int,
    pub w: c_int,
    pub h: c_int,
    pub internal_format: c_int,
}

/// Parámetro genérico de render.
#[repr(C)]
pub struct mpv_render_param {
    pub type_: c_int,
    pub data: *mut c_void,
}

/// Se llama cuando hay un frame nuevo que dibujar.
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
    /// Pide a mpv el estado pendiente de render (bits de `MPV_RENDER_UPDATE_*`).
    /// Debe llamarse al recibir el update callback, y es obligatorio cuando
    /// `ADVANCED_CONTROL` está activo; de lo contrario el VO se traba.
    pub fn mpv_render_context_update(ctx: mpv_render_context_handle) -> u64;
    /// Opcional; notifica a mpv que la cadena de presentación avanzó (vsync).
    pub fn mpv_render_context_report_swap(ctx: mpv_render_context_handle);
    pub fn mpv_render_context_free(ctx: mpv_render_context_handle);
}

/// Llama `glGetIntegerv` (resuelta vía el loader GL) para leer el framebuffer
/// actualmente enlazado por GTK/GLArea.
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
    /// Resuelve funciones de extensión y estándar desde el driver EGL.
    pub fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
    /// Resuelve funciones estándar/extensiones desde `libGL` (GLX loader).
    pub fn glXGetProcAddressARB(name: *const c_char) -> *mut c_void;
}

/// Resuelve el puntero de una función OpenGL para el callback que pide libmpv
/// (`mpv_opengl_init_params.get_proc_address`).
///
/// GTK4 no expone un lookup por nombre (eliminó `gdk_gl_context_get_proc_address`
/// de GTK3). En Wayland GTK4 usa un contexto EGL, así que se resuelve primero
/// con `eglGetProcAddress` y, si devuelve NULL, con `glXGetProcAddressARB`
/// (los dos exportados por `libEGL`/`libGL`).
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

    /// El resolver debe devolver punteros válidos para funciones GL estándar
    /// que libmpv consulta al crear el render context OpenGL.
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
