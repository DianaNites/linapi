//! Interface to Linux Kernel modules, on disk
use std::{
    collections::HashMap,
    convert::TryInto,
    ffi::CString,
    fs,
    io::{self, Read},
    mem::size_of,
    path::{Path, PathBuf},
    str::from_utf8,
};

use cms::{
    cert::x509::spki::AlgorithmIdentifier,
    content_info::ContentInfo,
    signed_data::{SignedData, SignerIdentifier, SignerInfo},
};
use der::{
    oid::db::{
        rfc5912::{ID_SHA_1, ID_SHA_224, ID_SHA_256, ID_SHA_384, ID_SHA_512},
        rfc6268::ID_SIGNED_DATA,
    },
    Decode,
    Encode,
};
use elf::{endian::AnyEndian, ElfBytes};
#[cfg(feature = "gz")]
use flate2::bufread::GzDecoder;
use nix::kmod::{finit_module, init_module, ModuleInitFlags};
use walkdir::WalkDir;
#[cfg(feature = "xz")]
use xz2::bufread::XzDecoder;
#[cfg(feature = "zst")]
use zstd::stream::read::Decoder as ZstDecoder;

use crate::{
    extensions::FileExt,
    module::{FromNameError as ModuleFromNameError, Module},
    system::kernel_info,
    util::MODULE_PATH,
};

const SIGNATURE_MAGIC: &[u8] = b"~Module signature appended~\n";

/// Valid/known kernel module compression schemes
const VALID_COMPRESSION: &[&str] = &["xz", "zst", "gz"];

mod imp {
    /// Helper type to treat `-` and `_` as equal
    #[derive(Debug, Eq)]
    #[repr(transparent)]
    pub struct EqEqualUnderDash<'a>(pub &'a str);

    impl<'a> PartialEq for EqEqualUnderDash<'a> {
        fn eq(&self, other: &Self) -> bool {
            self.0.len() == other.0.len()
                && self
                    .0
                    .bytes()
                    .zip(other.0.bytes())
                    .all(|(b1, b2)| b1 == b2 || b1 == b'-' || b1 == b'_')
        }
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct RawModuleSignature {
        /// Public-key algorithm
        ///
        /// Currently, only `0`
        pub algo: u8,

        /// Hash Digest algorithm
        ///
        /// Currently, only `0`
        pub hash: u8,

        /// Key type
        ///
        /// Currently, only `2`, representing PKCS#7
        pub id_type: u8,

        /// Length of key signers name
        ///
        /// Currently, only `0`
        pub signer_len: u8,

        /// Length of key ID
        ///
        /// Currently, only `0`
        pub key_id_len: u8,

        /// Padding
        pub __pad: [u8; 3],

        /// Big endian 32-bit signature length
        pub sig_len: u32,
    }

    impl RawModuleSignature {
        /// Check if this signature is valid
        ///
        /// A signature is valid if we recognize every value of its fields
        pub fn valid(&self) -> bool {
            self.algo == 0
                && self.hash == 0
                && self.id_type == 2
                && self.signer_len == 0
                && self.key_id_len == 0
                && self.__pad == [0; 3]
        }
    }
}
use imp::{EqEqualUnderDash, RawModuleSignature};

mod error {
    use core::result;
    use std::{error, fmt, io};

    use super::*;

    pub type Result<T, E = ()> = result::Result<T, E>;

    #[derive(Debug)]
    pub enum FromPathError {
        /// Invalid UTF-8
        InvalidUTF8(PathBuf),

        /// Missing module name
        MissingName(PathBuf),

        /// There was an error reading the module
        Module(ModInfoError),
    }

