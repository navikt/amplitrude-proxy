use http::Uri;
use serde_json::Value;

use crate::{k8s, metrics::DEFAULT_KEY};

pub fn with_proxy_version(event: &mut Value, proxy_version: &str) {
	match event {
		Value::Object(obj) => {
			obj.get_mut("events")
				.and_then(|events| events.as_array_mut())
				.map(|events_array| {
					events_array.iter_mut().for_each(|event_obj| {
						event_obj
							.as_object_mut()
							.and_then(|event_obj_map| event_obj_map.get_mut("event_properties"))
							.and_then(|event_properties| event_properties.as_object_mut())
							.map(|inner_object| {
								inner_object.insert(
									"proxyVersion".into(),
									Value::String(proxy_version.to_string()),
								);
							});
					});
				});
		},
		Value::Array(arr) => {
			for v in arr {
				with_proxy_version(v, proxy_version);
			}
		},
		_ => {
			// No need to do anything for these types
		},
	}
}

pub fn with_app_info(event: &mut Value, app_info: &k8s::cache::AppInfo, host: &str, context: &str) {
	match event {
		Value::Object(obj) => {
			obj.get_mut("events")
				.and_then(|events| events.as_array_mut())
				.map(|events_array| {
					events_array.iter_mut().for_each(|event_obj| {
						event_obj
							.as_object_mut()
							.and_then(|event_obj_map| event_obj_map.get_mut("event_properties"))
							.and_then(|event_properties| event_properties.as_object_mut())
							.map(|inner_object| {
								inner_object
									.insert("team".into(), app_info.namespace.clone().into());
								inner_object
									.insert("ingress".into(), app_info.ingress.clone().into());
								inner_object.insert("app".into(), app_info.app_name.clone().into());
								inner_object.insert("hostname".into(), host.into());
								inner_object.insert("context".into(), context.into());
							});
					});
				});
		},
		Value::Array(arr) => {
			for v in arr {
				with_app_info(v, app_info, host, context);
			}
		},
		_ => {
			// No need to do anything for these types
		},
	}
}

pub fn with_urls(event: &mut Value, hostname: &str) {
	match event {
		Value::Object(obj) => {
			obj.get_mut("events")
				.and_then(|events| events.as_array_mut())
				.map(|events_array| {
					events_array.iter_mut().for_each(|event_obj| {
						let source_url = event_obj.as_object().and_then(|event_obj_map| {
							event_obj_map
								.get("ingestion_metadata")
								.and_then(|metadata| metadata.get("source_name"))
								.and_then(|source_name| source_name.as_str())
								.map(|s| s.to_string()) // Clone the source URL string here
						});

						let platform = event_obj.as_object().and_then(|event_obj_map| {
							event_obj_map
								.get("platform")
								.and_then(|metadata| metadata.get("source_name"))
								.and_then(|source_name| source_name.as_str())
								.map(|s| s.to_string()) // Clone the source URL string here
						});

						// Perform mutable operations in a separate scope
						if let Some(uri) = source_url {
							if let Some(inner_object) = event_obj
								.as_object_mut()
								.and_then(|event_obj_map| event_obj_map.get_mut("event_properties"))
								.and_then(|event_properties| event_properties.as_object_mut())
							{
								// Only set "[Amplitude] Page Path" if it doesn't already exist
								inner_object.insert("url".into(), Value::String(uri.to_string()));
								inner_object.insert(
									"hostname".into(),
									Value::String(
										uri.parse::<Uri>()
											.ok()
											.and_then(|u| u.host().map(|h| h.to_string()))
											.unwrap_or_default()
											.to_owned(),
									),
								);
								// Only set "[Amplitude] Page Path" if it doesn't already exist, the new client often sets this
								inner_object
									.entry("[Amplitude] Page Path".to_string())
									.or_insert_with(|| {
										// Try parsing the URI's path, or fallback to `platform` or an empty string if all else fails
										Value::String(
											uri.to_owned()
												.parse::<Uri>()
												.ok()
												.map(|parsed_uri| parsed_uri.path().to_string())
												.or_else(|| platform.clone())
												.unwrap_or_else(|| "".to_string()),
										)
									});
							}
						}
					});
				});
		},

		Value::Array(arr) => {
			for v in arr {
				with_urls(v, hostname);
			}
		},
		_ => {
			// No need to do anything for these types
		},
	}
}

pub fn with_location(value: &mut Value, city: &String, country: &String) {
	match value {
		Value::Array(arr) => {
			for v in arr {
				with_location(v, city, country);
			}
		},
		Value::Object(obj) => {
			obj.insert("city".into(), Value::String(city.to_owned()));
			obj.insert("country".into(), Value::String(country.to_owned()));
		},

		_ => {
			// No need to do anything for these types
		},
	}
}

