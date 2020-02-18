# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Fixed

- `ModuleFile::has_signature` for compressed modules.
- `ModuleFile::info` for modules without parameters. Previously it would panic.
- `ModuleFile::info` for modules without parameter descriptions. Previously it would panic.
- Actually implement `LoadedModule::parameters`. Oops.
- `ModuleFile::from_name_with_uname`/`ModuleFile::from_name` actually search `/lib/modules/(uname -r)`.

### Added

- Error handling.
- `ModuleFile::from_name_with_uname`, to lookup by `uname` in `/lib/modules`

### Changed

- `ModuleFile` methods now return errors instead of panicking.
- `ModuleFile::info` returns `&ModInfo` instead of `ModInfo`.
- `ModParam::description` type changed to `Option<String>`.

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
[Unreleased]: https://github.com/DianaNites/linapi/compare/v0.3.4...HEAD
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