    impl fmt::Display for FromPathError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InvalidUTF8(p) => write!(f, "invalid UTF-8 in `{}`", p.display()),
                Self::MissingName(p) => write!(f, "missing module name in `{}`", p.display()),
                Self::Module(_) => write!(f, "an error reading the module"),
            }
        }
    }

    impl error::Error for FromPathError {
        fn source(&self) -> Option<&(dyn error::Error + 'static)> {
            match self {
                Self::Module(e) => Some(e),
                _ => None,
            }
        }
    }

    #[derive(Debug)]
    pub enum FromNameError {
        /// Module was not found
        NotFound(String),

        /// Module was invalid
        InvalidModule(String),
    }

    impl fmt::Display for FromNameError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::NotFound(s) => write!(f, "module {} not found", s),
                Self::InvalidModule(s) => write!(f, "module `{}` was invalid", s),
            }
        }
    }

    impl error::Error for FromNameError {}

    #[derive(Debug)]
    pub enum ModInfoError {
        /// Module was invalid or corrupt
        InvalidModule(String),

        /// Module signature information was invalid
        InvalidSignature,

        /// Module signature seemed valid, but we dont support it
        UnsupportedSignature,

        /// Module parameter information was corrupt
        InvalidParameter,

        /// Error decompressing module
        Compression(String),

        /// Missing `.modinfo` section
        MissingInfo,

        /// Invalid UTF-8 in `.modinfo` `tag=value`
        InvalidUtf8(String, String),
    }

    impl fmt::Display for ModInfoError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InvalidModule(s) => write!(f, "module invalid or corrupt: {s}"),
                Self::MissingInfo => write!(f, "module missing `.modinfo` section"),
                Self::Compression(e) => write!(f, "error decompressing module: {e}"),
                Self::InvalidSignature => write!(f, "module signature invalid"),
                Self::UnsupportedSignature => {
                    write!(f, "module signature seemed valid, but we dont support it")
                }
                Self::InvalidParameter => write!(f, "module parameter information was corrupt"),
                Self::InvalidUtf8(s, c) => {
                    write!(f, "invalid UTF-8 in tag or value: `{s}` context: {c}")
                }
            }
        }
    }

    impl error::Error for ModInfoError {}

    #[derive(Debug)]
    pub enum DecompressError<'a> {
        /// Failed to decompress
        Compression(&'a Path, String),

        /// Unknown or unsupported compression
        Unsupported(&'a Path),
    }

    impl<'a> fmt::Display for DecompressError<'a> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Compression(p, e) => {
                    write!(f, "Error decompressing module {}: {}", p.display(), e)
                }
                Self::Unsupported(_) => write!(f, "unknown or unsupported compression"),
            }
        }
    }

    impl<'a> error::Error for DecompressError<'a> {}

    #[derive(Debug)]
    pub enum LoadError {
        /// An error occurred trying to read the module
        Io(io::Error),

        /// The kernel or module returned an error during (de)initialization
        Module(io::Error),
    }

    impl fmt::Display for LoadError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Io(_) => write!(f, "an error occurred trying to read the module"),
                Self::Module(_) => {
                    write!(f, "the module returned an error during (de)initialization")
                }
            }
        }
    }

    impl error::Error for LoadError {
        fn source(&self) -> Option<&(dyn error::Error + 'static)> {
            match self {
                LoadError::Io(e) => Some(e),
                LoadError::Module(e) => Some(e),
            }
        }
    }
}
use error::{DecompressError, Result};
pub use error::{FromNameError, FromPathError, LoadError, ModInfoError};

/// A parameter type in a [`ModuleFile`]'s [`ModInfo`]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ParameterType {
    Byte,

    HexInt,

    Short,

    UnsignedShort,

    Int,

    UnsignedInt,

    Long,

    UnsignedLong,

    CharPointer,

    /// Accepted `true/false` values:
    ///
    /// - `0`/`1`?
    ///   - Its in this order in the docs, and C commonly treats 0 as true,
    ///     but.. ?? As it's ambiguous, prefer the other options.
    /// - `y`/`n`
    /// - `Y`/`N`
    Bool,

    /// The same as `bool`, but N = true
    InvBool,

    /// The type was not specified
    Unknown,

    String,

    UnsignedLongLong,

    /// The type was specified, but we don't recognize it
    Custom(String),
}

