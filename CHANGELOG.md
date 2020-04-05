# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.4.4] - 2020-04-05

### Fixed

- `LoadedModule::from_name` not returning an `Err` on non-existent modules.

## [0.4.3] - 2020-02-19

### Fixed

- `LoadedModule::get_loaded` failed when encountering built-in modules,
  or modules with parameters that require root to read.

## [0.4.2] - 2020-02-19

### Changed

- `ModParam` is now `Clone`.

## [0.4.1] - 2020-02-19

### Fixed

- Decompression failures for `xz` compressed kernel modules.

## [0.4.0] - 2020-02-18

### Fixed

- `ModuleFile::has_signature` for compressed modules.
- `ModuleFile::info` for modules without parameters. Previously it would panic.
- `ModuleFile::info` for modules without parameter descriptions. Previously it would panic.
- Actually implement `LoadedModule::parameters`. Oops.
- `ModuleFile::from_name_with_uname`/`ModuleFile::from_name` actually search `/lib/modules/(uname -r)`.

### Added

- Error handling.
- `ModuleFile::from_name_with_uname`, to lookup by `uname` in `/lib/modules`
- `LoadedModule::module_file`, to get a ModuleFile from a LoadedModule.
- Cargo Features for compression

### Changed

- `LoadedModule` methods now return errors instead of panicking.
- `ModuleFile` methods now return errors instead of panicking.
- `ModuleFile::info` returns `&ModInfo` instead of `ModInfo`.
- `ModParam::description` type changed to `Option<String>`.
- `Status` is no longer `Copy`.
- `LoadedModule::parameters` returns `&HashMap` instead of `HashMap`
- `LoadedModule::holders` returns `&Vec` instead of `Vec`
- `LoadedModule::status` returns `&Status` instead of `Status`

### Removed

- `LoadedModule::file_path`

## [0.3.4] - 2020-02-17

### Fixed

- Various `BlockDevice` methods, which forgot to trim newlines

## [0.3.3] - 2020-02-17

### Fixed

- `get_connected` for real this time...

## [0.3.2] - 2020-02-17 [YANKED]

### Fixed

- Bug in `BlockDevice::get_connected` where partitions would incorrectly
  be included.

## [0.3.1] - 2020-02-17

### Fixed

- `Device::get_connected`

## [0.3.0] - 2020-02-16

### Changed

- Renamed `LoadedModule::from_loaded` to `LoadedModule::get_loaded`
  `from_loaded` wasn't a good name because this constructor doesn't
  convert from anything.
- Replaced the `DevicePower` trait with `Device::power` on the `Device` trait.

## [0.2.4] - 2020-02-16

### Added

- xz compression support
- New extensions to `std::fs::File`,
  `create_memory` which allows having a File Descriptor without a file on disk!

## [0.2.3] - 2020-02-16

## [0.2.2] - 2020-02-16

### Added

- Changelog

## [0.2.1] - 2020-02-16

### Added

- Stuff for `cargo-release`

## [0.2.0] - 2020-02-16

### Added

- API for managing Linux Kernel Modules
- Types for common system interfaces

### Changed

- Updated dependencies

### Removed

- Old unfinished raw `ioctl` API

## [0.1.1] - 2019-10-17

### Added

- Docs.rs badge

## [0.1.0] - 2019-10-17

### Added

- Initial release. No notable features

<!-- next-url -->
[Unreleased]: https://github.com/DianaNites/linapi/compare/v0.4.4...HEAD
[0.4.4]: https://github.com/DianaNites/linapi/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/DianaNites/linapi/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/DianaNites/linapi/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/DianaNites/linapi/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/DianaNites/linapi/compare/v0.3.4...v0.4.0
[0.3.4]: https://github.com/DianaNites/linapi/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/DianaNites/linapi/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/DianaNites/linapi/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/DianaNites/linapi/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/DianaNites/linapi/compare/v0.2.4...v0.3.0
[0.2.4]: https://github.com/DianaNites/linapi/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/DianaNites/linapi/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/DianaNites/linapi/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/DianaNites/linapi/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/DianaNites/linapi/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/DianaNites/linapi/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/DianaNites/linapi/releases/tag/v0.1.0
