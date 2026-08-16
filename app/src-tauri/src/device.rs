//! Thin wrapper over `mirajazz` for talking to the AJAZZ AKP03 family (and
//! rebranded Mirabox/Soomfon/etc. hardware sharing the same firmware/protocol).
//!
//! VID/PID table and protocol details sourced from the reference OpenDeck
//! plugin (github.com/4ndv/opendeck-akp03), confirmed against mirajazz
//! 0.16.2's actual API on 2026-08-13. Verified against a real AKP03E
//! across several rounds of hardware testing (image push, button events,
//! unplug/replug) — see ROADMAP.md for the history. Other Kind variants
//! remain untested (best-effort, per SPEC.md §4).

use anyhow::{anyhow, Result};
use futures_lite::StreamExt;
use image::{DynamicImage, Rgb, RgbImage};
use mirajazz::{
    device::{list_devices, Device, DeviceQuery, DeviceWatcher},
    error::MirajazzError,
    types::{DeviceInput, HidDeviceInfo, ImageFormat, ImageMirroring, ImageMode, ImageRotation},
};
use serde::Serialize;

/// Total physical buttons (6 with an LCD screen + 3 plain push-buttons),
/// needed by `Device::connect` for correctly sized button-state tracking.
pub const KEY_COUNT: usize = 9;
/// Of those, only the first 6 (indices 0-5) have a screen and can show an
/// image — confirmed against a real AKP03E. Buttons 6/7/8 are plain
/// push-buttons; pushing an image to them is meaningless, so anything
/// that assigns metrics to buttons should stop at this count, not
/// `KEY_COUNT`.
pub const SCREEN_KEY_COUNT: usize = 6;
pub const ENCODER_COUNT: usize = 3;

const AJAZZ_VID: u16 = 0x0300;
const MIRABOX_6602_VID: u16 = 0x6602;
const MIRABOX_6603_VID: u16 = 0x6603;

const C_1001_PID: u16 = 0x1001;
const C_1002_PID: u16 = 0x1002;
const C_1003_PID: u16 = 0x1003;
const C_1000_PID: u16 = 0x1000;
const C_3002_PID: u16 = 0x3002;
const C_3003_PID: u16 = 0x3003;

/// Known device variants sharing this protocol family. AKP03E is the
/// primary/tested target; the rest are best-effort per SPEC.md §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Akp03,
    Akp03E,
    Akp03R,
    Akp03ERev2,
    Akp03RRev2,
    MiraboxN36602_1000,
    MiraboxN36602_1002,
    MiraboxN36603_1002,
    MiraboxN36603_1003,
}

impl Kind {
    pub fn from_vid_pid(vid: u16, pid: u16) -> Option<Self> {
        match (vid, pid) {
            (AJAZZ_VID, C_1001_PID) => Some(Kind::Akp03),
            (AJAZZ_VID, C_1002_PID) => Some(Kind::Akp03E),
            (AJAZZ_VID, C_1003_PID) => Some(Kind::Akp03R),
            (AJAZZ_VID, C_3002_PID) => Some(Kind::Akp03ERev2),
            (AJAZZ_VID, C_3003_PID) => Some(Kind::Akp03RRev2),
            (MIRABOX_6602_VID, C_1000_PID) => Some(Kind::MiraboxN36602_1000),
            (MIRABOX_6602_VID, C_1002_PID) => Some(Kind::MiraboxN36602_1002),
            (MIRABOX_6603_VID, C_1002_PID) => Some(Kind::MiraboxN36603_1002),
            (MIRABOX_6603_VID, C_1003_PID) => Some(Kind::MiraboxN36603_1003),
            _ => None,
        }
    }

    pub fn human_name(&self) -> &'static str {
        match self {
            Kind::Akp03 => "Ajazz AKP03",
            Kind::Akp03E => "Ajazz AKP03E",
            Kind::Akp03R => "Ajazz AKP03R",
            Kind::Akp03ERev2 => "Ajazz AKP03E (rev. 2)",
            Kind::Akp03RRev2 => "Ajazz AKP03R (rev. 2)",
            Kind::MiraboxN36602_1000 => "Mirabox N3 (6602:1000)",
            Kind::MiraboxN36602_1002 => "Mirabox N3 (6602:1002)",
            Kind::MiraboxN36603_1002 => "Mirabox N3 (6603:1002)",
            Kind::MiraboxN36603_1003 => "Mirabox N3 (6603:1003)",
        }
    }

    pub fn protocol_version(&self) -> usize {
        match self {
            Kind::MiraboxN36603_1002 | Kind::MiraboxN36603_1003 => 3,
            Kind::Akp03ERev2 | Kind::Akp03RRev2 => 3,
            _ => 2,
        }
    }

    pub fn image_format(&self) -> ImageFormat {
        if self.protocol_version() == 3 {
            ImageFormat {
                mode: ImageMode::JPEG,
                size: (64, 64),
                rotation: ImageRotation::Rot90,
                mirror: ImageMirroring::None,
            }
        } else {
            ImageFormat {
                mode: ImageMode::JPEG,
                size: (60, 60),
                rotation: ImageRotation::Rot0,
                mirror: ImageMirroring::None,
            }
        }
    }
}