impl ParameterType {
    fn parse(s: &str) -> Self {
        // The standard types are, per `linux/moduleparam.h`:
        //
        // - `byte`
        // - `hexint`
        // - `short`
        // - `ushort`
        // - `int`
        // - `uint`
        // - `long`
        // - `ulong`
        // - `charp` Character pointer
        // - `bool` values `0`/`1`, `y`/`n`, and `Y`/`N`
        // - `invbool` The same as `bool`, but N = true
        //
        // Non-standard types we know about are
        // - `string`
        // - `ullong`
        match s {
            "byte" => Self::Byte,
            "hexint" => Self::HexInt,
            "short" => Self::Short,
            "ushort" => Self::UnsignedShort,
            "int" => Self::Int,
            "uint" => Self::UnsignedInt,
            "long" => Self::Long,
            "ulong" => Self::UnsignedLong,
            "charp" => Self::CharPointer,
            "bool" => Self::Bool,
            "invbool" => Self::InvBool,
            "string" => Self::String,
            "ullong" => Self::UnsignedLongLong,
            s => Self::Custom(s.into()),
        }
    }
}

impl std::fmt::Display for ParameterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Byte => write!(f, "Byte"),
            Self::HexInt => write!(f, "HexInt"),
            Self::Short => write!(f, "Short"),
            Self::UnsignedShort => write!(f, "UnsignedShort"),
            Self::Int => write!(f, "Int"),
            Self::UnsignedInt => write!(f, "UnsignedInt"),
            Self::Long => write!(f, "Long"),
            Self::UnsignedLong => write!(f, "UnsignedLong"),
            Self::CharPointer => write!(f, "CharPointer"),
            Self::Bool => write!(f, "Bool"),
            Self::InvBool => write!(f, "InvBool"),
            Self::Unknown => write!(f, "Unknown"),
            Self::Custom(s) => write!(f, "Custom({s})"),
            Self::String => write!(f, "String"),
            Self::UnsignedLongLong => write!(f, "UnsignedLongLong"),
        }
    }
}

/// A parameter in a [`ModuleFile`]'s [`ModInfo`]
#[derive(Debug, Clone)]
pub struct ModuleParameter {
    /// Parameter name
    name: String,

    /// Parameter type
    ///
    /// See `module_param` in `linux/moduleparam.h` for details
    ty: ParameterType,

    /// Parameter description
    description: String,
}

impl ModuleParameter {
    /// Parameter name
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    /// Parameter type
    pub fn ty(&self) -> &ParameterType {
        &self.ty
    }

    /// Parameter description
    pub fn description(&self) -> &str {
        &self.description
    }
}

/// A Linux Kernel Module file on disk.
#[derive(Debug)]
pub struct ModuleFile {
    /// Module Name
    name: String,

    /// Module system path
    path: PathBuf,

    /// Module Info
    info: ModInfo,
}

// Constructors
impl ModuleFile {
    /// Search `/lib/modules/(uname -r)` for the module `name`.
    ///
    /// Underscore `_` and dash `-` are treated as identical in `name`
    ///
    /// # Errors
    ///
    /// Errors while searching are ignored
    pub fn from_name(name: &str) -> Result<Self, FromNameError> {
        Self::from_name_with_uname(name, kernel_info().release())
    }

    /// Search `/lib/modules/<uname>` for the module `name`
    ///
    /// Underscore `_` and dash `-` are treated as identical in `name`
    ///
    /// # Errors
    ///
    /// Errors while searching are ignored
    pub fn from_name_with_uname(name: &str, uname: &str) -> Result<Self, FromNameError> {
        let path = Path::new(MODULE_PATH).join(uname);
        for entry in WalkDir::new(path) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_file() {
                continue;
            }
            let ext = Path::new(entry.file_name())
                .extension()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();

            // Skip files that aren't modules or *potential* compressed modules
            if !(VALID_COMPRESSION.contains(&ext) || ext == "ko") {
                continue;
            }

            let m_name = entry
                .path()
                .file_stem()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();

            let (m_name, ext) = m_name.split_once('.').unwrap_or((m_name, ext));

            // Skip not modules
            if ext != "ko" {
                continue;
            }

            if EqEqualUnderDash(m_name) == EqEqualUnderDash(name) {
                return Ok(Self {
                    name: name.into(),
                    info: match ModInfo::read(entry.path()) {
                        Ok(m) => m,
                        Err(_) => continue,
                    },
                    path: entry.into_path(),
                });
            }
        }
        Err(FromNameError::NotFound(name.into()))
    }

    /// Create a [`ModuleFile`] from the module at `path`s
    ///
    /// # Errors
    ///
    /// - [`FromPathError::MissingName`] if `path` is missing a file name
    /// - [`FromPathError::InvalidUTF8`] if `path` contains invalid UTF-8
    /// - [`FromPathError::Module`] if reading the module fails
    pub fn from_path(path: &Path) -> Result<Self, FromPathError> {
        let _name = path
            .file_name()
            .ok_or_else(|| FromPathError::MissingName(path.to_path_buf()))?
            .to_str()
            .ok_or_else(|| FromPathError::InvalidUTF8(path.to_path_buf()))?;

        Ok(Self {
            name: name.into(),
            path: path.to_path_buf(),
            info: ModInfo::read(path).map_err(FromPathError::Module)?,
        })
    }
}

