//! Utility functions
use crate::types::UEventAction;
use std::{collections::HashMap, fs, io::prelude::*, path::Path};

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
