//! Interface to Dynamically Loaded Linux Kernel Modules.
//!
//! # Examples
//!
//! Print all currently loaded system modules
//!
//! ```rust
//! # use linapi::system::modules::*;
//!
//! let mods = LoadedModule::get_loaded().unwrap();
//!
//! for m in mods {
//!     println!("Module: {}", m.name());
//! }
//! ```
//!
//! Load a module
//!
//! ```rust,no_run
//! # use linapi::system::modules::*;
//!
//! let m = ModuleFile::from_name("MyModule").unwrap();
//! let loaded = m.load("my_param=1").unwrap();
//! println!("Loaded module {}. my_param={}", loaded.name(), std::str::from_utf8(&loaded.parameters()["my_param"]).unwrap());
//! ```
//!
//! # Implementation
//!
//! This uses the sysfs interface, documented [here][1] and [here][2], and
//! various undocumented interfaces where noted.
//!
//! [1]: https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-module
//! [2]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-module
use crate::{
    error::{text::*, ModuleError},
    extensions::FileExt,
    system::{UEvent, UEventAction},
    util::{read_uevent, write_uevent, MODULE_PATH, SYSFS_PATH},
};
#[cfg(feature = "gz")]
use flate2::bufread::GzDecoder;
use nix::{
    kmod::{delete_module, finit_module, init_module, DeleteModuleFlags, ModuleInitFlags},
    sys::utsname::uname,
};
use std::{
    collections::HashMap,
    ffi::CString,
    fs,
    fs::DirEntry,
    io,
    io::{prelude::*, BufRead},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
use xmas_elf::ElfFile;
#[cfg(feature = "xz")]
use xz2::bufread::XzDecoder;

const SIGNATURE_MAGIC: &[u8] = b"~Module signature appended~\n";

pub type Result<T, E = Box<dyn std::error::Error + Send + Sync>> = std::result::Result<T, E>;

/// Helper to read the `attribute` at `path`. Trims it.
fn read_attribute<P: AsRef<Path>>(base: P, attribute: &'static str) -> Result<String, io::Error> {
    fs::read_to_string(base.as_ref().join(attribute)).map(|s| s.trim().to_owned())
}

/// Kernel modules can be "tainted", which serve as a marker for debugging
/// purposes.
#[derive(Debug, Clone, Copy)]
pub enum Taint {
    /// Not tainted
    Clean,

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

/// Module Init Status
#[derive(Debug, Clone)]
pub enum Status {
    /// Normal state, fully loaded.
    Live,

    /// Running module init
    Coming,

    /// Going away, running module exit?
    Going,

    /// Unknown
    Unknown(String),
}

/// Describes a loaded Linux kernel Module
#[derive(Debug)]
pub struct LoadedModule {
    /// The name of the Module
    name: String,

    /// Type of module
    module_type: Type,

    /// Path to the module
    path: PathBuf,
}

// Public
impl LoadedModule {
    /// Get an already loaded module by name
    ///
    /// # Errors
    ///
    /// - If no such module exists
    /// - If the module is invalid in some way
    pub fn from_name(name: &str) -> Result<Self> {
        Self::from_dir(&Path::new(SYSFS_PATH).join("module").join(name))
    }

    /// Get currently loaded dynamic kernel modules.
    ///
    /// # Errors
    ///
    /// - I/O
    /// - If any modules couldn't be read
    pub fn get_loaded() -> Result<Vec<Self>> {
        let dir = Path::new(SYSFS_PATH).join("module");
        let mut mods = Vec::new();
        //
        for module in fs::read_dir(dir)? {
            let module: DirEntry = module?;
            let m = Self::from_dir(&module.path())?;
            if let Type::BuiltIn = m.module_type() {
                continue;
            }
            mods.push(m);
        }
        Ok(mods)
    }

    /// Unload the module.
    ///
    /// # Errors
    ///
    /// - On failure
    pub fn unload(self) -> Result<()> {
        delete_module(
            &CString::new(self.name.as_str()).expect("Module name had null bytes"),
            DeleteModuleFlags::O_NONBLOCK,
        )?;
        // .map_err(|e| UnloadError {
        //     module: self.name,
        //     source: e,
        // })?;
        //
        Ok(())
    }

    /// Forcefully unload a kernel module.
    ///
    /// # Safety
    ///
    /// Force unloading is wildly dangerous and will taint your kernel.
    ///
    /// It can cause modules to be unloaded while still in use, or unload
    /// modules not designed to be unloaded.
    ///
    /// # Errors
    ///
    /// - On failure
    pub unsafe fn force_unload(self) -> Result<()> {
        delete_module(
            &CString::new(self.name.as_str()).expect("Module name had null bytes"),
            DeleteModuleFlags::O_NONBLOCK | DeleteModuleFlags::O_TRUNC,
        )
        .map_err(|e| ModuleError::UnloadError(self.name, e.to_string()))?;
        //
        Ok(())
    }

    /// Name of the module
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Module type, Builtin or Dynamic
    pub fn module_type(&self) -> Type {
        self.module_type
    }

    /// Module parameters.
    ///
    /// The kernel exposes these as files in a directory, and their contents are
    /// entirely module specific, hence `HashMap<String, Vec<u8>>`.
    ///
    /// The key will be the parameter name and the value is it's data.
    ///
    /// There is currently no way to *write* parameters.
    ///
    /// # Stability
    ///
    /// The stability of parameters depends entirely on the specific module.
    ///
    /// # Panics
    ///
    /// - If a module parameters name is not valid UTF-8
    ///
    /// # Errors
    ///
    /// - If I/O does
    // FIXME: Need custom type with Read/Write here?
    pub fn parameters(&self) -> Result<HashMap<String, Vec<u8>>> {
        let mut map = HashMap::new();
        let path = self.path.join("parameters");
        if path.exists() {
            for entry in fs::read_dir(path)? {
                let entry: DirEntry = entry?;
                map.insert(
                    entry
                        .file_name()
                        .into_string()
                        .expect("Module parameter not valid UTF-8"),
                    fs::read(entry.path())?,
                );
            }
        }
        Ok(map)
    }

    /// Module reference count.
    ///
    /// If the module is built-in, or if the kernel was not built with
    /// `CONFIG_MODULE_UNLOAD`, this will be [`None`]
    ///
    /// # Errors
    ///
    /// - If I/O does
    pub fn ref_count(&self) -> Result<Option<u32>> {
        match read_attribute(&self.path, "refcnt") {
            Ok(s) => Ok(Some(s.parse()?)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Module size in bytes
    ///
    /// # Errors
    ///
    /// - If I/O does
    pub fn size(&self) -> Result<u64> {
        Ok(read_attribute(&self.path, "coresize")?.parse()?)
    }

    /// Module taint.
    ///
    /// See [`Taint`] for details.
    ///
    /// # Errors
    ///
    /// - On I/O
    /// - On unexpected taint flags
    pub fn taint(&self) -> Result<Taint> {
        // Can a module have *multiple* taint flags?
        match &*read_attribute(&self.path, "taint")? {
            "P" => Ok(Taint::Proprietary),
            "O" => Ok(Taint::OutOfTree),
            "F" => Ok(Taint::Forced),
            "C" => Ok(Taint::Staging),
            "E" => Ok(Taint::Unsigned),
            "" => Ok(Taint::Clean),
            _ => Err("Unexpected".into()),
        }
    }

    /// Names of other modules that use/reference this one.
    ///
    /// # Note
    ///
    /// This uses the `holders` sysfs folder, which is completely undocumented
    /// by the kernel, beware.
    ///
    /// # Panics
    ///
    /// - If the module name is not valid UTF-8
    ///
    /// # Errors
    ///
    /// - If I/O does
    pub fn holders(&self) -> Result<Vec<String>> {
        let mut v = Vec::new();
        for re in fs::read_dir(self.path.join("holders"))? {
            let re: DirEntry = re?;
            v.push(re.file_name().into_string().expect("Invalid Module Name"))
        }
        Ok(v)
    }

    /// Get a [`ModuleFile`] from a [`LoadedModule`]
    ///
    /// This can be useful to get information, such as parameter types, about a
    /// module.
    ///
    /// # Note
    ///
    /// There is no guarantee the returned path is the same module. The file may
    /// have changed on disk, or been removed.
    ///
    /// This is equivalent to `ModuleFile::from_name(&self.name)`
    ///
    /// # Errors
    ///
    /// See [`ModuleFile::from_name`]
    pub fn module_file(&self) -> Result<ModuleFile> {
        ModuleFile::from_name(&self.name)
    }

    /// Module status.
    ///
    /// # Note
    ///
    /// This uses the undocumented `initstate` file, which is probably
    /// `module_state` from `linux/module.h`.
    ///
    /// # Errors
    ///
    /// - If I/O does
    pub fn status(&self) -> Result<Status> {
        match &*read_attribute(&self.path, "initstate")? {
            "live" => Ok(Status::Live),
            "coming" => Ok(Status::Coming),
            "going" => Ok(Status::Going),
            s => Ok(Status::Unknown(s.into())),
        }
    }
}

// Private
impl LoadedModule {
    /// Create from module directory
    ///
    /// # Errors
    ///
    /// - If module doesn't exist
    /// - If module is invalid
    ///
    /// # Note
    ///
    /// Built-in modules may appear in `/sys/modules` and they are ill-formed,
    /// missing required files.
    ///
    /// In this case `refcnt` is [`None`], `coresize` is 0, and `taint` is
    /// [`None`]
    ///
    /// This method performs automatic underscore conversion.
    fn from_dir(path: &Path) -> Result<Self> {
        let name = path
            .file_name()
            .expect("Missing module name")
            .to_str()
            .expect("Invalid module name");
        // `/sys/modules` seems to always use `_` in paths?
        let path = path.with_file_name(name.replace('-', "_"));
        if !path.exists() {
            return Err(ModuleError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Couldn't find loaded module at {}", path.display()),
            ))
            .into());
        }
        // Only dynamic modules seem to have `coresize`/`initstate`
        let module_type = if path.join("coresize").exists() {
            Type::Dynamic
        } else {
            Type::BuiltIn
        };
        let s = Self {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.trim().to_owned())
                .ok_or_else(|| ModuleError::InvalidModule(NAME.into()))?,
            module_type,
            path,
        };
        Ok(s)
    }
}

impl UEvent for LoadedModule {
    fn write(&self, action: UEventAction, uuid: Option<String>, args: HashMap<String, String>) {
        write_uevent(&self.path.join("uevent"), action, uuid, args)
    }
    fn read(&self) -> HashMap<String, String> {
        read_uevent(&self.path.join("uevent"))
    }
}

/// A Linux Kernel Module file on disk.
///
/// On construction information about the module is read and saved.
///
/// But the file may change on disk or even be removed, so you can use
/// `ModuleFile::refresh` to update the information or show an error if it's
/// been removed.
#[derive(Debug)]
pub struct ModuleFile {
    name: String,
    path: PathBuf,
    //
    info: Option<ModInfo>,
    signature: bool,
}

// Public methods
impl ModuleFile {
    /// Refresh information on the module
    ///
    /// # Errors
    ///
    /// - If the file no longer exists
    /// - If the module or any of it's information is invalid
    pub fn refresh(&mut self) -> Result<()> {
        let img = self.read()?;
        self.info = Some(self._info(&img)?);
        self.signature = img.ends_with(SIGNATURE_MAGIC);
        //
        Ok(())
    }

    /// Search `/lib/modules/(uname -r)` for the module `name`.
    ///
    /// # Errors
    ///
    /// - If the module couldn't be found
    /// - See [`ModuleFile::refresh`]
    pub fn from_name(name: &str) -> Result<Self> {
        Self::from_name_with_uname(name, uname().release())
    }

    /// Search `lib/modules/<uname>` for the module `name`.
    ///
    /// See [`ModuleFile::from_name`] for more details.
    pub fn from_name_with_uname(name: &str, uname: &str) -> Result<Self> {
        let path = Path::new(MODULE_PATH).join(uname);
        for entry in WalkDir::new(path) {
            let entry = entry.map_err(|e| ModuleError::Io(e.into()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            // Get the module filename without any extensions.
            // Modules are `.ko` but can be compressed, `.ko.xz`.
            let m_name = entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.splitn(2, '.').next())
                .ok_or_else(|| ModuleError::InvalidModule(INVALID_EXTENSION.into()))?;
            if m_name == name {
                let mut s = Self {
                    name: name.into(),
                    path: entry.into_path(),
                    info: None,
                    signature: false,
                };
                s.refresh()?;
                return Ok(s);
            }
        }
        Err(ModuleError::LoadError(name.into(), NOT_FOUND.into()).into())
    }

    /// Use the file at `path` as a module.
    ///
    /// # Errors
    ///
    /// - if `path` does not exist
    /// - if `path` is not a valid module.
    pub fn from_path(path: &Path) -> Result<Self> {
        let mut s = Self {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| {
                    ModuleError::LoadError(path.display().to_string(), NOT_FOUND.into())
                })?
                .into(),
            path: path.into(),
            info: None,
            signature: false,
        };
        s.refresh()?;
        //
        Ok(s)
    }

    /// Load this kernel module, and return the [`LoadedModule`] describing it.
    ///
    /// # Arguments
    ///
    /// - `param`eters for the kernel module. See module documentation for
    ///   details, and `init_module(2)` for details on formatting.
    ///
    /// # Errors
    ///
    /// - If the file no longer exists
    /// - If the file can't be decompressed
    /// - If the module fails to load
    ///
    /// # Panics
    ///
    /// - if `param` has any `0` bytes.
    ///
    /// # Note
    ///
    /// Kernel modules may be compressed, and depending on crate features this
    /// function may automatically decompress it.
    pub fn load(&self, param: &str) -> Result<LoadedModule> {
        let img = self.read()?;
        // FIXME: ModuleError::AlreadyLoaded
        init_module(
            &img,
            &CString::new(param).expect("param can't have internal null bytes"),
        )
        .map_err(|e| ModuleError::LoadError(self.name.clone(), e.to_string()))?;

        Ok(LoadedModule::from_dir(
            &Path::new(SYSFS_PATH).join("module").join(&self.name),
        )?)
    }

    /// Force load this kernel module, and return the [`LoadedModule`]
    /// describing it.
    ///
    /// See [`ModuleFile::load`] for more details.
    ///
    /// # Safety
    ///
    /// Force loading a kernel module is dangerous, it skips important safety
    /// checks that help ensure module compatibility with your kernel.
    pub unsafe fn force_load(&self, param: &str) -> Result<LoadedModule> {
        let mut file = fs::File::create_memory("decompressed module");
        file.write_all(&self.read()?)?;
        //
        finit_module(
            &file,
            &CString::new(param).expect("param can't have internal null bytes"),
            ModuleInitFlags::MODULE_INIT_IGNORE_MODVERSIONS
                | ModuleInitFlags::MODULE_INIT_IGNORE_VERMAGIC,
        )
        .map_err(|e| ModuleError::LoadError(self.name.clone(), e.to_string()))?;
        //
        Ok(LoadedModule::from_dir(
            &Path::new(SYSFS_PATH).join("module").join(&self.name),
        )?)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get information embedded in the module file.
    pub fn info(&self) -> &ModInfo {
        // This unwrap should be okay, as `refresh` should be called by all constructors
        // and ensure this is `Some`
        self.info.as_ref().unwrap()
    }

    /// Whether the module has a signature.
    ///
    /// This does not check if it's valid.
    ///
    /// # Note
    ///
    /// This is a temporary API, as `rust-openssl` does not expose the APIs
    /// required for properly reading module signatures.
    // FIXME: rust-openssl does not expose the APIs we need, so this isn't possible.
    // When/if they do, see `module_signature.h` for details on structure.
    pub fn has_signature(&self) -> bool {
        self.signature
    }
}

// Private methods
impl ModuleFile {
    fn read(&self) -> Result<Vec<u8>> {
        self.decompress(fs::read(&self.path)?)
    }

    fn _info(&self, img: &[u8]) -> Result<ModInfo> {
        let elf = ElfFile::new(img).map_err(|e| ModuleError::InvalidModule(e.to_string()))?;
        let sect = elf
            .find_section_by_name(".modinfo")
            .ok_or_else(|| ModuleError::InvalidModule(MODINFO.into()))?;
        let data = sect.raw_data(&elf);
        //
        let mut map = HashMap::new();
        for kv in BufRead::split(data, b'\0') {
            let kv = kv?;
            let s = String::from_utf8(kv).map_err(|e| ModuleError::InvalidModule(e.to_string()))?;
            let mut s = s.splitn(2, '=');
            //
            let key = s
                .next()
                .map(|s| s.to_string())
                .ok_or_else(|| ModuleError::InvalidModule(MODINFO.into()))?;
            let value = s
                .next()
                .map(|s| s.to_string())
                .ok_or_else(|| ModuleError::InvalidModule(MODINFO.into()))?;
            let vec = map.entry(key).or_insert_with(Vec::new);
            if !value.is_empty() {
                vec.push(value);
            }
        }
        fn y_n(s: &str) -> bool {
            s == "Y" || s == "y"
        }
        fn one(map: &mut HashMap<String, Vec<String>>, key: &str) -> String {
            map.remove(key).map(|mut v| v.remove(0)).unwrap_or_default()
        }
        fn more(map: &mut HashMap<String, Vec<String>>, key: &str) -> Vec<String> {
            map.remove(key).unwrap_or_default()
        }
        //
        let mut x = HashMap::new();
        for (name, typ) in map
            .remove("parmtype")
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                let mut i = s.splitn(2, ':').map(|s| s.trim().to_owned());
                (i.next(), i.next())
            })
        {
            let name: Option<String> = name;
            let typ: Option<String> = typ;
            // Types are reasonably guaranteed to exist because
            // `linux/moduleparam.h` adds them for all the `module_param`
            // macros, which define parameters.
            let name = name.ok_or_else(|| ModuleError::InvalidModule(MODINFO.into()))?;
            let typ = typ.ok_or_else(|| ModuleError::InvalidModule(MODINFO.into()))?;
            // Parameters should not have multiple types.
            if x.insert(name, (typ, None)).is_some() {
                return Err(ModuleError::InvalidModule(MODINFO.into()).into());
            };
        }
        for (name, desc) in map.remove("parm").unwrap_or_default().into_iter().map(|s| {
            let mut i = s.splitn(2, ':').map(|s| s.trim().to_owned());
            (i.next(), i.next())
        }) {
            let name: Option<String> = name;
            let desc: Option<String> = desc;
            //
            let name = name.ok_or_else(|| ModuleError::InvalidModule(MODINFO.into()))?;
            // If we've seen the parameter, which we should have it's probably a
            // module bug otherwise, add it's description.
            //
            // Parameters aren't required to have descriptions.
            x.get_mut(&name)
                .map(|v| v.1 = desc)
                .ok_or_else(|| ModuleError::InvalidModule(MODINFO.into()))?;
        }
        let mut parameters = Vec::new();
        for (name, (type_, description)) in x {
            parameters.push(ModParam {
                name,
                type_,
                description,
            })
        }
        //
        Ok(ModInfo {
            alias: more(&mut map, "alias"),
            soft_dependencies: more(&mut map, "softdep"),
            license: one(&mut map, "license"),
            authors: more(&mut map, "author"),
            description: one(&mut map, "description"),
            version: one(&mut map, "version"),
            firmware: more(&mut map, "firmware"),
            version_magic: one(&mut map, "vermagic"),
            name: one(&mut map, "name"),
            in_tree: y_n(&one(&mut map, "intree")),
            retpoline: y_n(&one(&mut map, "retpoline")),
            staging: y_n(&one(&mut map, "staging")),
            dependencies: more(&mut map, "depends"),
            source_checksum: one(&mut map, "srcversion"),
            parameters,
        })
    }

    /// Decompresses a kernel module
    ///
    /// Returns `data` unchanged if not compressed.
    fn decompress(&self, data: Vec<u8>) -> Result<Vec<u8>> {
        #[cfg(any(feature = "xz", feature = "gz"))]
        let mut v = Vec::new();
        let ext = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| ModuleError::InvalidModule(INVALID_EXTENSION.into()))?;
        match ext {
            #[cfg(feature = "xz")]
            "xz" => {
                let mut data = XzDecoder::new(data.as_slice());
                data.read_to_end(&mut v)
                    .map_err(|e| ModuleError::InvalidModule(e.to_string()))?;
                Ok(v)
            }
            #[cfg(feature = "gz")]
            "gz" => {
                let mut data = GzDecoder::new(data.as_slice());
                data.read_to_end(&mut v)
                    .map_err(|e| ModuleError::InvalidModule(e.to_string()))?;
                Ok(v)
            }
            "ko" => Ok(data),
            _ => Err(ModuleError::InvalidModule(COMPRESSION.into()).into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModParam {
    /// Parameter name
    pub name: String,

    /// Parameter name
    ///
    /// See `module_param` in `linux/moduleparam.h` for details
    // TODO: Replace with enum for standard types
    pub type_: String,

    pub description: Option<String>,
}

/// Information on a [`ModuleFile`]
///
/// # Notes
///
/// This uses the `.modinfo` ELF section, which is semi-documented in
/// `linux/modules.h` and `MODULE_INFO`.
#[derive(Debug)]
pub struct ModInfo {
    /// Module Aliases. Alternative names for this module.
    pub alias: Vec<String>,

    /// Soft Dependencies. Not required, but may provide additional features.
    pub soft_dependencies: Vec<String>,

    /// Module License
    ///
    /// See `MODULE_LICENSE` for details on this value.
    pub license: String,

    /// Module Author and email
    pub authors: Vec<String>,

    /// What the module does
    pub description: String,

    /// Module version
    pub version: String,

    /// Optional firmware file(s) needed by the module
    pub firmware: Vec<String>,

    /// Version magic string, used by the kernel for compatibility checking.
    pub version_magic: String,

    /// Module name, self-reported.
    pub name: String,

    /// Whether the module is from the kernel source tree.
    pub in_tree: bool,

    /// The retpoline security feature
    pub retpoline: bool,

    /// If the module is staging
    pub staging: bool,

    /// Other modules this one depends on
    pub dependencies: Vec<String>,

    /// Source Checksum.
    pub source_checksum: String,

    /// Module Parameters
    pub parameters: Vec<ModParam>,
}
