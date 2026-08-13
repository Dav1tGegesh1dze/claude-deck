//! Thin wrapper over `mirajazz` for talking to the AJAZZ AKP03 family (and
//! rebranded Mirabox/Soomfon/etc. hardware sharing the same firmware/protocol).
//!
//! VID/PID table and protocol details sourced from the reference OpenDeck
//! plugin (github.com/4ndv/opendeck-akp03), confirmed against mirajazz
//! 0.16.2's actual API on 2026-08-13. **Not yet verified against real
//! hardware** — no AKP03E was connected to the dev machine during Phase 1.
//! Verify before Phase 5 release (see ROADMAP.md Phase 0).

use anyhow::{anyhow, Result};
use image::{DynamicImage, Rgb, RgbImage};
use mirajazz::{
    device::{list_devices, Device, DeviceQuery},
    error::MirajazzError,
    types::{DeviceInput, HidDeviceInfo, ImageFormat, ImageMirroring, ImageMode, ImageRotation},
};
use serde::Serialize;

pub const KEY_COUNT: usize = 9;
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
/// Not yet verified against real hardware (see module doc comment).
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
