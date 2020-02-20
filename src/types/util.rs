//! Utility functions
use crate::{
    error::{device_text::*, DeviceError},
    types::{
        device::{DevicePowerControl, DevicePowerStatus, Result as DeviceResult},
        UEventAction,
    },
};
use std::{collections::HashMap, fs, io::prelude::*, path::Path, time::Duration};

/// Read a uevent file
///
/// # Arguments
///
/// - `path`, path to the uevent file.
pub fn read_uevent(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in fs::read_to_string(path).unwrap().split_terminator('\n') {
        let line: &str = line;
        let mut i = line.split('=');
        let key = i.next().unwrap().into();
        let val = i.next().unwrap().into();
        map.insert(key, val);
    }
    map
}

/// Write a uevent file
///
/// # Arguments
///
/// - `path`, path to the uevent file.
pub fn write_uevent(
    path: &Path,
    action: UEventAction,
    uuid: Option<String>,
    args: HashMap<String, String>,
) {
    let mut data = String::new();
    match action {
        UEventAction::Add => data.push_str("add"),
        UEventAction::Change => data.push_str("change"),
        UEventAction::Remove => data.push_str("remove"),
    }
    data.push(' ');
    if let Some(uuid) = uuid {
        data.push_str(&uuid);
        data.push(' ');
    }
    for (k, v) in args {
        data.push_str(&k);
        data.push('=');
        data.push_str(&v);
        data.push(' ');
    }
    //
    let mut f = fs::OpenOptions::new().write(true).open(path).unwrap();
    f.write_all(data.trim().as_bytes()).unwrap();
}

pub fn read_subsystem(path: &Path) -> DeviceResult<String> {
    fs::read_link(path.join("subsystem"))?
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.into())
        .ok_or_else(|| DeviceError::InvalidDevice(DEVICE))
}

pub fn read_driver(path: &Path) -> DeviceResult<Option<String>> {
    fs::read_link(path.join("driver"))?
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| Some(s.into()))
        .ok_or_else(|| DeviceError::InvalidDevice(DEVICE))
}

pub fn read_power_control(path: &Path) -> DeviceResult<DevicePowerControl> {
    fs::read_to_string(path.join("power/control")).map(|s| match s.trim() {
        "auto" => Ok(DevicePowerControl::Auto),
        "on" => Ok(DevicePowerControl::On),
        _ => Err(DeviceError::InvalidDevice(DEVICE)),
    })?
}

pub fn read_power_autosuspend_delay(path: &Path) -> DeviceResult<Option<Duration>> {
    Ok(fs::read_to_string(path.join("power/autosuspend_delay_ms"))
        .map(|s| s.trim().parse())?
        .map(|s| Duration::from_millis(s))
        .ok())
}

pub fn read_power_status(path: &Path) -> DeviceResult<DevicePowerStatus> {
    fs::read_to_string(path.join("power/runtime_status")).map(|s| match s.trim() {
        "suspended" => Ok(DevicePowerStatus::Suspended),
        "suspending" => Ok(DevicePowerStatus::Suspending),
        "resuming" => Ok(DevicePowerStatus::Resuming),
        "active" => Ok(DevicePowerStatus::Active),
        "error" => Ok(DevicePowerStatus::FatalError),
        "unsupported" => Ok(DevicePowerStatus::Unsupported),
        _ => Err(DeviceError::InvalidDevice(DEVICE)),
    })?
}

pub fn read_power_async(path: &Path) -> DeviceResult<bool> {
    fs::read_to_string(path.join("power/async")).map(|s| match s.trim() {
        "enabled" => Ok(true),
        "disabled" => Ok(false),
        _ => Err(DeviceError::InvalidDevice(DEVICE)),
    })?
}
