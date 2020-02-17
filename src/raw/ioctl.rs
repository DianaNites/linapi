//! Bindings for Linux `ioctl`s

pub mod block;

mod _impl {
    use nix::*;

    ioctl_read! {
        /// The `BLKGETSIZE64` ioctl.
        block_device_size_bytes, 0x12, 114, u64
    }
}
