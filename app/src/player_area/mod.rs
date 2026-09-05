/*! Main playback area: embedded video + controls + timeline. */

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;

use crate::constants::player_area;

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

        let row = gtk::Box::new(
            gtk::Orientation::Horizontal,
            player_area::layout::ROW_SPACING,
        );
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
            (pos / self.duration).clamp(player_area::FRAC_CLAMP_MIN, player_area::FRAC_CLAMP_MAX)
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
        (
            player_area::controls::ICON_SEEK_START,
            crate::player::PlayerCommand::Seek(0.0),
        ),
        (
            player_area::controls::ICON_PLAY,
            crate::player::PlayerCommand::Play,
        ),
        (
            player_area::controls::ICON_PAUSE,
            crate::player::PlayerCommand::Pause,
        ),
        (
            player_area::controls::ICON_STOP,
            crate::player::PlayerCommand::Stop,
        ),
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

    controls.append(&build_volume_control(player));

    controls
}

/** Mutable state of the volume control (kept only in the UI). */
struct VolumeUiState {
    /** Last (or current) volume; restored when unmuting. */
    volume: f64,
    muted: bool,
    /** True while setting the slider programmatically (ignore its signal). */
    suppress: bool,
}

/**
 * Builds the volume control: a mute button plus a compact slider.
 *
 * Behavior is YouTube-like but rendered in GNOME style (symbolic speaker icon
 * and an adwaita slider):
 * - Clicking the speaker toggles mute (the "previous" volume is kept).
 * - The icon reflects the level: muted / low / medium / high.
 * - Dragging the slider while muted unmutes at the new level.
 * - It drives mpv's software volume (0-100), independent of the system mixer
 *   and capped so it never exceeds the system limits.
 */
fn build_volume_control(
    player: &std::sync::mpsc::Sender<crate::player::PlayerCommand>,
) -> gtk::Box {
    use crate::constants::player_area::volume as vol;

    let widget = gtk::Box::new(
        gtk::Orientation::Horizontal,
        player_area::layout::CONTROLS_SPACING,
    );
    widget.set_halign(gtk::Align::Center);

    let state = Rc::new(RefCell::new(VolumeUiState {
        volume: vol::DEFAULT,
        muted: false,
        suppress: false,
    }));

    let mute_button = gtk::Button::new();
    mute_button.set_icon_name(vol::ICON_HIGH);
    mute_button.set_tooltip_text(Some(vol::TOOLTIP_MUTE_BUTTON));

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, vol::MIN, vol::MAX, vol::STEP);
    scale.set_draw_value(false);
    scale.set_valign(gtk::Align::Center);
    scale.set_width_request(vol::SCALE_WIDTH_REQUEST);
    scale.set_tooltip_text(Some(vol::TOOLTIP_SCALE));
    scale.set_value(vol::DEFAULT);

    // Mute toggle. While muted the slider shows 0 but the volume is kept.
    {
        let state = state.clone();
        let scale = scale.clone();
        let icon = mute_button.clone();
        let send = player.clone();
        mute_button.connect_clicked(move |_| {
            let mut s = state.borrow_mut();
            if s.muted {
                // Reactivate: restore the previous volume.
                s.muted = false;
                s.suppress = true;
                scale.set_value(s.volume);
                s.suppress = false;
                let _ = send.send(crate::player::PlayerCommand::Mute(false));
                let _ = send.send(crate::player::PlayerCommand::Volume(s.volume));
            } else {
                // Mute: the slider drops to 0 while the volume is kept.
                s.muted = true;
                s.suppress = true;
                scale.set_value(vol::MIN);
                s.suppress = false;
                let _ = send.send(crate::player::PlayerCommand::Mute(true));
            }
            set_volume_icon(&icon, scale.value(), s.muted);
        });
    }

    // Live volume; dragging while muted reactivates the sound.
    {
        let state = state.clone();
        let icon = mute_button.clone();
        let send = player.clone();
        scale.connect_value_changed(move |scale| {
            let mut s = state.borrow_mut();
            if s.suppress {
                return;
            }
            let v = scale.value().clamp(vol::MIN, vol::MAX);
            s.volume = v;
            if s.muted {
                if v > vol::MIN {
                    s.muted = false;
                    let _ = send.send(crate::player::PlayerCommand::Mute(false));
                } else {
                    set_volume_icon(&icon, v, true);
                    return;
                }
            }
            let _ = send.send(crate::player::PlayerCommand::Volume(v));
            set_volume_icon(&icon, v, s.muted);
        });
    }

    widget.append(&mute_button);
    widget.append(&scale);
    widget
}

/** Picks the speaker icon that matches the given volume and mute state. */
fn set_volume_icon(button: &gtk::Button, volume: f64, muted: bool) {
    use crate::constants::player_area::volume as vol;
    let icon = if muted || volume <= vol::MIN {
        vol::ICON_MUTED
    } else if volume < vol::ICON_LOW_THRESHOLD {
        vol::ICON_LOW
    } else if volume < vol::ICON_MEDIUM_THRESHOLD {
        vol::ICON_MEDIUM
    } else {
        vol::ICON_HIGH
    };
    button.set_icon_name(icon);
}
