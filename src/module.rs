//! Interface to Linux Kernel modules, at runtime
use std::{
    ffi::CString,
    fs::{self, DirEntry},
    io,
    num::ParseIntError,
    path::{Path, PathBuf},
};

use nix::kmod::{delete_module, DeleteModuleFlags};

mod error {
    use core::result;
    use std::{error, fmt, io};

    use super::*;

    pub type Result<T, E = ModuleError> = result::Result<T, E>;

    #[derive(Debug)]
    #[non_exhaustive]
    pub enum ModuleError {
        /// The Kernel returned an error
        Kernel(io::Error),

        /// Invalid UTF-8
        InvalidUTF8(PathBuf),
    }

    impl fmt::Display for ModuleError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                ModuleError::Kernel(_) => write!(f, "kernel error"),
                ModuleError::InvalidUTF8(p) => {
                    write!(f, "invalid UTF-8 in `{}`", p.display())
                }
            }
        }
    }

    impl error::Error for ModuleError {
        fn source(&self) -> Option<&(dyn error::Error + 'static)> {
            match self {
                ModuleError::Kernel(e) => Some(e),
                _ => None,
            }
        }
    }

    #[derive(Debug)]
    #[non_exhaustive]
    pub enum FromNameError {
        /// The Kernel returned an error
        Kernel(io::Error),
    }

    impl fmt::Display for FromNameError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Kernel(_) => write!(f, "kernel error"),
            }
        }
    }

    impl error::Error for FromNameError {
        fn source(&self) -> Option<&(dyn error::Error + 'static)> {
            match self {
                Self::Kernel(e) => Some(e),
            }
        }
    }

    #[derive(Debug)]
    #[non_exhaustive]
    pub enum ModuleAttrError {
        /// The Kernel returned an error
        Kernel(io::Error),

        /// Attribute data was invalid or unexpected
        InvalidData(&'static str, String),

        /// Attribute was missing
        ///
        /// This is only returned if its expected for the attribute to be
        /// optional.
        ///
        /// Otherwise, it is considered a [`ModuleAttrError::Kernel`] error
        Missing(&'static str),
    }

    impl fmt::Display for ModuleAttrError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Kernel(_) => write!(f, "kernel error"),
                Self::InvalidData(n, d) => write!(f, "{n}: invalid data `{d}`"),
                Self::Missing(n) => write!(f, "Missing attribute {n}"),
            }
        }
    }

    impl error::Error for ModuleAttrError {
        fn source(&self) -> Option<&(dyn error::Error + 'static)> {
            match self {
                Self::Kernel(e) => Some(e),
                _ => None,
            }
        }
    }

    #[derive(Debug)]
    pub enum ModuleFromPathError {
        /// Missing module name
        MissingName(PathBuf),

        /// Invalid UTF-8
        InvalidUTF8(PathBuf),

        /// Module not found
        NotFound(PathBuf),

        /// Module exists, but is built-in
        BuiltIn(PathBuf),
    }

    impl fmt::Display for ModuleFromPathError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::MissingName(p) => write!(f, "missing module name in `{}`", p.display()),

                Self::InvalidUTF8(p) => write!(f, "invalid UTF-8 in `{}`", p.display()),

                Self::NotFound(p) => write!(f, "module not found at `{}`", p.display()),

                Self::BuiltIn(p) => write!(f, "module `{}` exists, but is built-in", p.display()),
            }
        }
    }

    impl error::Error for ModuleFromPathError {}
}

pub use error::{FromNameError, ModuleAttrError, ModuleError};
use error::{ModuleFromPathError, Result};

mod imp {
    use super::*;

    /// Helper to read the `attribute` at `path`. Trims it.
    pub(crate) fn read_attribute<P: AsRef<Path>>(
        base: P,
        attribute: &'static str,
    ) -> Result<String, io::Error> {
        fs::read_to_string(base.as_ref().join(attribute)).map(|s| s.trim().to_owned())
    }
}
use imp::*;

mod io_ {
    use std::{
        fs::File,
        io::{BufReader, BufWriter, Read, Write},
    };

    use super::*;

    /// [`Module`] Parameter
    ///
    /// This type implements [`Read`] and [`Write`], and is buffered.
    #[derive(Debug)]
    pub struct ModuleParam {
        name: String,
        path: PathBuf,
    }

