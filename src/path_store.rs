use std::sync::RwLock;

#[derive(Default)]
pub struct PathStore {
	entries: RwLock<Vec<String>>,
}

impl PathStore {
	pub fn get_path(&self, ino: u64) -> Option<String> {
		if self.entries.read().unwrap().len() > (ino - 1) as usize {
			return Some(self.entries.read().unwrap()[(ino - 1) as usize].clone());
		}

		None
	}

	pub fn get_child_path(&self, ino: u64, filename: &str) -> Option<String> {
		match self.get_path(ino) {
			Some(mut path) => {
				if !path.ends_with("/") {
					path.push('/');
				}

				path.push_str(filename);
				Some(path)
			}
			None => None,
		}
	}

	pub fn has_path(&self, path: &String) -> bool {
		self.entries.read().unwrap().contains(path)
	}

	pub fn add_path(&self, path: &str) {
		self.entries.write().unwrap().push(path.to_string());
	}

	pub fn get_ino(&self, path: &String) -> Option<u64> {
		self.entries
			.read()
			.unwrap()
			.iter()
			.position(|r| r == path)
			.map(|i| (i + 1) as u64)
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
		let store = PathStore {
			entries: RwLock::new(entries),
		};

		let path1 = store.get_path(1);
		assert_eq!(path1.is_none(), false);
		assert_eq!(path1.unwrap(), "/".to_string());

		let path2 = store.get_path(2);
		assert_eq!(path2.is_none(), false);
		assert_eq!(path2.unwrap(), "/test".to_string());

		let path3 = store.get_path(3);
		assert_eq!(path3.is_none(), true);
	}

	#[test]
	fn can_get_child_path() {
		let mut entries = Vec::new();
		entries.push("/".to_string());
		entries.push("/t".to_string());
		let store = PathStore {
			entries: RwLock::new(entries),
		};

		let path1 = store.get_child_path(1, "test");
		assert_eq!(path1.is_none(), false);
		assert_eq!(path1.unwrap(), "/test".to_string());

		let path2 = store.get_child_path(2, "test");
		assert_eq!(path2.is_none(), false);
		assert_eq!(path2.unwrap(), "/t/test".to_string());
	}

	#[test]
	fn has_path_works() {
		let mut entries = Vec::new();
		entries.push("/".to_string());
		entries.push("/test".to_string());
		let store = PathStore {
			entries: RwLock::new(entries),
		};

		assert_eq!(store.has_path(&"/".to_string()), true);
		assert_eq!(store.has_path(&"/test".to_string()), true);
		assert_eq!(store.has_path(&"/nonexistant".to_string()), false);
	}

	#[test]
	fn can_add_path() {
		let store = PathStore::default();
		let path = &"/".to_string();

		store.add_path(path);

		assert_eq!(store.has_path(path), true);
	}

	#[test]
	fn can_get_ino() {
		let mut entries = Vec::new();
		entries.push("/".to_string());
		entries.push("/test".to_string());
		let store = PathStore {
			entries: RwLock::new(entries),
		};

		assert_eq!(store.get_ino(&"/".to_string()), Some(1));
		assert_eq!(store.get_ino(&"/test".to_string()), Some(2));
	}
}
