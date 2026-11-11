//! Validation errors addressed by JSON pointer.

use std::fmt;

use serde::Serialize;

/// JSON pointer to the Layout inside a render request (`{layout, invoiceData}`).
pub const LAYOUT_PATH: &str = "/layout";
/// JSON pointer to the Invoice Data inside a render request (`{layout, invoiceData}`).
pub const INVOICE_DATA_PATH: &str = "/invoiceData";

/// One problem with the Layout or the Invoice Data.
///
/// `path` is a JSON pointer (RFC 6901) into the render request, e.g.
/// `/layout/table/columns/2/source` or `/invoiceData/lineItems/417/description`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Appends one reference token to a JSON pointer, escaping `~` and `/`.
pub(crate) fn join(base: &str, token: impl fmt::Display) -> String {
    let token = token.to_string().replace('~', "~0").replace('/', "~1");
    format!("{base}/{token}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_appends_plain_tokens() {
        assert_eq!(join("/invoiceData", "lineItems"), "/invoiceData/lineItems");
        assert_eq!(
            join("/invoiceData/lineItems", 417),
            "/invoiceData/lineItems/417"
        );
    }

    #[test]
    fn join_escapes_tilde_and_slash() {
        assert_eq!(
            join("/invoiceData/header", "a/b~c"),
            "/invoiceData/header/a~1b~0c"
        );
    }

    #[test]
    fn display_shows_path_and_message() {
        let error = ValidationError::new("/layout/margins/top", "must not be negative");
        assert_eq!(
            error.to_string(),
            "/layout/margins/top: must not be negative"
        );
    }
}