    impl ModuleParam {
        /// New module parameter at `path`
        ///
        /// # Errors
        ///
        /// - [`ModuleFromDirError::MissingName`] if `path` does not end in a
        ///   valid module name
        /// - [`ModuleFromDirError::InvalidUTF8`] if `path` contains invalid
        ///   UTF-8
        pub(crate) fn new(path: PathBuf) -> Result<Self, ModuleFromPathError> {
            let name = path
                .file_name()
                .ok_or_else(|| ModuleFromPathError::MissingName(path.to_path_buf()))?
                .to_str()
                .ok_or_else(|| ModuleFromPathError::InvalidUTF8(path.to_path_buf()))?
                .into();
            Ok(Self { name, path })
        }

        /// Parameter name
        pub fn name(&self) -> &str {
            &self.name
        }

        /// System path to module parameter
        ///
        /// # Example
        ///
        /// `<SYSFS>/module/name`
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Read for ModuleParam {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            BufReader::new(File::open(self.path())?).read(buf)
        }
    }

    impl Write for ModuleParam {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut b = BufWriter::new(File::options().write(true).open(self.path())?);
            let ret = b.write(buf)?;
            b.flush()?;
            Ok(ret)
        }

        fn flush(&mut self) -> io::Result<()> {
            // Flushing is inapplicable to module parameters,
            // and we always flush after write anyway
            Ok(())
        }
    }
}
pub use io_::ModuleParam;

use crate::util::SYSFS_PATH;

/// Kernel modules can be "tainted", which serve as a marker for debugging
/// purposes.
// See `module.h`, `panic.h`, and `panic.c` for taint flags, ctrl+f `taint_flags`
// See <https://www.kernel.org/doc/html/latest/admin-guide/sysctl/kernel.html#tainted>
#[derive(Debug, Clone, Copy)]
pub enum Taint {
    /// Not tainted
    Clean,

    /// Proprietary Module.
    Proprietary,

    /// Module was force loaded.
    Forced,

    /// Out of tree, third party Module.
    OutOfTree,

    /// Unstable Staging Module.
    Staging,

    /// Unsigned Module.
    Unsigned,

    /// Module has been live-patched?
    LivePatch,

    /// Auxiliary taint, defined and used by distros
    Aux,

    /// Module built with struct randomization plugin?
    RandStruct,

    /// Test taint ~~please ignore~~
    Test,
}

/// Module Init State
#[derive(Debug, Clone)]
pub enum State {
    /// Normal state, fully loaded.
    Live,

    /// Running module init
    Coming,

    /// Going away
    Going,
}

/// A Linux Kernel Module
#[derive(Debug)]
pub struct Module {
    /// Module name
    name: String,

    /// Kernel path to module information
    ///
    /// # Example
    ///
    /// `<SYSFS>/module/name`
    path: PathBuf,
}

// Constructors
impl Module {
    /// Get all currently loaded modules
    ///
    /// # Errors
    ///
    /// - [`ModuleError::Kernel`] if the kernel gives an error
    /// - [`ModuleError::InvalidUTF8`] if invalid UTF-8 is encountered in any
    ///   system paths
    ///
    /// # Implementation
    ///
    /// ## Linux
    ///
    /// This will look for modules under `/sys/modules`.
    ///
    /// Built-in modules will not be returned, even if they exist.
    pub fn loaded() -> Result<Vec<Self>> {
        let dir = Path::new(SYSFS_PATH).join("module");
        let mut mods = Vec::new();

        for module in fs::read_dir(dir).map_err(ModuleError::Kernel)? {
            let module: DirEntry = module.map_err(ModuleError::Kernel)?;
            let m = Self::from_path(&module.path());
            let m = match m {
                Ok(m) => m,
                Err(e) => match e {
                    // Ignore built-in modules
                    ModuleFromPathError::BuiltIn(_) => continue,

                    // TODO: Figure out kernel policy/support for module names.
                    ModuleFromPathError::InvalidUTF8(p) => return Err(ModuleError::InvalidUTF8(p)),

                    // Shouldn't happen? Maybe if a module is unloaded while we're scanning?
                    // In that case, skip it.
                    // TODO: Return Vec of Results, allow per-module permissions?
                    ModuleFromPathError::NotFound(_) => continue,

                    // This error shouldn't be possible, given we're iterating the directory
                    ModuleFromPathError::MissingName(_) => unreachable!(),
                },
            };

            mods.push(m);
        }
        Ok(mods)
    }