// Operations
impl ModuleFile {
    /// Load this kernel module, and return the [`Module`] describing it.
    ///
    /// This requires the `CAP_SYS_MODULE` capability
    ///
    /// # Arguments
    ///
    /// - `parameter`s for the kernel module. See module documentation for
    ///   details, and `init_module(2)` for details on formatting.
    ///
    /// # Errors
    ///
    /// - [`LoadError::Io`] if theres an error reading the module
    /// - [`LoadError::Module`] if the kernel returns an error loading the
    ///   module
    ///
    /// # Panics
    ///
    /// - if `parameter` has any internal nul bytes.
    pub fn load(&self, parameter: &str) -> Result<Module, LoadError> {
        let img = fs::read(self.path()).map_err(LoadError::Io)?;

        match init_module(
            &img,
            &CString::new(parameter).expect("parameter can't have internal null bytes"),
        ) {
            Ok(()) => (),
            Err(e) => return Err(LoadError::Module(io::Error::from_raw_os_error(e as i32))),
        };

        match Module::from_name(&self.name) {
            Ok(m) => Ok(m),
            Err(ModuleFromNameError::Kernel(e)) => Err(LoadError::Module(e)),
        }
    }

    /// Force load this kernel module, and return the [`Module`]
    /// describing it.
    ///
    /// See [`ModuleFile::load`] for more details.
    ///
    /// # Safety
    ///
    /// Force loading a kernel module is extremely dangerous, it skips important
    /// safety checks that help ensure module compatibility with your
    /// kernel.
    // #[cfg(no)]
    pub unsafe fn force_load(&self, parameter: &str) -> Result<Module, LoadError> {
        let mut file = fs::File::create_memory("decompressed module");
        io::copy(
            &mut fs::File::open(self.path()).map_err(LoadError::Io)?,
            &mut file,
        )
        .map_err(LoadError::Io)?;

        match finit_module(
            &file,
            &CString::new(parameter).expect("param can't have internal null bytes"),
            ModuleInitFlags::MODULE_INIT_IGNORE_MODVERSIONS
                | ModuleInitFlags::MODULE_INIT_IGNORE_VERMAGIC,
        ) {
            Ok(()) => (),
            Err(e) => return Err(LoadError::Module(io::Error::from_raw_os_error(e as i32))),
        };

        match Module::from_name(&self.name) {
            Ok(m) => Ok(m),
            Err(ModuleFromNameError::Kernel(e)) => Err(LoadError::Module(e)),
        }
    }
}

// Attributes
impl ModuleFile {
    /// Path to module file
    ///
    /// May not exist or be the same file as initially opened.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Embedded module information
    pub fn info(&self) -> &ModInfo {
        &self.info
    }
}

/// Information on a [`ModuleFile`]
// See `modules.h` and `modules/main.c` for details on the format
#[derive(Debug)]
pub struct ModInfo {
    /// Module Aliases. Alternative names for this module.
    alias: Vec<String>,

    /// Soft Dependencies. Not required, but may provide additional features.
    soft_dependencies: Vec<String>,

    /// Module License
    ///
    /// See `MODULE_LICENSE` for details on this value.
    license: String,

