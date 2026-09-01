//! Interfaz FFI mínima sobre libmpv para consultar propiedades en formato
//! nodo, que el crate `mpv` no expone (solo `bool`/`f64`/`i64`/`&str`).
//!
//! Se declaran únicamente los símbolos y estructuras necesarios para leer
//! `audio-device-list`: `mpv_get_property` con `MPV_FORMAT_NODE`, y el
//! recorrido del `mpv_node` resultante. Las definiciones ABI copian
//! exactamente `/usr/include/mpv/client.h` (API estable de libmpv).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int, c_void};

#[allow(non_camel_case_types)]
pub type mpv_handle = *mut c_void;
#[allow(non_camel_case_types)]
pub type mpv_format = c_int;

pub const MPV_FORMAT_NODE: mpv_format = 6;
pub const MPV_FORMAT_NODE_ARRAY: mpv_format = 7;
pub const MPV_FORMAT_NODE_MAP: mpv_format = 8;
pub const MPV_FORMAT_STRING: mpv_format = 1;

#[repr(C)]
pub struct mpv_node {
    pub u: mpv_node_union,
    pub format: mpv_format,
}

#[repr(C)]
pub union mpv_node_union {
    pub string: *mut c_char,
    pub flag: c_int,
    pub int64: i64,
    pub double_: c_double,
    pub list: *mut mpv_node_list,
    pub ba: *mut mpv_byte_array,
}

#[repr(C)]
pub struct mpv_node_list {
    pub num: c_int,
    pub values: *mut mpv_node,
    pub keys: *mut *mut c_char,
}

#[repr(C)]
pub struct mpv_byte_array {
    pub data: *mut c_void,
    pub size: usize,
}

extern "C" {
    fn mpv_get_property(
        ctx: *mut c_void,
        name: *const c_char,
        format: mpv_format,
        data: *mut c_void,
    ) -> c_int;

    fn mpv_free_node_contents(node: *mut mpv_node);
}

/// Lee la lista de dispositivos de audio de mpv (`audio-device-list`) y la
/// devuelve como parejas `(id, descripción)`.
///
/// Devuelve `Ok(None)` si la propiedad existe pero está vacía (sin
/// dispositivos); `Err(msg)` si no se pudo consultar.
pub fn audio_devices(handle: mpv_handle) -> Result<Option<Vec<(String, String)>>, String> {
    let name = CString::new("audio-device-list").expect("literal C a ASCII");

    let mut node = std::mem::MaybeUninit::<mpv_node>::zeroed();
    let rc = unsafe {
        mpv_get_property(
            handle,
            name.as_ptr(),
            MPV_FORMAT_NODE,
            node.as_mut_ptr().cast::<c_void>(),
        )
    };
    if rc < 0 {
        return Err(format!("mpv_get_property retornó {rc}"));
    }

    let mut node = unsafe { node.assume_init() };
    let mut devices = Vec::new();

    unsafe {
        if node.format != MPV_FORMAT_NODE_ARRAY {
            mpv_free_node_contents(&mut node);
            return Err(format!(
                "audio-device-list no es un array (formato {})",
                node.format
            ));
        }

        let list = node.u.list;
        let list_ref = &*list;
        for i in 0..list_ref.num {
            let value = &*list_ref.values.add(i as usize);
            if value.format != MPV_FORMAT_NODE_MAP {
                continue;
            }

            let mut id: Option<String> = None;
            let mut desc: Option<String> = None;
            let map = &*value.u.list;
            for k in 0..map.num {
                let key = CStr::from_ptr(*map.keys.add(k as usize));
                let key_str = key.to_string_lossy().into_owned();
                let val = &*map.values.add(k as usize);
                if val.format == MPV_FORMAT_STRING {
                    let value_str = CStr::from_ptr(val.u.string)
                        .to_string_lossy()
                        .into_owned();
                    match key_str.as_str() {
                        "name" => id = Some(value_str),
                        "description" => desc = Some(value_str),
                        _ => {}
                    }
                }
            }
            if let Some(id) = id {
                devices.push((id, desc.unwrap_or_default()));
            }
        }
        mpv_free_node_contents(&mut node);
    }

    Ok(if devices.is_empty() {
        None
    } else {
        Some(devices)
    })
}

// --- Render API (libmpv/render.h + render_gl.h) ----------------------------
//
// Permite embeber la salida de vídeo en superficies OpenGL propias (GTK4
// `gtk::GLArea`), en lugar de dejar que mpv abra su propia ventana
// (`--force-window`). Es el enfoque que usa Celluloid (vo=libmpv).

#[allow(non_camel_case_types)]
pub type mpv_render_context = c_void;

// mpv_render_param_type
pub const MPV_RENDER_PARAM_INVALID: c_int = 0;
pub const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
pub const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
pub const MPV_RENDER_PARAM_OPENGL_FBO: c_int = 3;
pub const MPV_RENDER_PARAM_FLIP_Y: c_int = 4;

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
    pub fn mpv_render_context_free(ctx: mpv_render_context_handle);
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
