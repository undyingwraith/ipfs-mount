use std::env;
use crate::ipfs_mfs::IpfsMfs;

mod ipfs_mfs;

fn main() {
	let mountpoint = env::args_os().nth(1).unwrap();
	let path = env::args_os().nth(2).unwrap();

	fuse::mount(IpfsMfs::new(path), &mountpoint, &[]).unwrap();
}
