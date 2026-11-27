//! An alternative way to mount ipfs using FUSE.
//!
//! Example:
//! ```no_run
//! use std::env;
//! use tokio::signal;
//! use ipfs_mount::mount;
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() {
//!    let handle = mount("/mnt", "/").await;
//!
//!    signal::ctrl_c().await.expect("failed to listen for event");
//!
//!    handle.unmount().await.expect("failed to unmount");
//! }
// ```

mod base_fs;
mod ipfs_mfs;

use crate::ipfs_mfs::IpfsMfs;
use fuse3::MountOptions;
use fuse3::path::Session;
use fuse3::raw::MountHandle;

pub async fn mount<S: AsRef<str>>(mountpoint: S, path: S) -> MountHandle {
	let uid = unsafe { libc::getuid() };
	let gid = unsafe { libc::getgid() };

	let mut mount_options = MountOptions::default();
	mount_options.uid(uid).gid(gid); //.read_only(true);

	Session::new(mount_options)
		.mount_with_unprivileged(IpfsMfs::new(path), mountpoint.as_ref())
		.await
		.unwrap()
}
