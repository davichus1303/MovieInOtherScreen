//! Selector de dispositivo de audio (PipeWire / PulseAudio) - UI layer.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::player::PlayerCommand;
use crate::mirror::MirrorController;
use mos_core::audio::{AudioDevice, AudioDevices};

/// Estado necesario para la sección de audio.
pub struct AudioDeps {
    pub player: std::sync::mpsc::Sender<PlayerCommand>,
    pub mirror: Rc<RefCell<MirrorController>>,
}

pub struct AudioSection {
    combo: gtk::DropDown,
    model: gtk::StringList,
    last_sent: Rc<RefCell<Option<String>>>,
    user_interacting: Rc<std::cell::Cell<bool>>,
    last_sent_cmd: Rc<RefCell<Option<String>>>,
}

impl AudioSection {
    pub fn new(deps: &AudioDeps) -> Self {
        let model = gtk::StringList::new(&["Cargando…"]);
        let combo = gtk::DropDown::new(
            Some(model.clone()),
            None::<&gtk::Expression>,
        );
        combo.set_halign(gtk::Align::Start);
        combo.set_hexpand(true);
        combo.set_sensitive(false);

        let last_sent = Rc::new(RefCell::new(None::<String>));
        let last_sent_cmd = Rc::new(RefCell::new(None::<String>));
        let user_interacting = Rc::new(std::cell::Cell::new(false));

        let section = Self {
            combo,
            model,
            last_sent,
            user_interacting,
            last_sent_cmd,
        };
        section.start_detection(deps);
        section
    }

    fn start_detection(&self, deps: &AudioDeps) {
        let model = self.model.clone();
        let combo = self.combo.clone();
        let last_sent = self.last_sent.clone();
        let player = deps.player.clone();
        let mirror = deps.mirror.clone();

        // Periodic detection
        glib::timeout_add_local(std::time::Duration::from_millis(5000), move || {
            // Real detection would happen here
            // For now, use mock devices
            let devices = vec![
                AudioDevice::new(
                    "alsa_output.pci-0000_00_1f.3.analog-stereo".into(),
                    "Speakers".into(),
                    true,
                ),
                AudioDevice::new(
                    "alsa_output.pci-0000_01_00.1.hdmi-stereo".into(),
                    "HDMI".into(),
                    false,
                ),
            ];
            
            // Update model
            let new_model = gtk::StringList::new(&[]);
            for device in &devices {
                new_model.append(&format!("{} ({})", device.label(), device.id()));
            }
            combo.set_model(Some(&new_model));
            
            // Set default selection
            if let Some(default) = devices.iter().find(|d| d.is_default()) {
                if let Some(idx) = devices.iter().position(|d| d.id() == default.id()) {
                    combo.set_selected(idx as u32);
                }
            }
            
            combo.set_sensitive(true);
            glib::ControlFlow::Continue
        });
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
