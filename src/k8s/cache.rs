use lru::LruCache;
use once_cell::sync::Lazy;
use ptrie::Trie;
use std::num::NonZeroUsize;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub static CACHE: Lazy<Arc<Mutex<LruCache<String, AppInfo>>>> = Lazy::new(|| {
	Arc::new(Mutex::new(LruCache::new(
		NonZeroUsize::new(2000).expect("cache has positive capacity"),
	)))
});

// Would it make sense to prefix-lookup on appInfo records instead? Can I do that? This only exists for longest-prefix matching
pub static PREFIX_TRIE: Lazy<Arc<Mutex<Trie<u8, String>>>> =
	Lazy::new(|| Arc::new(Mutex::new(Trie::new())));

// This keeps tracks of if the k8s exfiltration thread has spawned
// AtomicBool uses atomic operations provided by the CPU to ensure that reads and writes to the boolean value are indivisible (i.e atomic!). This means that no thread can see a partially-updated value. Its pretty neat. imho
pub static INITIALIZED: AtomicBool = AtomicBool::new(false);

// This is the appinfo struct as per the ingress-collector app
#[derive(Clone, Debug, PartialEq)]
pub struct AppInfo {
	pub app_name: String,
	pub namespace: String,
	pub ingress: String,
	pub creation_timestamp: String,
}

pub fn insert_into_cache(key: String, value: AppInfo) {
	CACHE
		.lock()
		.expect("Failed to lock cache")
		.put(key.clone(), value);

	PREFIX_TRIE
		.lock()
		.expect("Failed to lock trie")
		.insert(key.clone().bytes(), key);
}

pub fn get_app_info_with_longest_prefix(key: &str) -> Option<AppInfo> {
	if let Some(longest_prefix) = PREFIX_TRIE
		.lock()
		.expect("Failed to lock trie")
		.find_longest_prefix(key.bytes())
	{
		return CACHE
			.lock()
			.expect("Failed to lock cache")
			.get(&longest_prefix.clone())
			.cloned();
	}
	None
}

pub fn get_app_info(key: &str) -> Option<AppInfo> {
	if let Some(longest_prefix) = PREFIX_TRIE
		.lock()
		.expect("Failed to lock trie")
		.get(key.bytes())
	{
		return CACHE
			.lock()
			.expect("Failed to lock cache")
			.get(&longest_prefix.clone())
			.cloned();
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_insert_and_retrieve_from_cache() {
		let key = "test-key".to_owned();
		let app_info = AppInfo {
			app_name: "test-app".to_owned(),
			namespace: "test-namespace".to_owned(),
			ingress: "test-ingress".to_owned(),
			creation_timestamp: "2023-01-01T00:00:00Z".to_owned(),
		};

		insert_into_cache(key.clone(), app_info.clone());

		let retrieved_app_info = get_app_info_with_longest_prefix(&key.clone());
		assert!(
			retrieved_app_info.is_some(),
			"AppInfo should be present in cache"
		);
		assert_eq!(
			retrieved_app_info.unwrap(),
			app_info,
			"Retrieved AppInfo should match inserted value"
		);

		let prefix_key = "test-key-key-key".to_owned();
		let prefix_app_info = get_app_info_with_longest_prefix(&prefix_key).unwrap();
		assert_eq!(
			prefix_app_info, app_info,
			"Prefix-based retrieval should match inserted AppInfo"
		);

		let empty_key = "".to_owned();
		let prefix_app_info = get_app_info_with_longest_prefix(&empty_key);
		assert_eq!(
			prefix_app_info, None,
			"Prefix-based retrieval should match inserted AppInfo"
		);

		let dagpenger_key = "https://www.nav.no/arbeid/dagpenger/meldekort/".to_owned();

		let dagpenger_app_info = AppInfo {
			app_name: "dagpenger_app".to_owned(),
			namespace: "dagpenger-namespace".to_owned(),
			ingress: "https://www.nav.no/arbeid/dagpenger/meldekort/".to_owned(),
			creation_timestamp: "2023-01-01T00:00:00Z".to_owned(),
		};

		insert_into_cache(dagpenger_key.clone(), dagpenger_app_info.clone());

		let foo = PREFIX_TRIE
			.lock()
			.expect("Failed to lock trie")
			.iter()
			.for_each(|x| {
				dbg!(x);
			});

		let dagpenger_window_location = "https://www.nav.no/arbeid/dagpenger/meldekort/";
		let prefix_app_info = get_app_info_with_longest_prefix(&dagpenger_window_location).unwrap();
		assert_eq!(prefix_app_info, dagpenger_app_info, "get meldekort");
	}
}
