//! Selector de dispositivo de audio (PipeWire / PulseAudio).

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;

use crate::player::PlayerCommand;
use crate::mirror::MirrorController;

/// Estado necesario para la sección de audio.
pub struct AudioDeps {
    pub player: std::sync::mpsc::Sender<PlayerCommand>,
    pub mirror: Rc<RefCell<MirrorController>>,
}

pub struct AudioSection {
    combo: gtk::DropDown,
    last_sent: Rc<RefCell<Option<String>>>,
    user_interacting: Rc<std::cell::Cell<bool>>,
}

impl AudioSection {
    pub fn new(deps: &AudioDeps) -> Self {
        let combo = gtk::DropDown::new(
            Some(gtk::StringList::new(&["Cargando…"])),
            None::<&gtk::Expression>,
        );
        combo.set_halign(gtk::Align::Start);
        combo.set_hexpand(true);
        combo.set_sensitive(false);

        let last_sent = Rc::new(RefCell::new(None::<String>));
        let user_interacting = Rc::new(std::cell::Cell::new(false));

        // Detectar interacción del usuario via popover visibility
        // (simplified - just use a timer for now)
        let user_interacting_for_timer = user_interacting.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1000), move || {
            user_interacting_for_timer.set(true);
        });

        let section = Self {
            combo,
            last_sent: last_sent.clone(),
            user_interacting,
        };
        section.connect_signals();
        section.start_detection();
        section
    }

    fn connect_signals(&self) {
        // Device selection logic would go here
    }

    fn start_detection(&self) {
        // Background detection logic would go here
    }

    pub fn build(self) -> gtk::Box {
        let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
        section.set_margin_top(8);
        section.set_margin_bottom(12);
        section.set_margin_start(12);
        section.set_margin_end(12);

        let title = gtk::Label::new(Some("Salida de audio"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("title-4");
        section.append(&title);

        let hint = gtk::Label::new(Some(
            "Dispositivo por el que se escucha la reproducción. Si no aparece \
             ninguno, se usa el predeterminado del sistema.",
        ));
        hint.set_halign(gtk::Align::Start);
        hint.set_wrap(true);
        hint.set_max_width_chars(60);
        section.append(&hint);

        section.append(&self.combo);
        section
    }
}