use crate::ipfs_mfs::IpfsMfs;
use fuse3::MountOptions;
use fuse3::raw::prelude::*;
use std::env;

mod base_fs;
mod ipfs_mfs;
mod path_store;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let mountpoint = env::args_os().nth(1).unwrap();
	let path = env::args_os().nth(2).unwrap();

	let uid = unsafe { libc::getuid() };
	let gid = unsafe { libc::getgid() };

	let mut mount_options = MountOptions::default();
	mount_options.uid(uid).gid(gid); //.read_only(true);

	Session::new(mount_options)
		.mount_with_unprivileged(IpfsMfs::new(path), mountpoint)
		.await
		.unwrap()
		.await
		.unwrap()
}