    /// Module Author and email
    authors: Vec<String>,

    /// What the module does
    description: String,

    /// Module version
    version: String,

    /// Optional firmware file(s) needed by the module
    firmware: Vec<String>,

    /// Version magic string, used by the kernel for compatibility checking.
    version_magic: String,

    /// Module name, self-reported.
    name: String,

    /// Whether the module is from the kernel source tree.
    in_tree: bool,

    /// The retpoline security feature
    retpoline: bool,

    /// If the module is staging
    staging: bool,

    /// Other modules this one depends on
    dependencies: Vec<String>,

    /// Source Checksum.
    source_checksum: String,

    /// Module Parameters
    parameters: Vec<ModuleParameter>,

    /// Module Signature
    signature: Option<ModuleSignature>,

    /// Imported namespaces
    imports: Vec<String>,
}

// Attributes
impl ModInfo {
    /// Checksum of module source files
    pub fn source_checksum(&self) -> &str {
        &self.source_checksum
    }

    /// Whether this module is unstable and in staging
    pub fn staging(&self) -> &bool {
        &self.staging
    }

    /// Whether retpoline mitigations are enabled in this module?
    pub fn retpoline(&self) -> &bool {
        &self.retpoline
    }

    /// Whether the module is in-tree or not
    pub fn in_tree(&self) -> &bool {
        &self.in_tree
    }

    /// Module name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Kernel version compatibility string
    pub fn version_magic(&self) -> &str {
        &self.version_magic
    }

    /// Module version
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Description of what this module does
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Module Authors
    pub fn authors(&self) -> &[String] {
        &self.authors
    }

    /// Module license
    pub fn license(&self) -> &str {
        &self.license
    }

    /// Filenames of optional firmware files
    pub fn firmware(&self) -> &[String] {
        &self.firmware
    }

    /// Alternate, alias names for this module
    pub fn alias(&self) -> &[String] {
        &self.alias
    }

    /// Optional dependencies for this module
    pub fn soft_dependencies(&self) -> &[String] {
        &self.soft_dependencies
    }

    /// Dependencies for this module
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    /// Parameters for this module
    pub fn parameters(&self) -> &[ModuleParameter] {
        &self.parameters
    }

    /// Module signature bytes
    ///
    /// Empty if there is no signature.
    pub fn signature(&self) -> Option<&ModuleSignature> {
        self.signature.as_ref()
    }

    /// Imported namespaces
    pub fn imports(&self) -> &[String] {
        &self.imports
    }
}

