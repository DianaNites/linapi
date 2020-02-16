# Linapi

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat)](https://github.com/RichardLitt/standard-readme)
[![linapi crates.io version and link](https://img.shields.io/crates/v/linapi.svg)](https://crates.io/crates/linapi)
![linapi Crates.io license](https://img.shields.io/crates/l/linapi)
[![linapi docs.rs badge](https://docs.rs/linapi/badge.svg)](https://docs.rs/linapi)

High level bindings to various Linux APIs and interfaces

This crate provides high-level, safe, Rust bindings to
the various Linux Kernel APIs and interfaces.

This crate is currently experimental, and the API will change.

## Background

The goal of this crate is to provide relatively high-level bindings,
specifically for the Linux Kernel.

The kernel exposes a lot of information through filesystems like `sysfs`,
and there aren't a lot of good structured ways to handle it, on top of it being sparsely documented.

So this crate does the work of handling it for you!

## Install

```toml
[dependencies]
linapi = "0.2.1"
```

### Dependencies

- The Linux Kernel. This crate has only been tested with version `5.5.3`.

## Usage

See the documentation for details

## Changelog

Please see [CHANGELOG](CHANGELOG.md) for version history

## Contributing

This crate is not looking for contributors at this time.

However, feel free to ask questions and request bindings using github issues,
or suggest/discuss API improvements.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as below, without any additional terms or conditions.

## License

Licensed under either of

- Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0)>
- MIT license
   ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT)>

at your option.
