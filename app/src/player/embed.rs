/*!
 * Embedding mpv video output in a `gtk::GLArea`.
 *
 * Instead of `--force-window` (mpv's own window), the engine uses
 * `vo=libmpv` and here an `mpv_render_context` (OpenGL render API) is created.
 * Each frame is painted into the widget through the `render` signal of
 * `GtkGLArea`. This is the same approach used by Celluloid (GNOME's mpv player).
 *
 * Thread rules (libmpv render.h):
 * - The render context is created on the UI thread, with the GL context
 *   "current" (GTK leaves it current when entering `render`).
 * - `mpv_render_context_render` is called only from the UI thread.
 * - The update callback is invoked by mpv from the engine thread; here it only
 *   enqueues a `queue_draw` on the main GLib context.
 */

use std::cell::RefCell;
use std::os::raw::{c_char, c_int, c_void};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use gtk::glib;
use gtk::glib::translate::*;
use gtk::prelude::*;

use crate::logging;
use crate::player::ffi::{self, mpv_opengl_fbo, mpv_opengl_init_params, mpv_render_param};
use crate::player::mpv_engine::mpv_handle;

/** Global render counter (for embedded video diagnostics). */
static RENDER_COUNT: AtomicU32 = AtomicU32::new(0);

/**
 * Context passed to the libmpv update callback. Wraps the widget and its
 * render context so the callback (running on the engine thread) can call
 * `mpv_render_context_update` on the correct render context of **this**
 * instance (each window/mirror has its own) from the main thread.
 *
 * The render context is stored as `AtomicUsize` and NOT as a copied value:
 * in `unrealize` it is set to `0` before being freed, so that any
 * late-arriving update callback (already enqueued on the main loop) reads
 * `null` and does not invoke `mpv_render_context_update` on already-freed
 * memory (prevents a use-after-free that caused the app to crash when
 * closing mirrors).
 */
struct MpvUpdateCtx {
    /** The `GtkGLArea` address (as `usize`, a plain `Send` value). */
    widget: usize,
    /** The `mpv_render_context` of this instance (viewed as an address). */
    render_ctx: AtomicUsize,
}

/**
 * Callback (`mpv_render_context_set_update_callback`) that enqueues a
 * repaint of the GLArea on the main thread. mpv invokes it from its own thread.
 *
 * libmpv rule: from the callback you **must not** call any other mpv
 * function; that is why we delegate to the main thread, where we do call
 * `mpv_render_context_update` (mandatory so the VO does not get stuck and to
 * check if there is a new frame) and enqueue the GLArea redraw.
 */
unsafe extern "C" fn on_mpv_update(cb_ctx: *mut c_void) {
    // cb_ctx apunta al `MpvUpdateCtx` de esta instancia (leak intencionado,
    // no se libera en `unrealize` para que este callback pueda seguir
    // dereferenciándolo con seguridad aunque llegue tarde).
    let widget = unsafe { (*(cb_ctx as *const MpvUpdateCtx)).widget };
    // Se clona el `Arc` no; el `AtomicUsize` vive dentro del `MpvUpdateCtx`
    // (leakeado), así que se captura la const referencia co. El valor del
    // render context se lee en tiempo de ejecución, no al encolar.
    let render_ctx = &(*(cb_ctx as *const MpvUpdateCtx)).render_ctx;
    let main_ctx = glib::MainContext::default();
    main_ctx.invoke(move || {
        let ctx = render_ctx.load(Ordering::SeqCst) as ffi::mpv_render_context_handle;
        // Si ya se cerró el espejo, `unrealize` puso este atómico a 0 y
        // liberó el render context: hay que saltarse la llamada.
        if !ctx.is_null() {
            let flags = unsafe { ffi::mpv_render_context_update(ctx) };
            if flags & ffi::MPV_RENDER_UPDATE_FRAME != 0 {
                unsafe {
                    let area = gtk::GLArea::from_glib_none(widget as *mut gtk::ffi::GtkGLArea);
                    area.queue_render();
                }
            }
        }
    });
}

/** Mutable state shared between the GLArea and the callbacks. */
struct State {
    render_ctx: ffi::mpv_render_context_handle,
    /**
     * Pointer to this instance's `MpvUpdateCtx` (intentionally leaked; in
     * `unrealize` only its atomic `render_ctx` is zeroed, the box is not freed).
     */
    update_ctx: *mut MpvUpdateCtx,
    /**
     * mpv handle used to create the render context (per-instance; in the
     * main window it is filled with the engine's global handle).
     */
    handle: Option<ffi::mpv_handle>,
    last_w: i32,
    last_h: i32,
    last_scale: f64,
}

/**
 * GL provider required by `mpv_opengl_init_params.get_proc_address`.
 *
 * GTK4 does not expose `gdk_gl_context_get_proc_address` (removed compared to
 * GTK3); it is resolved via `eglGetProcAddress` with fallback to
 * `glXGetProcAddress`. The GL context pointer is not used for the resolution.
 */
unsafe extern "C" fn gl_get_proc_address(_gl_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { ffi::resolve_gl_proc(name) }
}

/** GLArea widget embedded with an `mpv_render_context`. */
pub struct EmbeddedVideo {
    /** The controlled GLArea (keeps the widget and its pointer alive). */
    gl_area: gtk::GLArea,
    state: Rc<RefCell<State>>,
}

impl EmbeddedVideo {
    /** Builds the GLArea already connected to play mpv output. */
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

    /**
     * Identical to [`EmbeddedVideo::new`], but uses the given mpv handle
     * (an independent core, e.g. monitor mirrors) instead of the main
     * player's global handle.
     */
    pub fn with_handle(handle: ffi::mpv_handle) -> Self {
        let this = Self::new();
        this.state.borrow_mut().handle = Some(handle);
        this
    }

    /** Access to the widget for placing it in the interface. */
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
            // Primero se anula el render context en el `MpvUpdateCtx` compartido:
            // cualquier update callback ya encolado en el bucle principal verá
            // `null` y no tocará el contexto. El propio `MpvUpdateCtx` se deja
            // con `leak` (no se libera aquí) para que dichos callbacks puedan
            // seguir dereferenciándolo con seguridad al leer ese atómico.
            if !s.update_ctx.is_null() {
                let uctx = unsafe { s.update_ctx.as_ref().unwrap() };
                uctx.render_ctx.store(0, Ordering::SeqCst);
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
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(50),
                            move || {
                                w.queue_draw();
                            },
                        );
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
                // callback (así cada ventana/espejo usa el suyo). Se hace
                // `leak` a propósito: no se libera en `unrealize` para que los
                // callbacks encolados puedan seguir leyendo el atómico sin
                // dereferenciar memoria liberada.
                let uctx = Box::into_raw(Box::new(MpvUpdateCtx {
                    widget: widget.as_ptr().cast::<c_void>() as usize,
                    render_ctx: AtomicUsize::new(s.render_ctx as usize),
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

/** Creates the `mpv_render_context` for the given handle and GDK's GL context. */
fn init_render_context(
    handle: ffi::mpv_handle,
    gl_ctx: *mut c_void,
) -> ffi::mpv_render_context_handle {
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
    let rc = unsafe { ffi::mpv_render_context_create(&mut res as *mut _, handle, params.as_ptr()) };
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

/**
 * Draws the current mpv frame into the GLArea framebuffer.
 *
 * The FBO is read from `GL_FRAMEBUFFER_BINDING` (whichever GTK/GLArea has
 * bound; it may not be 0 in configurations with an intermediate framebuffer).
 * `FLIP_Y` is mandatory because GTK has its Y origin at the top-left.
 */
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
