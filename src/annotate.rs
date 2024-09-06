use maxminddb::Reader;
use serde_json::Value;

pub fn annotate_with_proxy_version(event: &mut Value, proxy_version: &str) {
	if let Value::Object(map) = event {
		map.insert(
			"proxyVersion".to_string(),
			Value::String(proxy_version.to_string()),
		);
	}
}

pub fn annotate_with_location(event: &mut Value, reader: &Reader<Vec<u8>>) {}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

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

		annotate_with_proxy_version(&mut event, "1.2.3");

		let expected_event = json!({
			"user_id": "12345",
			"device_id": "device-98765",
			"event_type": "button_click",
			"event_properties": {
				"button_name": "signup_button",
				"color": "blue"
			},
			"session_id": 16789,
			"proxyVersion": "1.2.3"  // The new field added
		});

		assert_eq!(event, expected_event);
	}

	#[test]
	fn test_annotate_proxy_version_overwrite() {
		let mut event = json!({
			"user_id": "12345",
			"proxyVersion": "1.0.0"
		});

		annotate_with_proxy_version(&mut event, "2.0.0");

		let expected_event = json!({
			"user_id": "12345",
			"proxyVersion": "2.0.0"  // The field should be updated to 2.0.0
		});

		assert_eq!(event, expected_event);
	}
}