// Private
impl ModInfo {
    /// Read the `.modinfo` section from the file at `path`
    ///
    /// # Errors
    fn read(path: &Path) -> Result<Self, ModInfoError> {
        let img = Self::decompress(
            path,
            fs::read(path).map_err(|e| ModInfoError::Compression(e.to_string()))?,
        )
        .map_err(|e| ModInfoError::Compression(e.to_string()))?;

        let elf: ElfBytes<AnyEndian> = ElfBytes::minimal_parse(&img)
            .map_err(|e| ModInfoError::InvalidModule(e.to_string()))?;

        let sect = elf
            .section_header_by_name(".modinfo")
            .map_err(|e| ModInfoError::InvalidModule(e.to_string()))?
            .ok_or(ModInfoError::MissingInfo)?;

        let (data, comp) = elf
            .section_data(&sect)
            .map_err(|e| ModInfoError::InvalidModule(e.to_string()))?;

        // TODO: Support compression?
        if comp.is_some() {
            unimplemented!("Module .modinfo section compressed");
        }

        // (tag, Vec<value>)
        let mut map = HashMap::new();

        // Parse tag=value\0 strings from section
        //
        // Duplicate tags are expected to exist
        //
        // Known tags are:
        //
        // - `parm`: A parameter description `<name>:<desc>`
        //   - One for each line of `name`s description
        // - `parmtype`: A parameter's type `<name>:<type>`
        //   - One for each parameter
        //   - Not guaranteed to exist for each parameter
        // - `alias`: Alternate names for module
        //   - One for each alias name
        // - `softdep`: Optional module dependencies
        //   - One for each soft dep
        // - `license`: Module License
        //   - One of `GPL`, `GPL v2`, `GPL and additional rights`, `Dual BSD/GPL`,
        //     `Dual MIT/GPL`, `Dual MPL/GPL`, or `Proprietary`
        // - `author`: Module author(s) `Name <Email>` or `Name`
        //   - One for each author
        // - `description`: Module description. Freeform text
        // - `version`: Module version `[<epoch>:]<version>[-<extra-version>]`
        // - `firmware`: File name of.. optional firmware files needed? by the module
        //   - One per file
        // - `import_ns`: Namespaces imported by the module
        // - `intree`: Whether the module is "in-tree"
        // - `retpoline`: Whether the module is compiled with retpoline mitigations
        // - `staging`: Whether the module is is unstable and in staging
        // - `depends`: Comma separated string of module dependency names
        // - `vermagic`: Kernel version compatibility string
        //
        // - Anything using the `MODULE_INFO` C macro
        // See `module.h`
        for s in data.split(|&c| c == b'\0') {
            let s = from_utf8(s).map_err(|e| {
                ModInfoError::InvalidUtf8(
                    e.to_string(),
                    match from_utf8(&s[..e.valid_up_to()]) {
                        Ok(s) => s.into(),
                        _ => String::new(),
                    },
                )
            })?;
            // End of modinfo?
            if s.is_empty() {
                continue;
            }

            let (key, value) = s.split_once('=').ok_or(ModInfoError::InvalidParameter)?;

            let vec = map.entry(key.to_owned()).or_insert_with(Vec::new);
            if !value.is_empty() {
                vec.push(value.to_owned());
            }
        }

        // (Name, (Type, Description))
        // Module | Parameter
        let mut x: HashMap<String, (ParameterType, Vec<String>)> = HashMap::new();

        let parm_type = map.remove("parmtype").unwrap_or_default();
        let parm = map.remove("parm").unwrap_or_default();

        // Parse parameter types
        // Not all parameters will have a type
        for kv in parm_type {
            let (name, ty) = kv.split_once(':').ok_or(ModInfoError::InvalidParameter)?;

            // Parameters should not have multiple types.
            if x.insert(name.into(), (ParameterType::parse(ty), Vec::new()))
                .is_some()
            {
                return Err(ModInfoError::InvalidParameter);
            };
        }

        // Parse parameter descriptions
        // Not all parameters will have a description, either.
        for kv in parm {
            let (name, desc) = kv.split_once(':').ok_or(ModInfoError::InvalidParameter)?;

            // Add parameter descriptions
            x.entry(name.into())
                .or_insert((ParameterType::Unknown, Vec::new()))
                .1
                .push(desc.into());
        }

        // Combine into a single vector
        let parameters = x
            .into_iter()
            .map(|(name, (ty, description))| ModuleParameter {
                name,
                ty,
                description: description.join("\n"),
            })
            .collect::<Vec<ModuleParameter>>();

        // Get module signature information
        let mut signature = None;
        let sig_hdr_off = img.len() - size_of::<RawModuleSignature>() - SIGNATURE_MAGIC.len();

        if img.ends_with(SIGNATURE_MAGIC) {
            if let Some(sig) = img.get(sig_hdr_off..) {
                assert!(sig.len() >= size_of::<RawModuleSignature>());
                // Safety:
                // - `img` is at least `size_of::<RawModuleSignature>()`
                // - `RawModuleSignature` is always valid and initialized
                // - Alignment is accounted for
                let hdr = unsafe {
                    (sig.as_ptr() as *const u8 as *const RawModuleSignature).read_unaligned()
                };
                let sig_len: usize = u32::from_be(hdr.sig_len)
                    .try_into()
                    .map_err(|_| ModInfoError::InvalidSignature)?;
                if !hdr.valid() {
                    return Err(ModInfoError::InvalidSignature);
                }

                if let Some(sig) = img.get(sig_hdr_off - sig_len..sig_hdr_off) {
                    signature = Some(ModuleSignature::new(sig.to_vec())?);
                }
            }
        }

        // Helpers to process options
        fn y_n(s: &str) -> bool {
            if s == "0" || s == "1" {
                unimplemented!("ambiguous Bool");
            }
            s == "Y" || s == "y"
        }
        fn one(map: &mut HashMap<String, Vec<String>>, key: &str) -> String {
            map.remove(key).map(|mut v| v.remove(0)).unwrap_or_default()
        }
        fn more(map: &mut HashMap<String, Vec<String>>, key: &str) -> Vec<String> {
            map.remove(key).unwrap_or_default()
        }
        fn comma(map: &mut HashMap<String, Vec<String>>, key: &str) -> Vec<String> {
            map.remove(key)
                .unwrap_or_default()
                .pop()
                .unwrap_or_default()
                .split(',')
                .map(Into::into)
                .collect()
        }

        let s = Self {
            alias: more(&mut map, "alias"),
            // TODO: Proper soft deps
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
            dependencies: comma(&mut map, "depends"),
            source_checksum: one(&mut map, "srcversion"),
            parameters,
            signature,
            imports: more(&mut map, "import_ns"),
        };

        Ok(s)
    }

