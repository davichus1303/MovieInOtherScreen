//! Embebido de la salida de vídeo de mpv en un `gtk::GLArea`.
//!
//! En lugar de `--force-window` (ventana propia de mpv), el motor usa
//! `vo=libmpv` y aquí se crea un `mpv_render_context` (render API OpenGL) y,
//! mediante el señal `render` de `GtkGLArea`, se pinta cada frame en el
//! widget. Es el mismo enfoque que usa Celluloid (el reproductor mpv de GNOME).
//!
//! Reglas de hilos (libmpv render.h):
//! - El render context se crea en el hilo de la UI, con el GL context
//!   "current" (GTK lo deja current al entrar en `render`).
//! - `mpv_render_context_render` se llama solo desde el hilo de la UI.
//! - El update callback lo invoca mpv desde el hilo del motor; aquí solo se
//!   encola un `queue_draw` en el contexto principal de GLib.

use std::cell::RefCell;
use std::os::raw::{c_char, c_int, c_void};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use gtk::glib;
use gtk::glib::translate::*;
use gtk::prelude::*;

use crate::logging;
use crate::player::ffi::{self, mpv_opengl_fbo, mpv_opengl_init_params, mpv_render_param};
use crate::player::mpv_engine::mpv_handle;

/// Contador global de renders (para diagnóstico del video embebido).
static RENDER_COUNT: AtomicU32 = AtomicU32::new(0);

/// Callback (`mpv_render_context_set_update_callback`) que encola un repintado
/// del GLArea en el hilo principal. mpv lo invoca desde su propio hilo.
unsafe extern "C" fn on_mpv_update(cb_ctx: *mut c_void) {
    // El widget sigue vivo mientras el render context exista; se propaga su
    // dirección como `usize` (Send) para poder usarla en el hilo principal.
    let area_addr = cb_ctx as usize;
    let main_ctx = glib::MainContext::default();
    main_ctx.invoke(move || {
        unsafe {
            let area = gtk::GLArea::from_glib_none(area_addr as *mut gtk::ffi::GtkGLArea);
            area.queue_draw();
        }
    });
}

/// Estado mutable compartido entre el GLArea y los callbacks.
struct State {
    render_ctx: ffi::mpv_render_context_handle,
    last_w: i32,
    last_h: i32,
    last_scale: f64,
}

/// Proveedor GL requerido por `mpv_opengl_init_params.get_proc_address`.
///
/// GTK4 no expone `gdk_gl_context_get_proc_address` (eliminado respecto a
/// GTK3); se resuelve vía `eglGetProcAddress` con fallback a
/// `glXGetProcAddress`. El puntero del contexto GL no se usa en la resolución.
unsafe extern "C" fn gl_get_proc_address(_gl_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { ffi::resolve_gl_proc(name) }
}

/// Widget GLArea embechado con un `mpv_render_context`.
pub struct EmbeddedVideo {
    /// El GLArea bajo control (mantiene vivo el widget y su puntero).
    gl_area: gtk::GLArea,
    state: Rc<RefCell<State>>,
}

impl EmbeddedVideo {
    /// Construye el GLArea ya conectado para reproducir la salida de mpv.
    pub fn new() -> Self {
        let gl_area = gtk::GLArea::new();
        // El dibujado lo controlamos nosotros en `render`.
        gl_area.set_auto_render(false);

        let state = Rc::new(RefCell::new(State {
            render_ctx: std::ptr::null_mut(),
            last_w: 0,
            last_h: 0,
            last_scale: 1.0,
        }));

        let this = Self {
            gl_area: gl_area.clone(),
            state,
        };
        this.connect();
        this
    }

    /// Acceso al widget para colocarlo en la interfaz.
    pub fn widget(&self) -> &gtk::GLArea {
        &self.gl_area
    }

