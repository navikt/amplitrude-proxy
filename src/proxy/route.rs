use std::fmt::{self, Display, Formatter};

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
