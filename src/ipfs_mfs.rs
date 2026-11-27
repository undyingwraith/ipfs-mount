use crate::base_fs::{ReadonlyBaseFs, WriteableBaseFs};
use crate::path_store::PathStore;
use fuse3::raw::Filesystem;
use fuse3::raw::prelude::*;
use fuse3::raw::reply::{ReplyEntry, ReplyInit};
use fuse3::{Errno, FileType, Result, Timestamp};
use futures_util::stream;
use futures_util::stream::Stream;
use libc::ENOENT;
use regex::Regex;
use std::ffi::OsStr;
use std::io::Write;
use std::iter::Iterator;
use std::num::NonZeroU32;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::vec::Vec;

const TTL: Duration = Duration::from_secs(1);
const CREATE_TIME: Timestamp = Timestamp {
	sec: 1381237736,
	nsec: 0,
};
const STATFS: ReplyStatFs = ReplyStatFs {
	blocks: 1,
	bfree: 0,
	bavail: 0,
	files: 1,
	ffree: 0,
	bsize: 4096,
	namelen: u32::MAX,
	frsize: 0,
};

pub struct IpfsMfs {
	/// Internal store to map ino to path
	store: PathStore,
}

impl IpfsMfs {
	/// Creates a new instance of [`IpfsMfs`].
	pub fn new<S: AsRef<str>>(path: S) -> Self {
		let store = PathStore::default();
		store.add_path(path.as_ref());
		Self { store }
	}
}

impl ReadonlyBaseFs for IpfsMfs {
	fn stat(&self, path: &str) -> Option<FileAttr> {
		let re = Regex::new(
			r"^[A-Za-z0-9]+\nSize: (\d+)\nCumulativeSize: (\d+)\nChildBlocks: (\d+)\nType: (directory|file)",
		)
		.unwrap();
		let output = Command::new("ipfs")
			.arg("files")
			.arg("stat")
			.arg(path)
			.output()
			.expect("Failed to execute command");

		let out = String::from_utf8(output.stdout).expect("Failed to parse output");
		let res = re.captures(&out);

		res.as_ref()?;

		// Only add valid paths to self.entries
		if !self.store.has_path(path) {
			println!("stat added {:?}", path);
			self.store.add_path(path);
		}

		let caps = res.unwrap();

		let ino = self.store.get_ino(path).unwrap();
		let size = caps[1].parse::<u64>().unwrap();
		let kind = if &caps[4] == "directory" {
			FileType::Directory
		} else {
			FileType::RegularFile
		};

		Some(FileAttr {
			ino,
			size,
			blocks: 0,
			atime: CREATE_TIME,
			mtime: CREATE_TIME,
			ctime: CREATE_TIME,
			#[cfg(target_os = "macos")]
			crtime: CREATE_TIME,
			kind,
			perm: 0o755,
			nlink: 2,
			uid: 501,
			gid: 20,
			rdev: 0,
			#[cfg(target_os = "macos")]
			flags: 0,
			blksize: 0,
		})
	}

	fn read(&self, path: &str, offset: u64, size: u32) -> Vec<u8> {
		let output = Command::new("ipfs")
			.arg("files")
			.arg("read")
			.arg("-o")
			.arg(offset.to_string())
			.arg("-n")
			.arg(size.to_string())
			.arg(path)
			.output()
			.expect("Failed to execute command");

		output.stdout
	}

	fn ls(&self, path: &str) -> Vec<String> {
		let output = Command::new("ipfs")
			.arg("files")
			.arg("ls")
			.arg(path)
			.output()
			.expect("Failed to execute command");

		let out = String::from_utf8(output.stdout).expect("Failed to parse output");
		out.lines().map(|l| l.to_string()).collect::<Vec<String>>()
	}
}

impl WriteableBaseFs for IpfsMfs {
	fn write(&self, path: &str, offset: u64, data: &[u8]) {
		let mut child = Command::new("ipfs")
			.arg("files")
			.arg("write")
			.arg("-o")
			.arg(offset.to_string())
			.arg(path)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()
			.expect("Failed to execute command");

		let mut stdin = child.stdin.take().expect("Failed to open stdin");
		std::thread::scope(|s| {
			let t = s.spawn(move || {
				stdin.write_all(data).expect("Failed to write to stdin");
			});

			let _output = child.wait_with_output().expect("Failed to read stdout");

			let _ = t.join();
		});
	}

	fn create(&self, path: &str) {
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("write")
			.arg("-e")
			.arg(path)
			.output()
			.expect("Failed to execute command");
	}