    fn connect(&self) {
        let gl_area = self.gl_area.clone();

        // En `realize` se limpia cualquier contexto previo (la UI no se
        // despinta al re-realizar). El render context real se crea en `render`,
        // cuando el GL context ya es "current".
        let state = self.state.clone();
        gl_area.connect_realize(move |area| {
            if let Some(ctx) = area.context() {
                ctx.make_current();
            }
            state.borrow_mut().render_ctx = std::ptr::null_mut();
        });

        let state = self.state.clone();
        gl_area.connect_unrealize(move |_| {
            let mut s = state.borrow_mut();
            if !s.render_ctx.is_null() {
                unsafe { ffi::mpv_render_context_free(s.render_ctx) };
                s.render_ctx = std::ptr::null_mut();
            }
        });

        let state = self.state.clone();
        let widget = self.gl_area.clone();
        gl_area.connect_render(move |area, gl_context| {
            // Diagnóstico: cada ~90 renders se anota en el log (evita spam).
            let n = RENDER_COUNT.fetch_add(1, Ordering::Relaxed);
            if n % 90 == 0 {
                let created = state.borrow().render_ctx.is_null() == false;
                logging::info(format!(
                    "[embed] render n={n} ctx_creado={created} tamaño={}x{}",
                    area.width(),
                    area.height()
                ));
            }

            let mut s = state.borrow_mut();
            if s.render_ctx.is_null() {
                // Inicializa el render context la primera vez que dibujamos.
                // Requiere el GL context "current" (GTK lo deja así en render)
                // y el handle del core de mpv ya expuesto por el motor.
                let handle = match mpv_handle() {
                    Some(h) => h,
                    None => {
                        // El motor aún no expone el handle (p. ej. el primer
                        // repintado ocurre antes de arrancar mpv). Se reintenta
                        // en unos ms para no quedarse sin contexto de render.
                        eprintln!("[embed] motor mpv aún no listo; reintentando");
                        logging::warn("Render embebido sin handle de mpv; reintentando");
                        let w = widget.clone();
                        glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
                            w.queue_draw();
                        });
                        return glib::Propagation::Proceed;
                    }
                };
                let gl_ptr = gl_context.as_ptr() as *mut c_void;
                s.render_ctx = init_render_context(handle, gl_ptr);
                if s.render_ctx.is_null() {
                    eprintln!("[embed] no se pudo crear el render context");
                    logging::error("No se pudo crear el contexto de render de mpv (GLArea)");
                    return glib::Propagation::Proceed;
                }
                logging::info("[embed] render context de mpv creado (GLArea GL)");
                unsafe {
                    ffi::mpv_render_context_set_update_callback(
                        s.render_ctx,
                        Some(on_mpv_update),
                        widget.as_ptr().cast(),
                    );
                }
            }

            // Tamaño del área en píxeles físicos (incluye factor de escala).
            let scale = area.scale_factor();
            let factor: f64 = scale as f64;
            let w = (area.width() as f64 * factor) as i32;
            let h = (area.height() as f64 * factor) as i32;
            if w > 0 && h > 0 {
                s.last_w = w;
                s.last_h = h;
                s.last_scale = factor;
                render_frame(s.render_ctx, w, h);
            }
            glib::Propagation::Proceed
        });
    }
}

/// Crea el `mpv_render_context` para el handle dado y el GL context de GDK.
fn init_render_context(handle: ffi::mpv_handle, gl_ctx: *mut c_void) -> ffi::mpv_render_context_handle {
    let api_type = ffi::MPV_RENDER_API_TYPE_OPENGL;
    let mut init_params = mpv_opengl_init_params {
        get_proc_address: Some(gl_get_proc_address),
        get_proc_address_ctx: gl_ctx,
    };

    let params = [
        mpv_render_param {
            type_: ffi::MPV_RENDER_PARAM_API_TYPE,
            data: api_type.as_ptr().cast::<c_void>() as *mut c_void,
        },
        mpv_render_param {
            type_: ffi::MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
            data: (&mut init_params as *mut mpv_opengl_init_params).cast::<c_void>(),
        },
        mpv_render_param {
            type_: ffi::MPV_RENDER_PARAM_INVALID,
            data: std::ptr::null_mut(),
        },
    ];

    let mut res: ffi::mpv_render_context_handle = std::ptr::null_mut();
    let rc = unsafe {
        ffi::mpv_render_context_create(&mut res as *mut _, handle, params.as_ptr())
    };
    if rc < 0 || res.is_null() {
        eprintln!("[embed] mpv_render_context_create rc={rc}");
        logging::error(format!("mpv_render_context_create falló con código {rc}"));
        return std::ptr::null_mut();
    }
    res
}

/// Dibuja el frame actual de mpv en el framebuffer por defecto del GLArea.
fn render_frame(ctx: ffi::mpv_render_context_handle, w: i32, h: i32) {
    let fbo = mpv_opengl_fbo {
        fbo: 0,
        w,
        h,
        internal_format: 0,
    };
    let mut flip: c_int = 1; // GL default framebuffer está invertido
    let mut params = [
        mpv_render_param {
            type_: ffi::MPV_RENDER_PARAM_OPENGL_FBO,
            data: (&fbo as *const mpv_opengl_fbo).cast::<c_void>() as *mut c_void,
        },
        mpv_render_param {
            type_: ffi::MPV_RENDER_PARAM_FLIP_Y,
            data: (&mut flip as *mut c_int).cast::<c_void>(),
        },
        mpv_render_param {
            type_: ffi::MPV_RENDER_PARAM_INVALID,
            data: std::ptr::null_mut(),
        },
    ];
    unsafe {
        let rc = ffi::mpv_render_context_render(ctx, params.as_mut_ptr());
        if rc < 0 {
            eprintln!("[embed] mpv_render_context_render rc={rc}");
            logging::error(format!("mpv_render_context_render falló con código {rc}"));
        }
    }
}
