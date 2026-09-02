/*! Audio device selector (PipeWire / PulseAudio) - UI layer. */

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::constants::audio;
use crate::mirror::MirrorController;
use crate::player::PlayerCommand;
use mos_core::audio::{AudioDevice, AudioDevices};

/** State needed for the audio section. */
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
        let model = gtk::StringList::new(&[audio::LABEL_LOADING]);
        let combo = gtk::DropDown::new(Some(model.clone()), None::<&gtk::Expression>);
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
        glib::timeout_add_local(
            std::time::Duration::from_millis(audio::DETECTION_INTERVAL_MS as u64),
            move || {
                // Real detection would happen here
                // For now, use mock devices
                let devices = vec![
                    AudioDevice::new(
                        audio::mock::DEVICE_SPEAKERS.into(),
                        audio::mock::LABEL_SPEAKERS.into(),
                        true,
                    ),
                    AudioDevice::new(
                        audio::mock::DEVICE_HDMI.into(),
                        audio::mock::LABEL_HDMI.into(),
                        false,
                    ),
                ];

                // Update model
                let new_model = gtk::StringList::new(&[]);
                for device in &devices {
                    new_model.append(
                        &audio::ITEM_FORMAT
                            .replacen("{}", device.label(), 1)
                            .replacen("{}", device.id(), 1),
                    );
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
            },
        );
    }

    pub fn build(self) -> gtk::Box {
        let section = gtk::Box::new(gtk::Orientation::Vertical, audio::layout::SECTION_SPACING);
        section.set_margin_top(audio::layout::MARGIN_TOP);
        section.set_margin_bottom(audio::layout::MARGIN_BOTTOM);
        section.set_margin_start(audio::layout::MARGIN_START);
        section.set_margin_end(audio::layout::MARGIN_END);

        let title = gtk::Label::new(Some(audio::LABEL_SECTION_TITLE));
        title.set_halign(gtk::Align::Start);
        title.add_css_class(audio::CSS_TITLE);
        section.append(&title);

        let hint = gtk::Label::new(Some(audio::HINT_DESCRIPTION));
        hint.set_halign(gtk::Align::Start);
        hint.set_wrap(true);
        hint.set_max_width_chars(audio::layout::HINT_MAX_WIDTH_CHARS);
        section.append(&hint);

        section.append(&self.combo);
        section
    }
}