    /// Get an already loaded module by `name`
    ///
    /// # Errors
    ///
    /// - [`ModuleError::Kernel`] if the kernel gives an error
    /// - [`ModuleError::Kernel`] If `name` was not a valid module
    ///   - On Linux, sometimes modules in `/sys/modules` are not actually
    ///     modules, but built-ins, if they have parameters or a version
    /// - [`ModuleError::Kernel`] If no module `name` is found
    pub fn from_name(name: &str) -> Result<Self, FromNameError> {
        let path = Path::new(SYSFS_PATH).join("module").join(name);

        let m = Self::from_path(&path);
        match m {
            Ok(m) => Ok(m),
            Err(e) => match e {
                // Built-in modules are invalid
                ModuleFromPathError::BuiltIn(_) => Err(FromNameError::Kernel(io::Error::new(
                    io::ErrorKind::InvalidData,
                    e,
                ))),

                // Missing modules are invalid
                ModuleFromPathError::NotFound(_) => Err(FromNameError::Kernel(io::Error::new(
                    io::ErrorKind::NotFound,
                    e,
                ))),

                // This error shouldn't be possible, given we provide the name/path
                ModuleFromPathError::InvalidUTF8(_) => unreachable!(),

                // This error shouldn't be possible, given we provide the name
                ModuleFromPathError::MissingName(_) => unreachable!(),
            },
        }
    }
}

// Operations
impl Module {
    /// Unload this [`Module`]
    ///
    /// Will only succeed if [`Module::ref_count`] is zero
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error unloading the module
    ///
    /// # Implementation
    ///
    /// ## Linux
    ///
    /// `O_NONBLOCK` is always specified
    pub fn unload(self) -> Result<()> {
        delete_module(
            &CString::new(self.name.as_str()).expect("Module name had null bytes"),
            DeleteModuleFlags::O_NONBLOCK,
        )
        .map_err(|e| io::Error::from_raw_os_error(e as i32))
        .map_err(ModuleError::Kernel)?;
        Ok(())
    }

    /// Forcefully unload this [`Module`]
    ///
    /// # Safety
    ///
    /// Force unloading is extremely dangerous, and will taint your kernel.
    ///
    /// It can cause modules to be unloaded while still in use, or unload
    /// modules not designed to be unloaded.
    /// It can cause serious system instability and memory errors.
    ///
    /// If the kernel was not built with `MODULE_FORCE_UNLOAD`, forcing is
    /// ignored.
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error unloading the module
    ///
    /// # Implementation
    ///
    /// ## Linux
    ///
    /// `O_NONBLOCK` and `O_TRUNC` are always specified
    pub unsafe fn force_unload(self) -> Result<()> {
        delete_module(
            &CString::new(self.name.as_str()).expect("Module name had null bytes"),
            DeleteModuleFlags::O_NONBLOCK | DeleteModuleFlags::O_TRUNC,
        )
        .map_err(|e| io::Error::from_raw_os_error(e as i32))
        .map_err(ModuleError::Kernel)?;

        Ok(())
    }
}

// Attributes
impl Module {
    /// Name of the module
    pub fn name(&self) -> &str {
        &self.name
    }

