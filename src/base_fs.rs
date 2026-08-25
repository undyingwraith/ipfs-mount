use fuse3::raw::prelude::*;

pub trait ReadonlyBaseFs {
	fn stat(&self, path: &str) -> Option<FileAttr>;

	fn read(&self, path: &str, offset: u64, size: u32) -> Vec<u8>;

	fn ls(&self, path: &str) -> Vec<String>;
}

pub trait WriteableBaseFs {
	fn write(&self, path: &str, offset: u64, data: &[u8]);

	fn create(&self, path: &str);

	fn mkdir(&self, path: &str);

	fn mv(&self, source: &str, target: &str);

	fn rm(&self, path: &str);

	fn rmdir(&self, path: &str);
}
