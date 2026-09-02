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

/// Contexto pasado al update callback de libmpv. Empaqueta el widget y su
/// render context para que el callback (que corre en el hilo del motor) pueda,
/// en el hilo principal, llamar a `mpv_render_context_update` sobre el render
/// context correcto de **esta** instancia (cada ventana/espejo tiene el suyo).
#[repr(C)]
struct MpvUpdateCtx {
    /// La dirección del `GtkGLArea` (como `usize`, dato plano `Send`).
    widget: usize,
    /// El `mpv_render_context` de esta instancia.
    render_ctx: ffi::mpv_render_context_handle,
}

/// Callback (`mpv_render_context_set_update_callback`) que encola un repintado
/// del GLArea en el hilo principal. mpv lo invoca desde su propio hilo.
///
/// Regla de libmpv: desde el callback **no** se puede llamar a ninguna otra
/// función de mpv; por eso delegamos al hilo principal, donde sí hacemos
/// `mpv_render_context_update` (obligatorio para que el VO no se trabe y para
/// saber si hay frame nuevo) y encolamos el redibujado del GLArea.
unsafe extern "C" fn on_mpv_update(cb_ctx: *mut c_void) {
    // cb_ctx apunta al `MpvUpdateCtx` de esta instancia (leak intencionado,
    // se libera en `unrealize`). Se extraen los punteros como `usize` (Send).
    let widget = unsafe { (*(cb_ctx as *const MpvUpdateCtx)).widget };
    let render_addr = unsafe { (*(cb_ctx as *const MpvUpdateCtx)).render_ctx as usize };
    let main_ctx = glib::MainContext::default();
    main_ctx.invoke(move || {
        // Notifica a mpv que el frame fue recibido; sin esto el VO se traba y
        // nunca produce imagen (audio sí, vídeo no). Se llama sobre el render
        // context de ESTA instancia (una por ventana/espejo).
        let render_ctx = render_addr as ffi::mpv_render_context_handle;
        if !render_ctx.is_null() {
            let flags = unsafe { ffi::mpv_render_context_update(render_ctx) };
            if flags & ffi::MPV_RENDER_UPDATE_FRAME != 0 {
                unsafe {
                    let area = gtk::GLArea::from_glib_none(widget as *mut gtk::ffi::GtkGLArea);
                    area.queue_render();
                }
            }
        }
    });
}

/// Estado mutable compartido entre el GLArea y los callbacks.
struct State {
    render_ctx: ffi::mpv_render_context_handle,
    /// Puntero al `MpvUpdateCtx` de esta instancia (se libera en `unrealize`).
    update_ctx: *mut MpvUpdateCtx,
    /// Handle de mpv con el que crear el render context (per-instancia; en la
    /// ventana principal se rellena con el handle global del motor).
    handle: Option<ffi::mpv_handle>,
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
        // Con auto_render (true) GTK deja el GL context "current" alrededor del
        // signal `render`, limpia el buffer con el color de fondo y presenta el
        // resultado: el patrón que usa Celluloid. Nosotros dibujamos el frame de
        // mpv dentro de `render`.
        gl_area.set_auto_render(true);

        let state = Rc::new(RefCell::new(State {
            render_ctx: std::ptr::null_mut(),
            update_ctx: std::ptr::null_mut(),
            handle: None,
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

    /// Idéntico a [`EmbeddedVideo::new`], pero usa el handle de mpv dado
    /// (un core propio, p. ej. los espejos de monitores) en lugar del handle
    /// global del reproductor principal.
    pub fn with_handle(handle: ffi::mpv_handle) -> Self {
        let this = Self::new();
        this.state.borrow_mut().handle = Some(handle);
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
            let mut s = state.borrow_mut();
            s.render_ctx = std::ptr::null_mut();
        });

        let state = self.state.clone();
        gl_area.connect_unrealize(move |_| {
            let mut s = state.borrow_mut();
            // Libera el `MpvUpdateCtx` leakado y el render context.
            if !s.update_ctx.is_null() {
                unsafe { drop(Box::from_raw(s.update_ctx)) };
                s.update_ctx = std::ptr::null_mut();
            }
            if !s.render_ctx.is_null() {
                unsafe { ffi::mpv_render_context_free(s.render_ctx) };
                s.render_ctx = std::ptr::null_mut();
            }
        });

        let state = self.state.clone();
        let widget = self.gl_area.clone();
        gl_area.connect_render(move |area, gl_context| {
            // Diagnóstico: cada ~30 renders se anota en el log (evita spam).
            let n = RENDER_COUNT.fetch_add(1, Ordering::Relaxed);
            if n % 30 == 0 {
                let created = state.borrow().render_ctx.is_null() == false;
                let fbo = unsafe { ffi::gl_get_framebuffer_binding() };
                logging::info(format!(
                    "[embed] render n={n} ctx_creado={created} tamaño={}x{} fbo={fbo}",
                    area.width(),
                    area.height()
                ));
            }

            let mut s = state.borrow_mut();
            if s.render_ctx.is_null() {
                // Inicializa el render context la primera vez que dibujamos.
                // Requiere el GL context "current" (GTK lo deja así en render)
                // y un handle de core de mpv: el propio (espejos) o el global.
                let handle = match s.handle.or_else(mpv_handle) {
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
                    crate::reporting::report(
                        crate::reporting::ErrorKind::Player,
                        "No se pudo crear el contexto de render del vídeo",
                    );
                    return glib::Propagation::Proceed;
                }
                logging::info("[embed] render context de mpv creado (GLArea GL)");
                // Empaqueta el widget + este render context para el update
                // callback (así cada ventana/espejo usa el suyo). Se hace `leak`;
                // se libera en `unrealize`.
                let uctx = Box::into_raw(Box::new(MpvUpdateCtx {
                    widget: widget.as_ptr().cast::<c_void>() as usize,
                    render_ctx: s.render_ctx,
                }));
                s.update_ctx = uctx;
                unsafe {
                    ffi::mpv_render_context_set_update_callback(
                        s.render_ctx,
                        Some(on_mpv_update),
                        uctx.cast(),
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
        crate::reporting::report(
            crate::reporting::ErrorKind::Player,
            format!("No se pudo crear el contexto de render de mpv (código {rc})"),
        );
        return std::ptr::null_mut();
    }
    res
}

/// Dibuja el frame actual de mpv en el framebuffer del GLArea.
///
/// El FBO se lee de `GL_FRAMEBUFFER_BINDING` (el que GTK/GLArea haya enlazado;
/// puede no ser 0 en configuraciones con un framebuffer intermedio). `FLIP_Y`
/// es obligatorio porque GTK tiene el origen Y arriba-izquierda.
fn render_frame(ctx: ffi::mpv_render_context_handle, w: i32, h: i32) {
    // Debe ejecutarse con el GL context del GLArea ya "current".
    let fbo = unsafe { ffi::gl_get_framebuffer_binding() };
    let target = mpv_opengl_fbo {
        fbo,
        w,
        h,
        internal_format: 0,
    };
    let mut flip: c_int = 1;
    let mut params = [
        mpv_render_param {
            type_: ffi::MPV_RENDER_PARAM_OPENGL_FBO,
            data: (&target as *const mpv_opengl_fbo).cast::<c_void>() as *mut c_void,
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
        ffi::mpv_render_context_report_swap(ctx);
    }
}