    /// Decompresses module at `path`
    ///
    /// Returns `data` unchanged if its not compressed.
    fn decompress(path: &Path, data: Vec<u8>) -> Result<Vec<u8>, DecompressError> {
        #[cfg(any(feature = "xz", feature = "gz", feature = "zst"))]
        let mut v: Vec<u8> = Vec::new();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or(DecompressError::Unsupported(path))?;

        match ext {
            #[cfg(feature = "xz")]
            "xz" => {
                let mut data = XzDecoder::new(data.as_slice());
                data.read_to_end(&mut v)
                    .map_err(|e| DecompressError::Compression(path, e.to_string()))?;
                Ok(v)
            }
            #[cfg(feature = "gz")]
            "gz" => {
                let mut data = GzDecoder::new(data.as_slice());
                data.read_to_end(&mut v)
                    .map_err(|e| DecompressError::Compression(path, e.to_string()))?;
                Ok(v)
            }
            #[cfg(feature = "zst")]
            "zst" => {
                let mut data = ZstDecoder::new(data.as_slice())
                    .map_err(|e| DecompressError::Compression(path, e.to_string()))?;
                data.read_to_end(&mut v)
                    .map_err(|e| DecompressError::Compression(path, e.to_string()))?;
                Ok(v)
            }
            "ko" => Ok(data),
            _ => Err(DecompressError::Unsupported(path)),
        }
    }
}

/// Supported [`ModuleSignature`] hash algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ModuleSignatureHash {
    /// SHA-1
    Sha1,

    /// SHA-224
    Sha224,

    /// SHA-256
    Sha256,

    /// SHA-384
    Sha384,

    /// SHA-512
    Sha512,

    /// Unknown / unrecognized algorithm
    Unknown,
}

impl ModuleSignatureHash {
    const fn parse(f: &AlgorithmIdentifier<der::Any>) -> Self {
        match f.oid {
            ID_SHA_1 => ModuleSignatureHash::Sha1,
            ID_SHA_224 => ModuleSignatureHash::Sha224,
            ID_SHA_256 => ModuleSignatureHash::Sha256,
            ID_SHA_384 => ModuleSignatureHash::Sha384,
            ID_SHA_512 => ModuleSignatureHash::Sha512,
            _ => ModuleSignatureHash::Unknown,
        }
    }
}