	fn mkdir(&self, path: &str) {
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("mkdir")
			.arg(path)
			.output()
			.expect("Failed to execute command");
	}

	fn mv(&self, source: &str, target: &str) {
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("mv")
			.arg(source)
			.arg(target)
			.output()
			.expect("Failed to execute command");
	}

	fn rm(&self, path: &str) {
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("rm")
			.arg(path)
			.output()
			.expect("Failed to execute command");
	}

	fn rmdir(&self, path: &str) {
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("rm")
			.arg("-r")
			.arg(path)
			.output()
			.expect("Failed to execute command");
	}
}

impl Filesystem for IpfsMfs {
	async fn init(&self, _req: Request) -> Result<ReplyInit> {
		println!("init");
		//TODO: validate ipfs is reachable
		Ok(ReplyInit {
			max_write: NonZeroU32::new(16 * 1024).unwrap(),
		})
	}

	async fn destroy(&self, _req: Request) {
		println!("destroy");
	}

	async fn lookup(&self, _req: Request, parent: u64, name: &OsStr) -> Result<ReplyEntry> {
		println!("lookup {:?} {:?}", parent, name);
		let path = self
			.store
			.get_child_path(parent, name.to_str().unwrap())
			.ok_or_else(|| Errno::from(ENOENT))?;
		let stat = self.stat(&path).ok_or_else(|| Errno::from(ENOENT))?;

		Ok(ReplyEntry {
			ttl: TTL,
			attr: stat,
			generation: 0,
		})
	}

	async fn forget(&self, _req: Request, inode: u64, _nlookup: u64) {
		println!("forget {:?}", inode);
		//TODO: free up path in self.entries
	}

	async fn getattr(&self, _req: Request, ino: u64, _fh: Option<u64>, _flags: u32) -> Result<ReplyAttr> {
		println!("getattr {:?}", ino);

		let path = self.store.get_path(ino).ok_or_else(|| Errno::from(ENOENT))?;
		let res = self.stat(&path).ok_or_else(|| Errno::from(ENOENT))?;

		Ok(ReplyAttr { ttl: TTL, attr: res })
	}

	async fn mkdir(
		&self,
		_req: Request,
		parent: u64,
		name: &OsStr,
		_mode: u32, //TODO: use this if applicable
		_umask: u32,
	) -> Result<ReplyEntry> {
		println!("mkdir {:?} {:?}", parent, name);
		let path = self
			.store
			.get_child_path(parent, name.to_str().unwrap())
			.ok_or_else(|| Errno::from(ENOENT))?;

		WriteableBaseFs::mkdir(self, &path);

		//TODO: better error handling/throwing

		let stat = self.stat(&path).ok_or_else(|| Errno::from(ENOENT))?;

		Ok(ReplyEntry {
			ttl: TTL,
			attr: stat,
			generation: 0,
		})
	}

	async fn unlink(&self, _req: Request, parent: u64, name: &OsStr) -> Result<()> {
		println!("unlink {:?} {:?}", parent, name);
		let path = self
			.store
			.get_child_path(parent, name.to_str().unwrap())
			.ok_or_else(|| Errno::from(ENOENT))?;

		self.rm(&path);

		//TODO: better error handling/throwing

		Ok(())
	}

	async fn rmdir(&self, _req: Request, parent: u64, name: &OsStr) -> Result<()> {
		println!("rmdir {:?} {:?}", parent, name);
		let path = self
			.store
			.get_child_path(parent, name.to_str().unwrap())
			.ok_or_else(|| Errno::from(ENOENT))?;

		WriteableBaseFs::rmdir(self, &path);

		//TODO: better error handling/throwing

		Ok(())
	}

	async fn rename(&self, _req: Request, parent: u64, name: &OsStr, new_parent: u64, new_name: &OsStr) -> Result<()> {
		println!("rename {:?} {:?} {:?} {:?}", parent, name, new_parent, new_name);
		let source = self
			.store
			.get_child_path(parent, name.to_str().unwrap())
			.ok_or_else(|| Errno::from(ENOENT))?;
		let target = self
			.store
			.get_child_path(new_parent, new_name.to_str().unwrap())
			.ok_or_else(|| Errno::from(ENOENT))?;

		self.mv(&source, &target);

		//TODO: better error handling/throwing

		Ok(())
	}

