use std::env;
use tokio::signal;

use ipfs_mount::mount;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let mountpoint = env::args_os().nth(1).unwrap();
	let path = env::args_os().nth(2).unwrap();

	let handle = mount(mountpoint.to_str().unwrap(), path.to_str().unwrap()).await;

	signal::ctrl_c().await.expect("failed to listen for event");

	handle.unmount().await.expect("failed to unmount");
}