const QUERIES: [DeviceQuery; 9] = [
    DeviceQuery::new(65440, 1, AJAZZ_VID, C_1001_PID),
    DeviceQuery::new(65440, 1, AJAZZ_VID, C_1002_PID),
    DeviceQuery::new(65440, 1, AJAZZ_VID, C_1003_PID),
    DeviceQuery::new(65440, 1, AJAZZ_VID, C_3002_PID),
    DeviceQuery::new(65440, 1, AJAZZ_VID, C_3003_PID),
    DeviceQuery::new(65440, 1, MIRABOX_6602_VID, C_1000_PID),
    DeviceQuery::new(65440, 1, MIRABOX_6602_VID, C_1002_PID),
    DeviceQuery::new(65440, 1, MIRABOX_6603_VID, C_1002_PID),
    DeviceQuery::new(65440, 1, MIRABOX_6603_VID, C_1003_PID),
];

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredDevice {
    pub name: String,
    pub vid: u16,
    pub pid: u16,
}

/// Lists currently connected devices matching a known Kind. Devices with
/// unrecognized VID/PID pairs are silently skipped.
pub async fn discover() -> Result<Vec<DiscoveredDevice>> {
    let found = list_devices(&QUERIES)
        .await
        .map_err(|e| anyhow!("device enumeration failed: {e}"))?;

    Ok(found
        .into_iter()
        .filter_map(|dev| {
            let kind = Kind::from_vid_pid(dev.vendor_id, dev.product_id)?;
            Some(DiscoveredDevice {
                name: kind.human_name().to_string(),
                vid: dev.vendor_id,
                pid: dev.product_id,
            })
        })
        .collect())
}

pub async fn connect(dev: &HidDeviceInfo) -> Result<(Device, Kind)> {
    let kind = Kind::from_vid_pid(dev.vendor_id, dev.product_id)
        .ok_or_else(|| anyhow!("unrecognized device VID/PID"))?;

    let device = Device::connect(dev, kind.protocol_version(), KEY_COUNT, ENCODER_COUNT)
        .await
        .map_err(|e| anyhow!("failed to connect: {e}"))?;

    Ok((device, kind))
}

/// Finds the first connected device with a recognized VID/PID and connects
/// to it. Used both by the persistent background connection and the manual
/// spike commands below.
pub async fn connect_first() -> Result<(Device, Kind)> {
    let found = list_devices(&QUERIES)
        .await
        .map_err(|e| anyhow!("device enumeration failed: {e}"))?;

    let dev = found
        .into_iter()
        .find(|d| Kind::from_vid_pid(d.vendor_id, d.product_id).is_some())
        .ok_or_else(|| anyhow!("no supported device connected"))?;

    connect(&dev).await
}

/// Renders and pushes an image to a single button, then flushes.
pub async fn push_image(
    device: &Device,
    kind: Kind,
    key: u8,
    image: DynamicImage,
) -> Result<()> {
    device
        .set_button_image(key, kind.image_format(), image)
        .await
        .map_err(|e| anyhow!("failed to push image to button {key}: {e}"))?;

    device
        .flush()
        .await
        .map_err(|e| anyhow!("failed to flush: {e}"))?;

    Ok(())
}

/// Phase 0/1 hardware spike, wired as a real command: connects to the first
/// supported device found and pushes a solid-color test image to button 0.
pub async fn push_test_pattern() -> Result<String> {
    let (device, kind) = connect_first().await?;

    let format = kind.image_format();
    let (w, h) = format.size;
    let mut img = RgbImage::new(w as u32, h as u32);
    for pixel in img.pixels_mut() {
        *pixel = Rgb([0, 200, 100]);
    }

    device
        .set_button_image(0, format, DynamicImage::ImageRgb8(img))
        .await
        .map_err(|e| anyhow!("failed to push image: {e}"))?;

    device
        .flush()
        .await
        .map_err(|e| anyhow!("failed to flush: {e}"))?;

    Ok(format!(
        "Pushed test image to button 0 on {}",
        kind.human_name()
    ))
}