	async fn read(&self, _req: Request, ino: u64, _fh: u64, offset: u64, size: u32) -> Result<ReplyData> {
		println!("read {:?} {:?} {:?}", ino, offset, size);
		match self.store.get_path(ino) {
			Some(path) => {
				let data = ReadonlyBaseFs::read(self, &path, offset, size);
				Ok(ReplyData { data: data.into() })
			}
			None => Err(ENOENT.into()),
		}
	}

	async fn write(
		&self,
		_req: Request,
		inode: u64,
		_fh: u64,
		offset: u64,
		data: &[u8],
		_write_flags: u32,
		_flags: u32,
	) -> Result<ReplyWrite> {
		println!("write {:?} {:?}", inode, offset);
		let path = self.store.get_path(inode).ok_or_else(|| Errno::from(ENOENT))?;

		WriteableBaseFs::write(self, &path, offset, data);

		Ok(ReplyWrite {
			written: data.len() as _,
		})
	}

	async fn create(
		&self,
		_req: Request,
		parent: u64,
		name: &OsStr,
		_mode: u32, //TODO: use this if applicable
		flags: u32, //TODO: use this if applicable
	) -> Result<ReplyCreated> {
		println!("create {:?} {:?}", parent, name);
		let path = self
			.store
			.get_child_path(parent, name.to_str().unwrap())
			.ok_or_else(|| Errno::from(ENOENT))?;

		WriteableBaseFs::create(self, &path);

		let stat = self.stat(&path).ok_or_else(|| Errno::from(ENOENT))?;

		Ok(ReplyCreated {
			ttl: TTL,
			attr: stat,
			generation: 0,
			fh: 0,
			flags,
		})
	}

	async fn readdir(
		&self,
		_req: Request,
		ino: u64,
		_fh: u64,
		offset: i64,
	) -> Result<ReplyDirectory<impl Stream<Item = Result<DirectoryEntry>> + Send + '_>> {
		println!("readdir {:?} {:?}", ino, offset);
		match self.store.get_path(ino) {
			None => Err(ENOENT.into()),
			Some(path) => {
				let names = self.ls(&path);
				let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;

				let mut res = Vec::new();
				for (i, name) in names.into_iter().enumerate().skip(to_skip) {
					let path = self.store.get_child_path(ino, &name).unwrap();
					let stat = self.stat(&path).unwrap();
					res.push(Ok(DirectoryEntry {
						inode: stat.ino,
						kind: stat.kind,
						name: name.into(),
						offset: (i + 1) as i64,
					}));
				}

				Ok(ReplyDirectory {
					entries: stream::iter(res),
				})
			}
		}
	}

	async fn readdirplus(
		&self,
		_req: Request,
		ino: u64,
		_fh: u64,
		offset: u64,
		_lock_owner: u64,
	) -> Result<ReplyDirectoryPlus<impl Stream<Item = Result<DirectoryEntryPlus>> + Send + '_>> {
		println!("readdirplus {:?} {:?}", ino, offset);
		match self.store.get_path(ino) {
			None => Err(ENOENT.into()),
			Some(path) => {
				let names = self.ls(&path);
				let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;

				let mut res = Vec::new();
				for (i, name) in names.into_iter().enumerate().skip(to_skip) {
					let path = self.store.get_child_path(ino, &name).unwrap();
					let stat = self.stat(&path).unwrap();
					res.push(Ok(DirectoryEntryPlus {
						inode: stat.ino,
						generation: 0,
						kind: stat.kind,
						name: name.into(),
						offset: (i + 1) as i64,
						attr: stat,
						entry_ttl: TTL,
						attr_ttl: TTL,
					}));
				}

				Ok(ReplyDirectoryPlus {
					entries: stream::iter(res),
				})
			}
		}
	}

	async fn statfs(&self, _req: Request, _inode: u64) -> Result<ReplyStatFs> {
		println!("statfs");
		Ok(STATFS)
	}

	async fn copy_file_range(
		&self,
		req: Request,
		inode: u64,
		fh_in: u64,
		off_in: u64,
		inode_out: u64,
		fh_out: u64,
		off_out: u64,
		length: u64,
		flags: u64,
	) -> Result<ReplyCopyFileRange> {
		println!("copy_file_range");
		let data = Filesystem::read(self, req, inode, fh_in, off_in, length as _).await?;

		let data = data.data.as_ref();

		let ReplyWrite { written } =
			Filesystem::write(self, req, inode_out, fh_out, off_out, data, 0, flags as _).await?;

		Ok(ReplyCopyFileRange {
			copied: u64::from(written),
		})
	}
}
