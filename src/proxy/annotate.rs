use serde_json::Value;

use crate::k8s;

pub fn with_proxy_version(event: &mut Value, proxy_version: &str) {
	match event {
		Value::Array(arr) => {
			for v in arr {
				with_proxy_version(v, proxy_version);
			}
		},
		Value::Object(obj) => {
			for (key, v) in obj.iter_mut() {
				if key == "event_properties" && v.is_object() {
					let inner_object = v.as_object_mut().expect(
						"Should be possible to get a mutable reference to the inner object",
					);
					inner_object.insert(
						"proxyVersion".into(),
						Value::String(proxy_version.to_string()),
					);
				}
			}
		},

		_ => {
			// No need to do anything for these types
		},
	}
}

// some of these
// [Amplitude] City, [Amplitude] DMA, [Amplitude] Region, and [Amplitude] Country
pub fn with_location(value: &mut Value, city: &String, country: &String) {
	match value {
		Value::Array(arr) => {
			for v in arr {
				with_location(v, city, country);
			}
		},
		Value::Object(obj) => {
			for (key, v) in obj.iter_mut() {
				if key == "event_properties" && v.is_object() {
					let inner_object = v.as_object_mut().expect(
						"Should be possible to get a mutable reference to the inner object",
					);
					inner_object.insert("[Amplitude] City".into(), Value::String(city.to_owned()));
					inner_object.insert(
						"[Amplitude] Country".into(),
						Value::String(country.to_owned()),
					);
				}
			}
		},

		_ => {
			// No need to do anything for these types
		},
	}
}

pub fn with_app_info(value: &mut Value, app_info: &k8s::cache::AppInfo, host: &String) {
	match value {
		Value::Array(arr) => {
			for v in arr {
				with_app_info(v, &app_info.clone(), host);
			}
		},
		Value::Object(obj) => {
			for (key, v) in obj.iter_mut() {
				if key == "event_properties" && v.is_object() {
					let inner_object = v.as_object_mut().expect(
						"Should be possible to get a mutable reference to the inner object",
					);
					inner_object.insert("team".into(), app_info.namespace.clone().into());
					inner_object.insert("ingress".into(), app_info.ingress.clone().into());
					inner_object.insert("app".into(), app_info.app_name.clone().into());
					inner_object.insert("hostname".into(), host.clone().into());
				}
			}
		},

		_ => {
			// No need to do anything for these types
		},
	}
}

pub fn with_key(v: &mut Value, amplitude_api_key: String) {
	if let Value::Object(obj) = v {
		obj.insert("api_key".to_string(), Value::String(amplitude_api_key));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

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
				"color": "blue",
				"[Amplitude] City": "New York",
				"[Amplitude] Country": "USA"
			},
			"session_id": 16789
		});

		assert_eq!(event, expected_event);
	}

	#[test]
	fn test_annotate_with_location_existing_location() {
		let mut event = json!({
			"user_id": "12345",
			"event_properties": {
				"button_name": "signup_button",
				"color": "blue",
				"[Amplitude] City": "Los Angeles",
				"[Amplitude] Country": "Canada"
			}
		});

		with_location(&mut event, &"New York".to_string(), &"USA".to_string());

		let expected_event = json!({
			"user_id": "12345",
			"event_properties": {
				"button_name": "signup_button",
				"color": "blue",
				"[Amplitude] City": "New York",
				"[Amplitude] Country": "USA"
			}
		});

		assert_eq!(event, expected_event);
	}
	#[test]
	fn test_annotate_with_proxy_version() {
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

		with_proxy_version(&mut event, "1.2.3");

		let expected_event = json!({
			"user_id": "12345",
			"device_id": "device-98765",
			"event_type": "button_click",
			"event_properties": {
				"button_name": "signup_button",
				"color": "blue",
				"proxyVersion": "1.2.3"

			},
			"session_id": 16789,
		});

		assert_eq!(event, expected_event);
	}
}
