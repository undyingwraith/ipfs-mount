use crate::base_fs::{ReadonlyBaseFs, WriteableBaseFs};
use fuse3::path::PathFilesystem;
use fuse3::path::prelude::*;
use fuse3::{Errno, FileType, Result};
use futures_util::stream;
use futures_util::stream::Stream;
use libc::ENOENT;
use regex::Regex;
use std::ffi::OsStr;
use std::io::Write;
use std::iter::Iterator;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};
use std::vec::Vec;

const TTL: Duration = Duration::from_secs(1);
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
	path: PathBuf,
}

impl IpfsMfs {
	/// Creates a new instance of [`IpfsMfs`].
	pub fn new<S: AsRef<str>>(path: S) -> Self {
		Self {
			path: PathBuf::from(path.as_ref()),
		}
	}
}

impl ReadonlyBaseFs for IpfsMfs {
	fn stat(&self, path: &str) -> Option<FileAttr> {
		let path = self.path.join(PathBuf::from(path).strip_prefix("/").unwrap());
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

		let size = caps[1].parse::<u64>().unwrap();
		let kind = if &caps[4] == "directory" {
			FileType::Directory
		} else {
			FileType::RegularFile
		};

		Some(FileAttr {
			size,
			blocks: 0,
			atime: SystemTime::UNIX_EPOCH,
			mtime: SystemTime::UNIX_EPOCH,
			ctime: SystemTime::UNIX_EPOCH,
			#[cfg(target_os = "macos")]
			crtime: CREATE_TIME,
			kind,
			perm: if kind == FileType::Directory { 0o755 } else { 0o644 },
			nlink: if kind == FileType::Directory { 2 } else { 1 },
			uid: 501, //TODO: use user that started the rust program
			gid: 20,  //TODO: same for group
			rdev: 0,
			#[cfg(target_os = "macos")]
			flags: 0,
			blksize: 0,
		})
	}

	fn read(&self, path: &str, offset: u64, size: u32) -> Vec<u8> {
		let path = self.path.join(PathBuf::from(path).strip_prefix("/").unwrap());
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
		let path = self.path.join(PathBuf::from(path).strip_prefix("/").unwrap());
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
		let path = self.path.join(PathBuf::from(path).strip_prefix("/").unwrap());
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
		let path = self.path.join(PathBuf::from(path).strip_prefix("/").unwrap());
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("write")
			.arg("-e")
			.arg(path)
			.output()
			.expect("Failed to execute command");
	}

	fn mkdir(&self, path: &str) {
		let path = self.path.join(PathBuf::from(path).strip_prefix("/").unwrap());
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("mkdir")
			.arg(path)
			.output()
			.expect("Failed to execute command");
	}

	fn mv(&self, source: &str, target: &str) {
		let source_path = self.path.join(PathBuf::from(source).strip_prefix("/").unwrap());
		let target_path = self.path.join(PathBuf::from(target).strip_prefix("/").unwrap());
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("mv")
			.arg(source_path)
			.arg(target_path)
			.output()
			.expect("Failed to execute command");
	}

	fn rm(&self, path: &str) {
		let path = self.path.join(PathBuf::from(path).strip_prefix("/").unwrap());
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("rm")
			.arg(path)
			.output()
			.expect("Failed to execute command");
	}

	fn rmdir(&self, path: &str) {
		let path = self.path.join(PathBuf::from(path).strip_prefix("/").unwrap());
		let _output = Command::new("ipfs")
			.arg("files")
			.arg("rm")
			.arg("-r")
			.arg(path)
			.output()
			.expect("Failed to execute command");
	}
}

impl PathFilesystem for IpfsMfs {
	async fn init(&self, _req: Request) -> Result<ReplyInit> {
		println!("init {:?}", self.path);
		//TODO: validate ipfs is reachable
		Ok(ReplyInit {
			max_write: NonZeroU32::new(16 * 1024).unwrap(),
		})
	}

	async fn destroy(&self, _req: Request) {
		// NOOP
	}

	async fn lookup(&self, _req: Request, parent: &OsStr, name: &OsStr) -> Result<ReplyEntry> {
		println!("lookup {:?} {:?}", parent, name);
		let mut path = PathBuf::from(parent);
		path.push(name);
		let stat = self.stat(path.to_str().unwrap()).ok_or_else(|| Errno::from(ENOENT))?;

		Ok(ReplyEntry { ttl: TTL, attr: stat })
	}

