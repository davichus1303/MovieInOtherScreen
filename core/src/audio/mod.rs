/*! Audio device selection (pure domain logic). */

use std::sync::mpsc::Sender;

/** Audio output device. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    id: String,
    label: String,
    is_default: bool,
}

impl AudioDevice {
    pub fn new(id: String, label: String, is_default: bool) -> Self {
        Self {
            id,
            label,
            is_default,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn is_default(&self) -> bool {
        self.is_default
    }
}

/** Set of detected audio devices. */
#[derive(Debug, Default)]
pub struct AudioDevices {
    devices: Vec<AudioDevice>,
    preferred: Option<String>,
    active: Option<String>,
}

impl AudioDevices {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = &AudioDevice> {
        self.devices.iter()
    }

    pub fn get(&self, id: &str) -> Option<&AudioDevice> {
        self.devices.iter().find(|d| d.id() == id)
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    pub fn update_detected(&mut self, devices: Vec<AudioDevice>) {
        self.devices = devices;
        self.recompute_active();
    }

    pub fn select(&mut self, id: &str) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        self.preferred = Some(id.to_string());
        self.active = Some(id.to_string());
        true
    }

    pub fn active_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn preferred_id(&self) -> Option<&str> {
        self.preferred.as_deref()
    }

    pub fn active(&self) -> Option<&AudioDevice> {
        self.active.as_deref().and_then(|id| self.get(id))
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.active.as_deref() == Some(id)
    }

    pub fn preference_for_storage(&self) -> Option<String> {
        self.preferred.clone()
    }

    pub fn restore_preference(&mut self, id: Option<String>) {
        self.preferred = id;
        self.recompute_active();
    }

    fn recompute_active(&mut self) {
        if let Some(id) = &self.preferred {
            if self.get(id).is_some() {
                self.active = Some(id.clone());
                return;
            }
        }
        self.active = self
            .devices
            .iter()
            .find(|d| d.is_default())
            .map(|d| d.id().to_string());
    }
}

/** Commands for the audio selector (UI → Domain). */
#[derive(Debug, Clone)]
pub enum AudioCmd {
    Refresh,
    SelectDevice(String),
    SetPreferred(String),
}

/** Events from the audio selector (Domain → UI). */
#[derive(Debug, Clone)]
pub enum AudioEvent {
    DevicesUpdated(Vec<AudioDevice>),
    ActiveChanged(Option<String>),
    Error(String),
}

/** Trait for device detection (injectable for testing). */
pub trait AudioDetector: Send + Sync {
    fn detect(&self) -> Vec<AudioDevice>;
}

/** Pure audio selector (no UI). */
pub struct AudioSelector {
    devices: AudioDevices,
    detector: Box<dyn AudioDetector>,
    cmd_tx: Option<Sender<AudioCmd>>,
}

impl AudioSelector {
    pub fn new(detector: Box<dyn AudioDetector>) -> Self {
        Self {
            devices: AudioDevices::new(),
            detector,
            cmd_tx: None,
        }
    }

    pub fn set_command_sender(&mut self, tx: Sender<AudioCmd>) {
        self.cmd_tx = Some(tx);
    }

    pub fn refresh(&mut self) {
        let devices = self.detector.detect();
        self.devices.update_detected(devices);
    }

    pub fn select(&mut self, id: &str) -> bool {
        self.devices.select(id)
    }

    pub fn devices(&self) -> &AudioDevices {
        &self.devices
    }
}
