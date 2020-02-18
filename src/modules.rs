//! Interface to Dynamically Loaded Linux Kernel Modules.
//!
//! # Examples
//!
//! Print all currently loaded system modules
//!
//! ```rust
//! # use linapi::modules::*;
//!
//! let mods = LoadedModule::from_loaded();
//!
//! for m in mods {
//!     println!("Module: {}", m.name());
//! }
//! ```
//!
//! Load a module
//!
//! ```rust,no_run
//! # use linapi::modules::*;
//!
//! let m = ModuleFile::from_name("MyModule");
//! let loaded = m.load("my_param=1");
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
    types::{
        util::{read_uevent, write_uevent},
        UEvent,
        UEventAction,
        MODULE_PATH,
        SYSFS_PATH,
    },
};
use flate2::read::GzDecoder;
use lzma_rs::xz_decompress;
use nix::{
    kmod::{delete_module, finit_module, init_module, DeleteModuleFlags, ModuleInitFlags},
    sys::utsname::uname,
};
use std::{
    collections::HashMap,
    ffi::CString,
    fs,
    fs::DirEntry,
    io::{prelude::*, BufRead},
    mem::size_of,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
use xmas_elf::ElfFile;

const SIGNATURE_MAGIC: &[u8] = b"~Module signature appended~\n";

pub type Result<T> = std::result::Result<T, ModuleError>;

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

/// Module Init Status
#[derive(Debug, Clone, Copy)]
pub enum Status {
    /// Normal state, fully loaded.
    Live,

    /// Running module init
    Coming,

    /// Going away, running module exit?
    Going,
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

impl LoadedModule {
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
        Self {
            name: path.file_stem().unwrap().to_str().unwrap().trim().into(),
            module_type,
            path: path.into(),
        }
    }

    /// Get an already loaded module by name
    ///
    /// # Panics
    ///
    /// - If no such module exists
    pub fn from_name(name: &str) -> Self {
        Self::from_dir(&Path::new(SYSFS_PATH).join("module").join(name))
    }

    /// Get currently loaded dynamic kernel modules
    ///
    /// # Note
    ///
    /// Modules can be unloaded, and if that happens methods on [`LoadedModule`]
    /// will panic
    pub fn get_loaded() -> Vec<Self> {
        let dir = Path::new(SYSFS_PATH).join("module");
        let mut mods = Vec::new();
        //
        for module in fs::read_dir(dir).unwrap() {
            let module: DirEntry = module.unwrap();
            let m = Self::from_dir(&module.path());
            if let Type::BuiltIn = m.module_type() {
                continue;
            }
            mods.push(m);
        }
        mods
    }

    /// Unload the module.
    ///
    /// # Panics
    ///
    /// - On failure
    pub fn unload(self) {
        delete_module(
            &CString::new(self.name).unwrap(),
            DeleteModuleFlags::O_NONBLOCK,
        )
        .unwrap();
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
    /// # Panics
    ///
    /// - On failure
    pub unsafe fn force_unload(self) {
        delete_module(
            &CString::new(self.name).unwrap(),
            DeleteModuleFlags::O_NONBLOCK | DeleteModuleFlags::O_TRUNC,
        )
        .unwrap();
    }

    /// Name of the module
    pub fn name(&self) -> &str {
        self.path.file_stem().unwrap().to_str().unwrap()
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
        todo!()
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
    pub fn holders(&self) -> Vec<Self> {
        let mut v = Vec::new();
        for re in fs::read_dir(self.path.join("holders")).unwrap() {
            let re: DirEntry = re.unwrap();
            v.push(Self::from_dir(&re.path()))
        }
        v
    }

    /// Path to the module file.
    ///
    /// # Note
    ///
    /// There is no guarantee the returned path is the same module. The file may
    /// have changed on disk.
    ///
    /// This is equivalent to `find_module_file(&module.name())`
    pub fn file_path(&self) -> PathBuf {
        // find_module_file(&self.name())
        todo!()
    }

    /// Module status.
    ///
    /// # Note
    ///
    /// This uses the undocumented `initstate` file, which is probably
    /// `module_state` from `linux/module.h`.
    pub fn status(&self) -> Status {
        match fs::read_to_string(self.path.join("initstate"))
            .unwrap()
            .trim()
        {
            "live" => Status::Live,
            "coming" => Status::Coming,
            "going" => Status::Going,
            _ => panic!("Unknown module state"),
        }
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
        let path = Path::new(MODULE_PATH)
            .join(uname().release())
            .join("kernel");
        for entry in WalkDir::new(path) {
            let entry = entry.map_err(|e| ModuleError::Io(e.into()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            // Get the module filename without any extensions.
            // Modules are `.ko` but can be compressed, `.ko.xz`.
            let m_name = entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .and_then(|s| s.splitn(2, ".").next())
                .ok_or(ModuleError::InvalidModule(INVALID_EXTENSION.into()))?;
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
        return Err(ModuleError::LoadError(name.into(), NOT_FOUND.into()));
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
                .ok_or(ModuleError::LoadError(
                    path.display().to_string(),
                    NOT_FOUND.into(),
                ))?
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
        init_module(&img, &CString::new(param).unwrap())
            .map_err(|e| ModuleError::LoadError(self.name.clone(), e.to_string()))?;
        //
        Ok(LoadedModule::from_dir(
            &Path::new(SYSFS_PATH).join("module").join(&self.name),
        ))
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
            &CString::new(param).unwrap(),
            ModuleInitFlags::MODULE_INIT_IGNORE_MODVERSIONS
                | ModuleInitFlags::MODULE_INIT_IGNORE_VERMAGIC,
        )
        .map_err(|e| ModuleError::LoadError(self.name.clone(), e.to_string()))?;
        //
        Ok(LoadedModule::from_dir(
            &Path::new(SYSFS_PATH).join("module").join(&self.name),
        ))
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
            .ok_or(ModuleError::InvalidModule(MODINFO.into()))?;
        let data = sect.raw_data(&elf);
        dbg!(data);
        //
        let mut map = HashMap::new();
        for kv in BufRead::split(data, b'\0') {
            let kv = kv?;
            let s = String::from_utf8(kv).map_err(|e| ModuleError::InvalidModule(e.to_string()))?;
            let mut s = s.splitn(2, '=');
            //
            let key = s
                .next()
                .and_then(|s| Some(s.to_string()))
                .ok_or(ModuleError::InvalidModule(MODINFO.into()))?;
            let value = s
                .next()
                .and_then(|s| Some(s.to_string()))
                .ok_or(ModuleError::InvalidModule(MODINFO.into()))?;
            let vec = map.entry(key).or_insert(Vec::new());
            if !value.is_empty() {
                vec.push(value);
            }
        }
        fn y_n(s: &str) -> bool {
            if s == "Y" {
                true
            } else {
                false
            }
        }
        fn one(map: &mut HashMap<String, Vec<String>>, key: &str) -> String {
            map.remove(key).map(|mut v| v.remove(0)).unwrap_or_default()
        }
        fn more(map: &mut HashMap<String, Vec<String>>, key: &str) -> Vec<String> {
            map.remove(key).unwrap_or_default()
        }
        let mut parameters = Vec::new();
        // FIXME: Are parameters and their types guaranteed to be the same order?
        // Sort first?
        for ((name, description), type_) in map
            .remove("parm")
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                let mut i = s.splitn(2, ':');
                // These unwraps *should* be okay, if not it means .modinfo is incorrect
                let name = i.next().unwrap();
                let desc = i.next().unwrap();
                (name.to_string(), desc.to_string())
            })
            .zip(
                map.remove("parmtype")
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| {
                        let mut i = s.splitn(2, ':');
                        // These unwraps *should* be okay, if not it means .modinfo is incorrect
                        i.next().unwrap();
                        let typ = i.next().unwrap();
                        typ.to_string()
                    }),
            )
        {
            parameters.push(ModParam {
                name,
                description,
                type_,
            })
        }
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

    /// Module Signature info, if any.
    // FIXME: rust-openssl does not expose the APIs we need, so this isn't possible.
    fn _signature(&self) -> Option<ModSig> {
        let f = fs::read(&self.path).unwrap();
        if f.ends_with(SIGNATURE_MAGIC) {
            // Length of file, minus the signature structure, minus the magic
            let len = f.len() - size_of::<RawModSig>() - SIGNATURE_MAGIC.len();
            //
            let sig: &[u8] = &f[len..];
            let mut sig = unsafe { (sig.as_ptr() as *const RawModSig).read_unaligned() };
            sig.signature_length = u32::from_be(sig.signature_length);
            dbg!(sig);
            //
            let data_start = len - sig.signature_length as usize;
            let _sig_data: &[u8] = &f[data_start..][..sig.signature_length as usize];
            //
            todo!()
        } else {
            None
        }
    }

    /// Decompresses a kernel module
    ///
    /// Returns `data` unchanged if not compressed.
    fn decompress(&self, data: Vec<u8>) -> Result<Vec<u8>> {
        let mut v = Vec::new();
        let ext = self
            .path
            .extension()
            .ok_or(ModuleError::InvalidModule(INVALID_EXTENSION.into()))?;
        if ext == "xz" {
            let mut data = std::io::BufReader::new(data.as_slice());
            // FIXME: Write own xz library with an actual error type?
            xz_decompress(&mut data, &mut v).map_err(|e| {
                ModuleError::InvalidModule(match e {
                    lzma_rs::error::Error::LZMAError(s) => s,
                    lzma_rs::error::Error::XZError(s) => s,
                    lzma_rs::error::Error::IOError(s) => s.to_string(),
                })
            })?;
            Ok(v)
        } else if ext == "gz" {
            let mut data = GzDecoder::new(data.as_slice());
            data.read_to_end(&mut v)
                .map_err(|e| ModuleError::InvalidModule(e.to_string()))?;
            Ok(v)
        } else if ext == "ko" {
            return Ok(data);
        } else {
            return Err(ModuleError::InvalidModule(COMPRESSION.into()));
        }
    }
}

/// Information on the signature added to the end of the module
///
/// See `linux/include/linux/module_signature.h` for some details.
#[derive(Debug, Copy, Clone)]
#[repr(C)]
struct RawModSig {
    /// Public-key crypto algorithm
    algorithm: u8,

    // Digest hash
    hash: u8,

    // Key type
    id_type: u8,

    // Length of signer name
    signer_length: u8,

    // Length of key
    key_id_length: u8,

    _pad: [u8; 3],

    // Length of signature IN BIG ENDIAN
    signature_length: u32,
}

#[derive(Debug)]
struct ModSig {
    signer: String,
}

#[derive(Debug)]
pub struct ModParam {
    pub name: String,
    pub description: String,
    pub type_: String,
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
