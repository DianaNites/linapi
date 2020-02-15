//! Dynamically Loaded kernel modules.
//!
//! This interface is documented [here][1] and [here][2]
//!
//! [1]: https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-module
//! [2]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-module
use super::{
    interfaces::UEvent,
    util::{read_uevent, write_uevent},
    SYSFS_PATH,
};

use std::{
    collections::HashMap,
    fs,
    fs::DirEntry,
    io,
    path::{Path, PathBuf},
};

/// Kernel modules can be "tainted", which serve as a marker for debugging
/// purposes.
#[derive(Debug, Clone, Copy)]
pub enum Taint {
    /// Proprietary Module.
    Proprietary,

    /// Out of tree, third party Module.
    OutOfTree,

    /// Module was force loaded.
    Forced,

    /// Unstable Staging Module.
    Staging,

    /// Unsigned Module.
    Unsigned,
}

/// Module type
#[derive(Debug, Clone, Copy)]
pub enum Type {
    /// Built in to the kernel.
    ///
    /// These only show up if they have a version or one run-time parameter, and
    /// are missing most values.
    ///
    /// # Note
    ///
    /// The fact these show up isn't intentional and technically may change, or
    /// so claim the kernel docs.
    BuiltIn,

    /// Dynamically loaded.
    Dynamic,
}

/// Describes a Linux kernel Module
#[derive(Debug)]
pub struct Module {
    /// The name of the Module
    name: String,

    /// Type of module
    module_type: Type,

    /// Path to the module
    path: PathBuf,
}

impl Module {
    /// Create from module directory
    ///
    /// # Note
    ///
    /// Built-in modules may appear in `/sys/modules` and they are ill-formed,
    /// missing required files.
    ///
    /// In this case `refcnt` is [`None`], `coresize` is 0, and `taint` is
    /// [`None`]
    fn from_dir(path: &Path) -> Self {
        let module_type = if path.join("coresize").exists() {
            Type::Dynamic
        } else {
            Type::BuiltIn
        };
        Module {
            name: path.file_stem().unwrap().to_str().unwrap().trim().into(),
            module_type,
            path: path.into(),
        }
    }

    /// Name of the module
    pub fn name(&self) -> String {
        self.path.file_stem().unwrap().to_str().unwrap().into()
    }

    /// Module type, Builtin or Dynamic
    pub fn module_type(&self) -> Type {
        self.module_type
    }

    /// Module parameters.
    ///
    /// The kernel exposes these as files in a directory, and their contents are
    /// entirely module specific, hence `Vec<(String, Vec<u8>)>`, which can be
    /// [`std::io::Read`].
    ///
    /// # Stability
    ///
    /// The stability of parameters depends entirely on the specific module.
    pub fn parameters(&self) -> HashMap<String, Vec<u8>> {
        HashMap::new()
    }

    /// Module reference count.
    ///
    /// If the module is built-in, or if the kernel was not built with
    /// `CONFIG_MODULE_UNLOAD`, this will be [`None`]
    pub fn ref_count(&self) -> Option<u32> {
        fs::read_to_string(self.path.join("refcnt"))
            .map(|s| s.trim().parse().unwrap())
            .ok()
    }

    /// Module size in bytes
    pub fn size(&self) -> u64 {
        fs::read_to_string(self.path.join("coresize"))
            .map(|s| s.trim().parse().unwrap())
            .unwrap()
    }

    /// Module taint, or [`None`] if untainted.
    ///
    /// See [`Taint`] for details.
    pub fn taint(&self) -> Option<Taint> {
        match fs::read_to_string(self.path.join("taint")).unwrap().trim() {
            "P" => Some(Taint::Proprietary),
            "O" => Some(Taint::OutOfTree),
            "F" => Some(Taint::Forced),
            "C" => Some(Taint::Staging),
            "E" => Some(Taint::Unsigned),
            _ => None,
        }
    }

    /// List of other modules that use/reference this one.
    ///
    /// # Note
    ///
    /// This uses the `holders` sysfs folder, which is completely undocumented
    /// by the kernel, beware.
    pub fn holders(&self) -> Vec<Module> {
        let mut v = Vec::new();
        for re in fs::read_dir(self.path.join("holders")).unwrap() {
            let re: DirEntry = re.unwrap();
            v.push(Module::from_dir(&re.path()))
        }
        v
    }
}

impl UEvent for Module {
    fn write(
        &self,
        action: super::interfaces::UEventAction,
        uuid: Option<String>,
        args: HashMap<String, String>,
    ) {
        write_uevent(&self.path.join("uevent"), action, uuid, args)
    }
    fn read(&self) -> HashMap<String, String> {
        read_uevent(&self.path.join("uevent"))
    }
}

/// Get loaded system modules.
///
/// # Note
///
/// This ignores any built-in modules that may appear.
pub fn get_system_modules() -> Vec<Module> {
    let dir = Path::new(SYSFS_PATH).join("module");
    let mut mods = Vec::new();
    //
    for module in fs::read_dir(dir).unwrap() {
        let module: DirEntry = module.unwrap();
        let m = Module::from_dir(&module.path());
        if let Type::BuiltIn = m.module_type() {
            continue;
        }
        mods.push(m);
    }
    mods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        dbg!(get_system_modules());
        todo!();
    }
}