	async fn forget(&self, _req: Request, inode: &OsStr, _nlookup: u64) {
		println!("forget {:?}", inode);
		//TODO: free up path in self.entries
	}

	async fn getattr(&self, _req: Request, path: Option<&OsStr>, _fh: Option<u64>, _flags: u32) -> Result<ReplyAttr> {
		let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
		println!("getattr {:?}", path);

		let res = self.stat(&path).ok_or_else(|| Errno::from(ENOENT))?;

		Ok(ReplyAttr { ttl: TTL, attr: res })
	}

	async fn setattr(
		&self,
		_req: Request,
		path: Option<&OsStr>,
		_fh: Option<u64>,
		_set_attr: SetAttr,
	) -> Result<ReplyAttr> {
		let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
		println!("setattr {:?}", path);

		let res = self.stat(&path).ok_or_else(|| Errno::from(ENOENT))?;

		Ok(ReplyAttr { ttl: TTL, attr: res })
	}

	async fn mkdir(
		&self,
		_req: Request,
		parent: &OsStr,
		name: &OsStr,
		_mode: u32, //TODO: use this if applicable
		_umask: u32,
	) -> Result<ReplyEntry> {
		println!("mkdir {:?} {:?}", parent, name);
		let mut path = PathBuf::from(parent);
		path.push(name);
		let path = path.to_str().unwrap();

		WriteableBaseFs::mkdir(self, path);

		//TODO: better error handling/throwing

		let stat = self.stat(path).ok_or_else(|| Errno::from(ENOENT))?;

		Ok(ReplyEntry { ttl: TTL, attr: stat })
	}

	async fn unlink(&self, _req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
		println!("unlink {:?} {:?}", parent, name);
		let mut path = PathBuf::from(parent);
		path.push(name);
		let path = path.to_str().unwrap();

		self.rm(path);

		//TODO: better error handling/throwing

		Ok(())
	}

	async fn rmdir(&self, _req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
		println!("rmdir {:?} {:?}", parent, name);
		let mut path = PathBuf::from(parent);
		path.push(name);
		let path = path.to_str().unwrap();

		WriteableBaseFs::rmdir(self, path);

		//TODO: better error handling/throwing

		Ok(())
	}

	async fn rename(
		&self,
		_req: Request,
		parent: &OsStr,
		name: &OsStr,
		new_parent: &OsStr,
		new_name: &OsStr,
	) -> Result<()> {
		println!("rename {:?} {:?} {:?} {:?}", parent, name, new_parent, new_name);
		let mut source = PathBuf::from(parent);
		source.push(name);
		let source = source.to_str().unwrap();
		let mut target = PathBuf::from(new_parent);
		target.push(new_name);
		let target = target.to_str().unwrap();

		self.mv(source, target);

		//TODO: better error handling/throwing

		Ok(())
	}

	async fn open(&self, _req: Request, path: &OsStr, _flags: u32) -> Result<ReplyOpen> {
		println!("open {:?}", path);
		let path = path.to_string_lossy();
		let stat = self.stat(&path).ok_or_else(|| Errno::from(ENOENT))?;

		if stat.kind != FileType::Directory {
			Ok(ReplyOpen { fh: 0, flags: 0 })
		} else {
			Err(libc::EISDIR.into())
		}
	}

	async fn read(&self, _req: Request, path: Option<&OsStr>, _fh: u64, offset: u64, size: u32) -> Result<ReplyData> {
		println!("read {:?} {:?} {:?}", path, offset, size);
		let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
		let data = ReadonlyBaseFs::read(self, &path, offset, size);
		Ok(ReplyData { data: data.into() })
	}

	async fn write(
		&self,
		_req: Request,
		path: Option<&OsStr>,
		_fh: u64,
		offset: u64,
		data: &[u8],
		_write_flags: u32,
		_flags: u32,
	) -> Result<ReplyWrite> {
		println!("write {:?} {:?}", path, offset);
		let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();

		WriteableBaseFs::write(self, &path, offset, data);

		Ok(ReplyWrite {
			written: data.len() as _,
		})
	}