// Sometimes there's a sentinel value in the api_key called "default"
pub fn with_key(v: &mut Value, amplitude_api_key: String) {
	if let Value::Object(obj) = v {
		match obj.get("api_key") {
			Some(Value::String(api_key)) if api_key == "default" => {
				// This could eventually be zero, then this check needs to go
				DEFAULT_KEY.inc();
				obj.insert("api_key".to_string(), Value::String(amplitude_api_key));
			},
			None => {
				obj.insert("api_key".to_string(), Value::String(amplitude_api_key));
			},
			_ => {
				// do nothing if there's an existing api_key that is not "default"
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn test_with_key_replaces_default() {
		let mut event = json!({
			"api_key": "default",
		});
		with_key(&mut event, "new_api_key".to_string());
		assert_eq!(event["api_key"], "new_api_key");
	}

	#[test]
	fn test_with_key_inserts_if_absent() {
		let mut event = json!({});
		with_key(&mut event, "new_api_key".to_string());
		assert_eq!(event["api_key"], "new_api_key");
	}

	#[test]
	fn test_with_key_does_not_change_existing_key() {
		let mut event = json!({
			"api_key": "existing_key",
		});
		with_key(&mut event, "new_api_key".to_string());
		assert_eq!(event["api_key"], "existing_key");
	}

	#[test]
	fn test_with_key_does_nothing_for_non_object() {
		let mut event = json!("not_an_object");
		with_key(&mut event, "new_api_key".to_string());
		// The value should remain unchanged
		assert_eq!(event, "not_an_object");
	}
	#[test]
	fn test_with_key_adds_api_key() {
		let amplitude_api_key = "test_api_key".to_string();
		let mut value = json!({});

		with_key(&mut value, amplitude_api_key.clone());

		assert_eq!(value["api_key"], amplitude_api_key);
	}

	#[test]
	fn test_with_key_does_not_override_existing_key() {
		let amplitude_api_key = "new_api_key".to_string();
		let mut value = json!({"api_key": "existing_api_key"});

		with_key(&mut value, amplitude_api_key);

		assert_eq!(value["api_key"], "existing_api_key");
	}

	#[test]
	fn test_annotate_with_location() {
		let mut event = json!({
			"user_id": "12345",
			"device_id": "device-98765",
			"event_type": "button_click",
			"event_properties": {
				"button_name": "signup_button",
				"color": "blue"
			},
			"session_id": 16789
		});

		with_location(&mut event, &"New York".to_string(), &"USA".to_string());

		let expected_event = json!({
			"user_id": "12345",
			"device_id": "device-98765",
			"event_type": "button_click",
			"event_properties": {
				"button_name": "signup_button",
				"color": "blue"
			},
			"session_id": 16789,
			"city": "New York",
			"country": "USA"
		});

		assert_eq!(event, expected_event);
	}

	#[test]
	fn test_annotate_with_proxy_version() {
		let mut event = json!({ "events": [{
			"user_id": "12345",
			"device_id": "device-98765",
			"event_type": "button_click",
			"event_properties": {
				"button_name": "signup_button",
				"color": "blue"
			},
		"session_id": 16789}
		]});

		with_proxy_version(&mut event, "1.2.3");

		let expected_event = json!({ "events": [
			{"user_id": "12345",
			"device_id": "device-98765",
			"event_type": "button_click",
			"event_properties": {
				"button_name": "signup_button",
				"color": "blue",
				"proxyVersion": "1.2.3"

			},
			"session_id": 16789,}
		]});

		assert_eq!(event, expected_event);
	}
	#[test]
	fn test_with_urls_updates_url_properties() {
		let hostname = "hostname";
		let mut event = json!({
			"events": [{
				"ingestion_metadata": {"source_name": "/foo/bar"},
				"event_properties": {
					"key": "value"
				}
			}]
		});

		with_urls(&mut event, hostname);

		let expected_event = json!({
			"events": [{
				"ingestion_metadata": {"source_name": "/foo/bar"},
				"event_properties": {
					"url": "/foo/bar",
					"hostname": hostname.to_owned(),
					"[Amplitude] Page Path": "/foo/bar",
					"key": "value",
				}
			}]
		});

		assert_eq!(event, expected_event);
	}

	#[test]
	fn test_with_urls_doesnt_update_existing_url_properties() {
		let url_str = "https://design.nav.no/foo/bar?query=param";

		let hostname = "hostname";
		let mut event = json!({
			"events": [{
				"event_properties": {
					"key": "value",
					"[Amplitude] Page Path": "/notouch", // Existing property that should not be updated
				}
			}]
		});

		with_urls(&mut event, hostname);

		// Expected JSON structure that preserves `[Amplitude] Page Path` as `/notouch`
		let expected_event = json!({
			"events": [{
				"event_properties": {
					"[Amplitude] Page Path": "/notouch",
					"key": "value",
				}
			}]
		});

		assert_eq!(event, expected_event);
	}
}