impl std::fmt::Display for ModuleSignatureHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sha1 => write!(f, "SHA-1"),
            Self::Sha224 => write!(f, "SHA-224"),
            Self::Sha256 => write!(f, "SHA-256"),
            Self::Sha384 => write!(f, "SHA-384"),
            Self::Sha512 => write!(f, "SHA-512"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A signature on a [`ModuleFile`]
///
/// Linux kernel modules may be signed according to [the docs][kernel_sign_docs]
///
/// [kernel_sign_docs]: https://www.kernel.org/doc/html/latest/admin-guide/module-signing.html
#[derive(Debug)]
pub struct ModuleSignature {
    /// Raw signature bytes
    ///
    /// This seems to be CMS DER
    signature: Vec<u8>,

    /// Parsed signed data
    signed_data: SignedData,
}

impl ModuleSignature {
    fn new(signature: Vec<u8>) -> Result<Self, ModInfoError> {
        let cms = ContentInfo::from_der(&signature).map_err(|_| ModInfoError::InvalidSignature)?;
        if cms.content_type != ID_SIGNED_DATA {
            return Err(ModInfoError::InvalidSignature);
        }

        let sd = cms
            .content
            .to_der()
            .map_err(|_| ModInfoError::InvalidSignature)?;
        let signed_data = SignedData::from_der(&sd).map_err(|_| ModInfoError::InvalidSignature)?;

        if signed_data.digest_algorithms.len() > 1 {
            return Err(ModInfoError::UnsupportedSignature);
        }

        if signed_data.signer_infos.0.len() > 1 {
            return Err(ModInfoError::UnsupportedSignature);
        }

        match signed_data.signer_infos.0.as_slice().get(0).map(|f| &f.sid) {
            Some(SignerIdentifier::IssuerAndSerialNumber(_)) => (),
            _ => return Err(ModInfoError::UnsupportedSignature),
        }

        Ok(Self {
            signature,
            signed_data,
        })
    }

    /// Raw on-disk module signature bytes
    pub fn raw(&self) -> &[u8] {
        &self.signature
    }

    /// Who signed this module
    pub fn signers(&self) -> impl Iterator<Item = ModuleSigner> + '_ {
        self.signed_data
            .signer_infos
            .0
            .iter()
            .map(|f| ModuleSigner::new(f.clone()))
    }

    /// Set of hash algorithms module says it was signed with
    ///
    /// Not guaranteed to match what it was actually signed with.
    pub fn hashes(&self) -> impl Iterator<Item = ModuleSignatureHash> + '_ {
        self.signed_data
            .digest_algorithms
            .as_slice()
            .iter()
            .map(ModuleSignatureHash::parse)
    }
}

/// A signer of a [`ModuleSignature`]
#[derive(Debug)]
pub struct ModuleSigner {
    signer: SignerInfo,
}

impl ModuleSigner {
    fn new(signer: SignerInfo) -> Self {
        Self { signer }
    }

    /// Signer Public Key
    pub fn public_key(&self) -> &[u8] {
        match &self.signer.sid {
            SignerIdentifier::IssuerAndSerialNumber(i) => i.serial_number.as_bytes(),
            SignerIdentifier::SubjectKeyIdentifier(b) => b.0.as_bytes(),
        }
    }

    /// Signers signature
    pub fn signature(&self) -> &[u8] {
        self.signer.signature.as_bytes()
    }

    /// Digest hash algorithm
    pub const fn hash(&self) -> ModuleSignatureHash {
        ModuleSignatureHash::parse(&self.signer.digest_alg)
    }

    /// Certificate issuer
    pub fn issuer(&self) -> String {
        match &self.signer.sid {
            SignerIdentifier::IssuerAndSerialNumber(i) => i
                .issuer
                .to_string()
                .split_once('=')
                .unwrap_or_default()
                .1
                .into(),
            SignerIdentifier::SubjectKeyIdentifier(_) => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;

    /// Test that we can parse every module file of every currently running
    /// module
    #[test]
    fn modinfo() -> Result<()> {
        let mods = Module::loaded()?;

        for m in mods.into_iter() {
            let name = m.name();
            let m = ModuleFile::from_name(name);
            // dbg!(&m);
            let m = m?;
            // dbg!(&m);
            // dbg!(&name);
            let sig = m.info().signature().unwrap();
            for s in sig.signers() {
                s.signature();
                s.issuer();
                s.hash();
                s.public_key();
            }
            dbg!(&sig);
        }

        // panic!();
        Ok(())
    }

    #[test]
    fn feature() -> Result<()> {
        let m = ModuleFile::from_name("amdgpu")?;
        let i = m.info();
        let s = i.signature().unwrap();
        let ss = s.signers().collect::<Vec<_>>();
        dbg!(&ss);
        dbg!(ss[0].issuer());
        // dbg!(&m);
        // panic!();
        Ok(())
    }
}
