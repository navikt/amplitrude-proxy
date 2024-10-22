use serde_json::Value;
use url::Url;

use crate::k8s;

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

pub fn with_app_info(event: &mut Value, app_info: &k8s::cache::AppInfo, host: &String) {
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
								inner_object.insert("hostname".into(), host.clone().into());
							});
					});
				});
		},
		Value::Array(arr) => {
			for v in arr {
				with_app_info(v, app_info, host);
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
}
