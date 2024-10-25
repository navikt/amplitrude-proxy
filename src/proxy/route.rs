use std::fmt::{self, Display, Formatter};

use crate::metrics::{COLLECT, COLLECT_AUTO};

#[derive(Debug, PartialEq)]
pub enum Route {
	Amplitude(String),
	AmplitudeCollect(String), // Should be deprecated shortly
	Unexpected(String),       //Someone did a goof
}

impl Display for Route {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{self:?}")
	}
}

pub fn match_route(path: String) -> Route {
	if path.starts_with("/collect-auto") {
		COLLECT_AUTO.inc();
		Route::AmplitudeCollect(path)
	} else if path.starts_with("/collect") {
		COLLECT.inc();
		Route::Amplitude(path)
	} else {
		Route::Unexpected(path)
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn show_starts_with() {
		assert_eq!(
			"/collect".starts_with("/collect"),
			"/collect-auto".starts_with("/collect")
		);
	}
}