	async fn create(
		&self,
		_req: Request,
		parent: &OsStr,
		name: &OsStr,
		_mode: u32, //TODO: use this if applicable
		flags: u32, //TODO: use this if applicable
	) -> Result<ReplyCreated> {
		println!("create {:?} {:?}", parent, name);
		let mut path = PathBuf::from(parent);
		path.push(name);
		let path = path.to_str().unwrap();

		WriteableBaseFs::create(self, path);

		let stat = self.stat(path).ok_or_else(|| Errno::from(ENOENT))?;

		Ok(ReplyCreated {
			ttl: TTL,
			attr: stat,
			generation: 0,
			fh: 0,
			flags,
		})
	}

	async fn fallocate(
		&self,
		_req: Request,
		path: Option<&OsStr>,
		_fh: u64,
		_offset: u64,
		_length: u64,
		_mode: u32,
	) -> Result<()> {
		println!("fallocate {:?}", path);
		let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
		let stat = self.stat(&path).ok_or_else(|| Errno::from(ENOENT))?;

		if stat.kind != FileType::Directory {
			Ok(())
		} else {
			Err(libc::EISDIR.into())
		}
	}

	async fn readdir(
		&self,
		_req: Request,
		path: &OsStr,
		_fh: u64,
		offset: i64,
	) -> Result<ReplyDirectory<impl Stream<Item = Result<DirectoryEntry>> + Send + '_>> {
		println!("readdir {:?} {:?}", path, offset);
		let path = path.to_str().unwrap();
		let mut items = Vec::from([".".to_owned(), "..".to_owned()]);
		let mut names = self.ls(path);
		items.append(&mut names);
		let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;

		let mut res = Vec::new();
		for (i, name) in items.into_iter().enumerate().skip(to_skip) {
			let mut path = PathBuf::from(path);
			path.push(&name);
			let path = path.to_str().unwrap();
			let stat = self.stat(path).unwrap();
			res.push(Ok(DirectoryEntry {
				kind: stat.kind,
				name: name.into(),
				offset: (i + 1) as i64,
			}));
		}

		Ok(ReplyDirectory {
			entries: stream::iter(res),
		})
	}

	async fn readdirplus(
		&self,
		_req: Request,
		path: &OsStr,
		_fh: u64,
		offset: u64,
		_lock_owner: u64,
	) -> Result<ReplyDirectoryPlus<impl Stream<Item = Result<DirectoryEntryPlus>> + Send + '_>> {
		println!("readdirplus {:?} {:?}", path, offset);
		let path = path.to_str().unwrap();
		let mut items = Vec::from([".".to_owned(), "..".to_owned()]);
		let mut names = self.ls(path);
		items.append(&mut names);
		let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;

		let mut res = Vec::new();
		for (i, name) in items.into_iter().enumerate().skip(to_skip) {
			let mut path = PathBuf::from(path);
			path.push(&name);
			let path = path.to_str().unwrap();
			let stat = self.stat(path).unwrap();
			res.push(Ok(DirectoryEntryPlus {
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

	async fn lseek(
		&self,
		_req: Request,
		path: Option<&OsStr>,
		_fh: u64,
		offset: u64,
		whence: u32,
	) -> Result<ReplyLSeek> {
		println!("lseek {:?}", path);
		let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
		let stat = self.stat(&path).ok_or_else(|| Errno::from(ENOENT))?;

		let whence = whence as i32;

		if stat.kind != FileType::Directory {
			let offset = if whence == libc::SEEK_CUR || whence == libc::SEEK_SET {
				offset
			} else if whence == libc::SEEK_END {
				let content_size = stat.size;

				if content_size >= offset as _ {
					content_size - offset
				} else {
					0
				}
			} else {
				return Err(libc::EINVAL.into());
			};

			Ok(ReplyLSeek { offset })
		} else {
			Err(libc::EISDIR.into())
		}
	}

	async fn statfs(&self, _req: Request, _path: &OsStr) -> Result<ReplyStatFs> {
		println!("statfs");
		Ok(STATFS)
	}

	async fn copy_file_range(
		&self,
		req: Request,
		path: Option<&OsStr>,
		fh_in: u64,
		off_in: u64,
		path_out: Option<&OsStr>,
		fh_out: u64,
		off_out: u64,
		length: u64,
		flags: u64,
	) -> Result<ReplyCopyFileRange> {
		println!("copy_file_range");
		let data = PathFilesystem::read(self, req, path, fh_in, off_in, length as _).await?;

		let data = data.data.as_ref();

		let ReplyWrite { written } =
			PathFilesystem::write(self, req, path_out, fh_out, off_out, data, 0, flags as _).await?;

		Ok(ReplyCopyFileRange {
			copied: u64::from(written),
		})
	}
}
