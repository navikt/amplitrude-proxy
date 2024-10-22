use serde_json::Value;
use url::Url;

use crate::k8s;

// This knows too much about the data structure, frfr.
pub fn with_proxy_version(event: &mut Value, proxy_version: &str) {
	match event {
		Value::Object(obj) => {
			if let Some(events) = obj.get_mut("events") {
				if let Value::Array(events_array) = events {
					for event_obj in events_array {
						if let Value::Object(event_obj_map) = event_obj {
							if let Some(event_properties) =
								event_obj_map.get_mut("event_properties")
							{
								if event_properties.is_object() {
									let inner_object = event_properties.as_object_mut().expect(
                                        "Should be possible to get a mutable reference to the inner object",
                                    );
									inner_object.insert(
										"proxyVersion".into(),
										Value::String(proxy_version.to_string()),
									);
								}
							}
						}
					}
				}
			}
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

pub fn with_url(event: &mut Value, event_url_obj: &Url) {
	if let Value::Object(obj) = event {
		if let Some(Value::Object(event_properties)) = obj.get_mut("event_properties") {
			event_properties.insert(
				"url".into(),
				Value::String(format!(
					"{}{}",
					event_url_obj.host_str().unwrap_or(""),
					event_url_obj.path()
				)),
			);
		}
	}
}

pub fn with_hostname(event: &mut Value, host: &str) {
	if let Value::Object(obj) = event {
		if let Some(Value::Object(event_properties)) = obj.get_mut("event_properties") {
			event_properties.insert("hostname".into(), Value::String(host.into()));
		}
	}
}

pub fn with_page_path(event: &mut Value, event_url_obj: &Url) {
	if let Value::Object(obj) = event {
		if let Some(Value::Object(event_properties)) = obj.get_mut("event_properties") {
			event_properties.insert(
				"pagePath".into(),
				Value::String(event_url_obj.path().to_string()),
			);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn test_with_url() {
		let url = Url::parse("https://navno.com/foo").unwrap();
		let mut event = json!({
			"event_properties": {}
		});

		with_url(&mut event, &url);

		assert_eq!(event["event_properties"]["url"], "navno.com/foo");
	}

	#[test]
	fn test_with_hostname() {
		let url = "navno.com";
		let mut event = json!({
			"event_properties": {}
		});

		with_hostname(&mut event, &url);

		assert_eq!(event["event_properties"]["hostname"], "navno.com");
	}

	#[test]
	fn test_with_page_path() {
		let url = Url::parse("https://navno.com/foo").unwrap();
		let mut event = json!({
			"event_properties": {}
		});

		with_page_path(&mut event, &url);

		assert_eq!(event["event_properties"]["pagePath"], "/foo");
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

		dbg!("{}", &event);
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