#[derive(Debug, Clone, Serialize)]
pub enum ButtonEvent {
    Down(u8),
    Up(u8),
}

/// Connects to the first supported device and waits (up to `timeout`) for
/// one batch of button events. Used by the hardware spike to confirm button
/// presses are actually readable, not just image push.
pub async fn read_events_once(timeout: std::time::Duration) -> Result<Vec<ButtonEvent>> {
    let (device, _kind) = connect_first().await?;
    let reader = device.get_reader(process_input);

    let updates = reader
        .read(Some(timeout))
        .await
        .map_err(|e| anyhow!("read failed: {e}"))?;

    Ok(updates
        .into_iter()
        .filter_map(|u| match u {
            mirajazz::state::DeviceStateUpdate::ButtonDown(k) => Some(ButtonEvent::Down(k)),
            mirajazz::state::DeviceStateUpdate::ButtonUp(k) => Some(ButtonEvent::Up(k)),
            _ => None,
        })
        .collect())
}

/// Maps raw (input, state) bytes to a [DeviceInput]. Shared across the whole
/// AKP03 family per the reference plugin — same physical 3x3 grid + 3
/// encoders layout despite different VID/PID branding.
pub fn process_input(input: u8, state: u8) -> Result<DeviceInput, MirajazzError> {
    match input {
        (0..=6) | 0x25 | 0x30 | 0x31 => read_button_press(input, state),
        _ => Err(MirajazzError::BadData),
    }
}

fn read_button_press(input: u8, state: u8) -> Result<DeviceInput, MirajazzError> {
    let mut button_states = vec![0u8; KEY_COUNT + 1];

    if input == 0 {
        return Ok(DeviceInput::ButtonStateChange(to_bools(&button_states)));
    }

    let pressed_index: usize = match input {
        (1..=6) => input as usize,
        0x25 => 7,
        0x30 => 8,
        0x31 => 9,
        _ => return Err(MirajazzError::BadData),
    };

    button_states[pressed_index] = state;

    Ok(DeviceInput::ButtonStateChange(to_bools(&button_states)))
}

fn to_bools(states: &[u8]) -> Vec<bool> {
    (0..KEY_COUNT).map(|i| states[i + 1] != 0).collect()
}

/// Reported over the channel `watch_forever` sends to - deliberately not
/// `mirajazz::device::DeviceLifecycleEvent` directly, so callers don't need
/// to depend on mirajazz's types just to consume this.
pub enum DeviceLifecycle {
    Connected(HidDeviceInfo),
    Disconnected,
}

/// Watches for the device being plugged in or unplugged **while the app is
/// already running**, and reports each transition over `tx` immediately -
/// this is the actual fix for "I unplugged and replugged the device and
/// the buttons stayed blank." Before this, the only way we noticed a
/// device was gone was a failed image push during the next scheduled
/// usage poll (up to `refresh_interval_secs` later, now 120-300s after
/// the 429 fix), and there was no evidence a failed push reliably fired
/// quickly after an actual unplug either. OS-level HID attach/detach
/// notifications (what this wraps, via mirajazz's DeviceWatcher /
/// async-hid) are how a well-behaved app is supposed to do this -
/// mirajazz already exposed it, we just weren't using it.
///
/// Runs until the underlying event stream ends (which shouldn't normally
/// happen for a live hardware-events subscription); callers should treat
/// that as worth retrying with a fresh `DeviceWatcher`, since a
/// `DeviceWatcher` can only be watched once.
///
/// Does **not** distinguish which of several devices changed if more than
/// one supported device is plugged in at once - matches the rest of this
/// module's single-active-device assumption (see connect_first). Not
/// something we've had a way to test with only one real device.
pub async fn watch_forever(tx: tokio::sync::mpsc::UnboundedSender<DeviceLifecycle>) {
    let mut watcher = DeviceWatcher::new();

    let stream = match watcher.watch(&QUERIES).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("failed to start device watcher: {e}");
            return;
        }
    };

    futures_lite::pin!(stream);

    while let Some(event) = stream.next().await {
        let lifecycle = match event {
            mirajazz::types::DeviceLifecycleEvent::Connected(info) => {
                DeviceLifecycle::Connected(info)
            }
            mirajazz::types::DeviceLifecycleEvent::Disconnected(_) => {
                DeviceLifecycle::Disconnected
            }
        };

        if tx.send(lifecycle).is_err() {
            return; // receiving end gone - app is shutting down
        }
    }
}
