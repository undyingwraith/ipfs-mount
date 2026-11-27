# IPFS-Mount

This project provides a more dynamic way to mount ipfs directories.

> [!WARNING]  
> Currently only works for `MFS`, because there is no real alternative to `ipfs files stat /` for ipfs/ipns entries.
> I do want this project to be able to do that too, but currently mounting `MFS` is the main thing I want with this project.
>
> _Do expect some bugs, I'm just happy it works for what I'm using it for right now._

## Usage

### CLI

> [!NOTE]
> See **Warning** above^ .


```
ipfs-mount </mount-directory> </ipfs/path>
```

**Examples**
```
ipfs-mount /mnt /ipns/ipfs.io
ipfs-mount /mnt /ipfs/bafybeiealq5axr4jqdvfcltkq5g3kt2l5fjs3o3i5mks6ujpmoi3pb6d2y
```

### Library

```rust
use std::env;
use tokio::signal;

#[tokio::main(flavor = "current_thread")]
async fn main() {
   let handle = ipfs_mount::mount("/mnt", "/").await;

   signal::ctrl_c().await.expect("failed to listen for event");

   handle.unmount().await.expect("failed to unmount");
}
```
