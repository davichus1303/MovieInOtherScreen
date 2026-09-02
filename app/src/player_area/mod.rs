/*! Main playback area: embedded video + controls + timeline. */

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;

use crate::constants::player_area;
use crate::player::PlayerCommand;

/** Timeline state (progress bar + time labels). */
#[derive(Debug)]
pub struct Timeline {
    duration: f64,
    bar: gtk::Scale,
    pos_label: gtk::Label,
    dur_label: gtk::Label,
}

impl Timeline {
    pub fn new() -> (gtk::Box, Rc<RefCell<Self>>) {
        let bar = gtk::Scale::with_range(
            gtk::Orientation::Horizontal,
            0.0,
            player_area::SCALE_MAX_PERCENT,
            player_area::SCALE_STEP,
        );
        bar.set_draw_value(false);
        bar.set_hexpand(true);

        let pos_label = gtk::Label::new(Some(player_area::TIME_LABEL_ZERO));
        pos_label.add_css_class(player_area::CSS_CAPTION);
        let dur_label = gtk::Label::new(Some(player_area::TIME_LABEL_ZERO));
        dur_label.add_css_class(player_area::CSS_CAPTION);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, player_area::layout::ROW_SPACING);
        row.append(&pos_label);
        row.append(&bar);
        row.append(&dur_label);

        let timeline = Self {
            duration: 0.0,
            bar: bar.clone(),
            pos_label,
            dur_label,
        };

        let timeline_rc = Rc::new(RefCell::new(timeline));
        (row, timeline_rc)
    }

    pub fn update_position(&self, pos: f64) {
        self.pos_label.set_label(&fmt_time(pos));
        let frac = if self.duration > 0.0 {
            (pos / self.duration).clamp(
                player_area::FRAC_CLAMP_MIN,
                player_area::FRAC_CLAMP_MAX,
            )
        } else {
            0.0
        };
        self.bar.set_value(frac * 100.0);
    }

    pub fn update_duration(&mut self, dur: f64) {
        self.duration = dur;
        self.dur_label.set_label(&fmt_time(dur));
    }

    pub fn seek_seconds(&self, value: f64) -> Option<f64> {
        if self.duration <= 0.0 {
            return None;
        }
        Some((value / player_area::SCALE_MAX_PERCENT) * self.duration)
    }

    /**
     * Connects the progress bar: clicking/dragging seeks to the point in the
     * main player and in the mirrors.
     */
    pub fn connect_seek(
        self_rc: &Rc<RefCell<Self>>,
        player: std::sync::mpsc::Sender<crate::player::PlayerCommand>,
        mirror: std::rc::Rc<std::cell::RefCell<crate::mirror::MirrorController>>,
    ) {
        let tl = self_rc.clone();
        let bar = self_rc.borrow().bar.clone();
        let send = player;
        bar.connect_change_value(move |_, _, value| {
            let seconds = match tl.borrow().seek_seconds(value) {
                Some(s) => s,
                None => return glib::Propagation::Proceed,
            };
            let _ = send.send(crate::player::PlayerCommand::Seek(seconds));
            mirror
                .borrow_mut()
                .control(crate::mirror::MirrorCmd::Seek(seconds));
            glib::Propagation::Proceed
        });
    }
}

/** Formats a duration in seconds as `mm:ss` (or `hh:mm:ss` when applicable). */
pub fn fmt_time(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return player_area::TIME_LABEL_ZERO.to_string();
    }
    let total = secs as u64;
    let h = total / player_area::SECS_PER_HOUR;
    let m = (total % player_area::SECS_PER_HOUR) / player_area::SECS_PER_MINUTE;
    let s = total % player_area::SECS_PER_MINUTE;
    if h > 0 {
        player_area::TIME_HMS_FORMAT
            .replace("{h:02}", &format!("{:02}", h))
            .replace("{m:02}", &format!("{:02}", m))
            .replace("{s:02}", &format!("{:02}", s))
    } else {
        player_area::TIME_MS_FORMAT
            .replace("{m:02}", &format!("{:02}", m))
            .replace("{s:02}", &format!("{:02}", s))
    }
}

/**
 * Builds the controls row (buttons) and synchronizes the mirrors with the
 * main player when they are pressed.
 */
pub fn build_controls(
    player: &std::sync::mpsc::Sender<crate::player::PlayerCommand>,
    mirror: std::rc::Rc<std::cell::RefCell<crate::mirror::MirrorController>>,
) -> gtk::Box {
    let controls = gtk::Box::new(
        gtk::Orientation::Horizontal,
        player_area::layout::CONTROLS_SPACING,
    );
    controls.set_halign(gtk::Align::Center);
    controls.set_margin_top(player_area::layout::CONTROLS_MARGIN_TOP);
    controls.set_margin_bottom(player_area::layout::CONTROLS_MARGIN_BOTTOM);

    for (label, command) in [
        (player_area::controls::ICON_SEEK_START, crate::player::PlayerCommand::Seek(0.0)),
        (player_area::controls::ICON_PLAY, crate::player::PlayerCommand::Play),
        (player_area::controls::ICON_PAUSE, crate::player::PlayerCommand::Pause),
        (player_area::controls::ICON_STOP, crate::player::PlayerCommand::Stop),
    ] {
        let send = player.clone();
        let mirror_state = mirror.clone();
        let button = gtk::Button::with_label(label);
        button.connect_clicked(move |_| {
            let _ = send.send(command.clone());
            // Stop detiene (pausa) y regresa al inicio en los espejos, igual
            // que en el reproductor principal.
            if command == crate::player::PlayerCommand::Stop {
                mirror_state
                    .borrow_mut()
                    .control(crate::mirror::MirrorCmd::Pause);
                mirror_state
                    .borrow_mut()
                    .control(crate::mirror::MirrorCmd::Seek(0.0));
            } else {
                let mirror_cmd = match command {
                    crate::player::PlayerCommand::Play => Some(crate::mirror::MirrorCmd::Play),
                    crate::player::PlayerCommand::Pause => Some(crate::mirror::MirrorCmd::Pause),
                    crate::player::PlayerCommand::Seek(pos) => {
                        Some(crate::mirror::MirrorCmd::Seek(pos))
                    }
                    _ => None,
                };
                if let Some(cmd) = mirror_cmd {
                    mirror_state.borrow_mut().control(cmd);
                }
            }
        });
        controls.append(&button);
    }

    controls
}
