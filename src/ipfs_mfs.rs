use fuse::{FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request};
use libc::ENOENT;
use regex::Regex;
use std::ffi::{OsStr, OsString, c_int};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::vec::Vec;
use time::Timespec;

//const TTL: Duration = Duration::from_secs(1);
const TTL: Timespec = Timespec { sec: 1, nsec: 0 };
const CREATE_TIME: Timespec = Timespec { sec: 1381237736, nsec: 0 };

pub struct IpfsMfs {
	// IPFS path to mount
	path: OsString,

	// Internal store to map ino to path
	entries: Vec<String>,
}

impl IpfsMfs {
	pub fn new(path: OsString) -> Self {
		Self { path, entries: Vec::new() }
	}

	fn stat(&mut self, path: &String) -> Option<FileAttr> {
		if !self.entries.contains(path) {
			self.entries.push(path.clone());
		}

		let re = Regex::new(r"^[A-Za-z0-9]+\nSize: (\d+)\nCumulativeSize: (\d+)\nChildBlocks: (\d+)\nType: (directory|file)").unwrap();
		let output = Command::new("ipfs")
			.arg("files")
			.arg("stat")
			.arg(path)
			.output()
			.expect("Failed to execute command");

		let out = String::from_utf8(output.stdout).expect("Failed to parse output");
		let res = re.captures(&out);

		if res.is_none() {
			return None;
		}

		let caps = res.unwrap();

		let ino = (self.entries.iter().position(|r| r == path).unwrap() + 1) as u64;
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
			atime: CREATE_TIME, // 1970-01-01 00:00:00
			mtime: CREATE_TIME,
			ctime: CREATE_TIME,
			crtime: CREATE_TIME,
			kind,
			perm: 0o755,
			nlink: 2,
			uid: 501,
			gid: 20,
			rdev: 0,
			flags: 0,
		})
	}

	fn get_path(&mut self, ino: u64) -> Option<String> {
		if self.entries.len() > (ino - 1) as usize {
			return Some(self.entries[(ino - 1) as usize].clone());
		}

		None
	}

	fn append_path(&mut self, mut path: String, filename: &str) -> String {
		if !path.ends_with("/") {
			path.push_str("/");
		}

		path.push_str(filename);
		path
	}
}

impl Filesystem for IpfsMfs {
	fn init(&mut self, _req: &Request<'_>) -> Result<(), c_int> {
		self.stat(&self.path.clone().into_string().unwrap()); //TODO: muse proper path
		Ok(())
	}

	fn destroy(&mut self, _req: &Request<'_>) {
		self.entries = Vec::new();
	}

	fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
		println!("lookup {:?}", name);

		let p = self.get_path(parent);
		if !p.is_none() {
			let path = self.append_path(p.unwrap(), &name.to_str().unwrap());
			let stat = self.stat(&path);
			if !stat.is_none() {
				reply.entry(&TTL, &stat.unwrap(), 0);
				return;
			}
		}

		reply.error(ENOENT);
	}

	fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
		println!("getattr {:?}", ino);

		let path = self.get_path(ino);
		if !path.is_none() {
			let res = self.stat(&path.unwrap());
			if !res.is_none() {
				reply.attr(&TTL, &res.unwrap());
				return;
			}
		}

		reply.error(ENOENT);
	}

	fn read(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, size: u32, reply: ReplyData) {
		println!("read {:?} {:?} {:?}", ino, offset, size);
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
				reply.data(&output.stdout);
			}
			None => reply.error(ENOENT),
		}
	}

	fn readdir(&mut self, _req: &Request<'_>, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
		println!("readdir {:?} {:?}", ino, offset);

		match self.get_path(ino) {
			None => reply.error(ENOENT),
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
				for (i, name) in names.enumerate().skip(to_skip) {
					let path = self.append_path(self.entries[(ino - 1) as usize].clone(), name);
					let stat = self.stat(&path).unwrap();
					reply.add(stat.ino, (i + 1) as i64, stat.kind, name);
				}

				reply.ok();
			}
		}
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
		let mut fs = IpfsMfs { path: "/".into(), entries };

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
			entries: Vec::new(),
		};

		let path1 = fs.append_path("/".to_string(), "test");
		assert_eq!(path1, "/test");

		let path2 = fs.append_path(path1.to_string(), "test");
		assert_eq!(path2, "/test/test");
	}
}