    /// System path to the module
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Module reference count
    ///
    /// Treats the kernel not being compiled with `MODULE_UNLOAD` the same
    /// as having a zero ref_count.
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error other reading the attribute
    /// - [`ModuleAttrError::InvalidData`] on invalid attribute data
    pub fn ref_count(&self) -> Result<u32, ModuleAttrError> {
        match read_attribute(&self.path, "refcnt") {
            Ok(s) => Ok(s.parse().map_err(|e: ParseIntError| {
                ModuleAttrError::InvalidData("refcnt", e.to_string())
            })?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(ModuleAttrError::Kernel(e)),
        }
    }

    /// Module core section size in bytes
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error reading the attribute
    /// - [`ModuleAttrError::InvalidData`] on invalid attribute data
    pub fn core_size(&self) -> Result<u64, ModuleAttrError> {
        read_attribute(&self.path, "coresize")
            .map_err(ModuleAttrError::Kernel)?
            .parse()
            .map_err(|e: ParseIntError| ModuleAttrError::InvalidData("coresize", e.to_string()))
    }

    /// Module init section size in bytes
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error reading the attribute
    /// - [`ModuleAttrError::InvalidData`] on invalid attribute data
    pub fn init_size(&self) -> Result<u64, ModuleAttrError> {
        read_attribute(&self.path, "initsize")
            .map_err(ModuleAttrError::Kernel)?
            .parse()
            .map_err(|e: ParseIntError| ModuleAttrError::InvalidData("initsize", e.to_string()))
    }

    /// Per-module taint flags
    ///
    /// See [`Taint`] for details.
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error reading the attribute
    /// - [`ModuleAttrError::InvalidData`] on unexpected [`Taint`] flags
    pub fn taint(&self) -> Result<Taint, ModuleAttrError> {
        // TODO: Find out if a module can have *multiple* taint flags?
        match &*read_attribute(&self.path, "taint").map_err(ModuleAttrError::Kernel)? {
            "P" => Ok(Taint::Proprietary),
            "O" => Ok(Taint::OutOfTree),
            "F" => Ok(Taint::Forced),
            "C" => Ok(Taint::Staging),
            "E" => Ok(Taint::Unsigned),
            "K" => Ok(Taint::LivePatch),
            "X" => Ok(Taint::Aux),
            "T" => Ok(Taint::RandStruct),
            "N" => Ok(Taint::Test),
            "" => Ok(Taint::Clean),
            d => Err(ModuleAttrError::InvalidData("taint", d.into())),
        }
    }

    /// Module version information
    ///
    /// Returns [`None`] if kernel was not compiled with `MODULE_VERSION`
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error reading the attribute
    pub fn version(&self) -> Result<Option<String>, ModuleAttrError> {
        match read_attribute(&self.path, "version") {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ModuleAttrError::Kernel(e)),
        }
    }

    /// Module source checksum
    ///
    /// Returns [`None`] if kernel was not compiled with `MODULE_VERSION`
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error reading the attribute
    pub fn checksum(&self) -> Result<Option<String>, ModuleAttrError> {
        match read_attribute(&self.path, "srcversion") {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ModuleAttrError::Kernel(e)),
        }
    }

    /// Module init state
    ///
    /// See [`State`] for details
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error reading the attribute
    /// - [`ModuleAttrError::InvalidData`] on an unexpected [`State`]
    pub fn state(&self) -> Result<State, ModuleAttrError> {
        match &*read_attribute(&self.path, "initstate").map_err(ModuleAttrError::Kernel)? {
            "live" => Ok(State::Live),
            "coming" => Ok(State::Coming),
            "going" => Ok(State::Going),
            s => Err(ModuleAttrError::InvalidData("initstate", s.into())),
        }
    }

    /// Module parameters
    ///
    /// Module parameters are exposed, generically and un-typed, within
    /// `<SYSFS>/modules/<NAME>/parameters/`
    ///
    /// The format and stability of parameters depends entirely on the specific
    /// module
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error reading the module parameters
    pub fn parameters(&self) -> Result<Vec<io_::ModuleParam>, ModuleAttrError> {
        let mut vec = Vec::new();
        let path = self.path.join("parameters");

        if path.try_exists().map_err(ModuleAttrError::Kernel)? {
            for entry in fs::read_dir(path).map_err(ModuleAttrError::Kernel)? {
                let entry: DirEntry = entry.map_err(ModuleAttrError::Kernel)?;

                vec.push(io_::ModuleParam::new(entry.path()).map_err(|e| {
                    ModuleAttrError::Kernel(io::Error::new(io::ErrorKind::InvalidData, e))
                })?);
            }
        }

        Ok(vec)
    }

    /// List of other loaded [`Module`]s that are currently using/referencing
    /// this one
    ///
    /// # Errors
    ///
    /// - [`ModuleAttrError::Kernel`] on any error reading the module parameters
    // See `sysfs.c` for details, ctrl+f `holders_dir`
    pub fn holders(&self) -> Result<Vec<Module>, ModuleAttrError> {
        let mut v = Vec::new();
        for re in fs::read_dir(self.path.join("holders")).map_err(ModuleAttrError::Kernel)? {
            let re: DirEntry = re.map_err(ModuleAttrError::Kernel)?;
            let m = Module::from_path(&re.path());
            let m = match m {
                Ok(m) => m,
                Err(e) => match e {
                    // TODO: Figure out kernel policy/support for module names.
                    e @ ModuleFromPathError::InvalidUTF8(_) => {
                        return Err(ModuleAttrError::Kernel(io::Error::new(
                            io::ErrorKind::InvalidData,
                            e,
                        )))
                    }

                    // Shouldn't happen? Maybe if a module is unloaded while we're scanning?
                    // In that case, skip it.
                    // TODO: Return Vec of Results, allow per-module permissions?
                    ModuleFromPathError::NotFound(_) => continue,

                    // Should be impossible
                    ModuleFromPathError::BuiltIn(_) => unreachable!(),

                    // Shouldn't be possible, given we're iterating the directory
                    ModuleFromPathError::MissingName(_) => unreachable!(),
                },
            };
            v.push(m);
        }
        Ok(v)
    }
}

