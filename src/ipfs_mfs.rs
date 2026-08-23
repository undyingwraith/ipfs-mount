use fuse3::raw::Filesystem;
use fuse3::raw::prelude::*;
use fuse3::raw::reply::{ReplyEntry, ReplyInit};
use fuse3::{FileType, Result, Timestamp};
use futures_util::stream;
use futures_util::stream::Stream;
use libc::ENOENT;
use regex::Regex;
use std::ffi::{OsStr, OsString};
use std::iter::Iterator;
use std::num::NonZeroU32;
use std::process::Command;
use std::sync::RwLock;
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
	// IPFS path to mount
	path: OsString,

	// Internal store to map ino to path
	entries: RwLock<Vec<String>>,
}

impl IpfsMfs {
	pub fn new(path: OsString) -> Self {
		Self {
			path,
			entries: RwLock::new(Vec::new()),
		}
	}

	fn stat(&self, path: &String) -> Option<FileAttr> {
		if !self.entries.read().unwrap().contains(path) {
			self.entries.write().unwrap().push(path.clone());
		}

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

		let caps = res.unwrap();

		let ino = (self.entries.read().unwrap().iter().position(|r| r == path).unwrap() + 1) as u64;
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

	fn get_path(&self, ino: u64) -> Option<String> {
		if self.entries.read().unwrap().len() > (ino - 1) as usize {
			return Some(self.entries.read().unwrap()[(ino - 1) as usize].clone());
		}

		None
	}

	fn append_path(&self, mut path: String, filename: &str) -> String {
		if !path.ends_with("/") {
			path.push('/');
		}

		path.push_str(filename);
		path
	}
}

impl Filesystem for IpfsMfs {
	async fn init(&self, _req: Request) -> Result<ReplyInit> {
		self.stat(&self.path.clone().into_string().unwrap());
		Ok(ReplyInit {
			max_write: NonZeroU32::new(16 * 1024).unwrap(),
		})
	}

	async fn destroy(&self, _req: Request) {}

	async fn lookup(&self, _req: Request, parent: u64, name: &OsStr) -> Result<ReplyEntry> {
		if let Some(p) = self.get_path(parent) {
			let path = self.append_path(p, name.to_str().unwrap());
			if let Some(stat) = self.stat(&path) {
				return Ok(ReplyEntry {
					ttl: TTL,
					attr: stat,
					generation: 0,
				});
			}
		}

		Err(ENOENT.into())
	}

	async fn getattr(&self, _req: Request, ino: u64, _fh: Option<u64>, _flags: u32) -> Result<ReplyAttr> {
		if let Some(path) = self.get_path(ino)
			&& let Some(res) = self.stat(&path)
		{
			Ok(ReplyAttr { ttl: TTL, attr: res })
		} else {
			Err(ENOENT.into())
		}
	}

	async fn read(&self, _req: Request, ino: u64, _fh: u64, offset: u64, size: u32) -> Result<ReplyData> {
		match self.get_path(ino) {
			Some(path) => {
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
				Ok(ReplyData {
					data: output.stdout.into(),
				})
			}
			None => Err(ENOENT.into()),
		}
	}
	async fn readdir(
		&self,
		_req: Request,
		ino: u64,
		_fh: u64,
		offset: i64,
	) -> Result<ReplyDirectory<impl Stream<Item = Result<DirectoryEntry>> + Send + '_>> {
		match self.get_path(ino) {
			None => Err(ENOENT.into()),
			Some(path) => {
				let output = Command::new("ipfs")
					.arg("files")
					.arg("ls")
					.arg(path)
					.output()
					.expect("Failed to execute command");

				let out = String::from_utf8(output.stdout).expect("Failed to parse output");
				let names = out.lines();
				let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;

				let mut res = Vec::new();
				for (i, name) in names.enumerate().skip(to_skip) {
					let path = self.append_path(self.entries.read().unwrap()[(ino - 1) as usize].clone(), name);
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
		match self.get_path(ino) {
			None => Err(ENOENT.into()),
			Some(path) => {
				let output = Command::new("ipfs")
					.arg("files")
					.arg("ls")
					.arg(path)
					.output()
					.expect("Failed to execute command");

				let out = String::from_utf8(output.stdout).expect("Failed to parse output");
				let names = out.lines();
				let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;

				let mut res = Vec::new();
				for (i, name) in names.enumerate().skip(to_skip) {
					let path = self.append_path(self.entries.read().unwrap()[(ino - 1) as usize].clone(), name);
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
		Ok(STATFS)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn can_retrieve_path_from_ino() {
		let mut entries = Vec::new();
		entries.push("/".to_string());
		entries.push("/test".to_string());
		let mut fs = IpfsMfs {
			path: "/".into(),
			entries: RwLock::new(entries),
		};

		let path1 = fs.get_path(1);
		assert_eq!(path1.is_none(), false);
		assert_eq!(path1.unwrap(), "/".to_string());

		let path2 = fs.get_path(2);
		assert_eq!(path2.is_none(), false);
		assert_eq!(path2.unwrap(), "/test".to_string());

		let path3 = fs.get_path(3);
		assert_eq!(path3.is_none(), true);
	}

	#[test]
	fn can_append_path() {
		let mut fs = IpfsMfs {
			path: "/".into(),
			entries: RwLock::new(Vec::new()),
		};

		let path1 = fs.append_path("/".to_string(), "test");
		assert_eq!(path1, "/test");

		let path2 = fs.append_path(path1.to_string(), "test");
		assert_eq!(path2, "/test/test");
	}
}