// Private
impl Module {
    /// Create a [`Module`] from the given system path
    ///
    /// # Example Path
    ///
    /// `/sys/module/loop`
    ///
    /// # Errors
    ///
    /// - [`ModuleFromDirError::MissingName`] if `path` does not end in a valid
    ///   module name
    /// - [`ModuleFromDirError::InvalidUTF8`] if `path` contains invalid UTF-8
    /// - [`ModuleFromDirError::NotFound`] if the module at `path` does not
    ///   exist
    /// - [`ModuleFromDirError::BuiltIn`] if the module is built-in
    ///
    /// # Implementation
    ///
    /// ## Linux
    ///
    /// The Linux kernel currently puts some built-in modules with dynamically
    /// loaded ones.
    ///
    /// A module is considered built-in if the `coresize` attribute does not
    /// exist.
    ///
    /// All module names seem to have `-` dashes in their name replaced with `_`
    /// underscores in system paths. This is consistent with `modprobe`.
    ///
    /// Only UTF-8 paths and module names are supported.
    fn from_path(path: &Path) -> Result<Self, ModuleFromPathError> {
        // TODO: Theres no reason we should have to do this at all just to replace
        // `-` with `_`, except for lack of APIs.
        let name = path
            .file_name()
            .ok_or_else(|| ModuleFromPathError::MissingName(path.to_path_buf()))?
            .to_str()
            .ok_or_else(|| ModuleFromPathError::InvalidUTF8(path.to_path_buf()))?;

        // `/sys/modules` seems to always use `_` in paths?
        let path = path.with_file_name(name.replace('-', "_"));

        // Error if module doesn't exist
        if !path.exists() {
            return Err(ModuleFromPathError::NotFound(path));
        }

        // Error if built-in
        if !path.join("coresize").exists() {
            return Err(ModuleFromPathError::BuiltIn(path));
        }

        let stem = path
            .file_stem()
            .ok_or_else(|| ModuleFromPathError::MissingName(path.to_path_buf()))?
            .to_str()
            .ok_or_else(|| ModuleFromPathError::InvalidUTF8(path.to_path_buf()))?
            .trim()
            .to_owned();

        let s = Self { name: stem, path };
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    #![allow(unreachable_code, unused_variables)]
    use anyhow::Result;

    use super::*;

    #[test]
    fn test_name() -> Result<()> {
        let mods = Module::loaded()?;

        dbg!(&mods);

        Ok(())
    }

    /// Test that we can successfully list all modules and attributes on the
    /// current system
    #[test]
    fn all_modules() -> Result<()> {
        let mods = Module::loaded()?;

        for m in mods {
            let name = m.name();
            let core_size = m.core_size()?;
            let init_size = m.init_size()?;
            let taint = m.taint()?;
            let ref_count = m.ref_count()?;
            let ver = m.version()?;
            let check = m.checksum()?;
            let state = m.state()?;
            let params = m.parameters()?;
            let holders = m.holders()?;
            let nm = Module::from_name(name)?;
        }
        Ok(())
    }
}
