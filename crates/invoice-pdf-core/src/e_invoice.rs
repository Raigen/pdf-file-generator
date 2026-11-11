//! The ZUGFeRD profile declared for an E-Invoice XML (embedding: issue #9).

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EInvoiceProfile {
    Minimum,
    BasicWl,
    Basic,
    En16931,
    Extended,
    XRechnung,
}

impl EInvoiceProfile {
    pub const ALL: [EInvoiceProfile; 6] = [
        Self::Minimum,
        Self::BasicWl,
        Self::Basic,
        Self::En16931,
        Self::Extended,
        Self::XRechnung,
    ];

    /// The profile name as written in the ZUGFeRD specification.
    pub fn name(self) -> &'static str {
        match self {
            Self::Minimum => "MINIMUM",
            Self::BasicWl => "BASIC WL",
            Self::Basic => "BASIC",
            Self::En16931 => "EN 16931",
            Self::Extended => "EXTENDED",
            Self::XRechnung => "XRECHNUNG",
        }
    }
}

impl fmt::Display for EInvoiceProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for EInvoiceProfile {
    type Err = String;

    /// Accepts the specification names case-insensitively, with spaces,
    /// hyphens or underscores optional (`EN16931`, `en-16931`, `BASIC WL`, `basic_wl`).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let wanted = normalize(s);
        Self::ALL
            .into_iter()
            .find(|profile| normalize(profile.name()) == wanted)
            .ok_or_else(|| {
                let names: Vec<_> = Self::ALL.iter().map(|p| p.name()).collect();
                format!(
                    "unknown ZUGFeRD profile `{s}`; expected one of {}",
                    names.join(", ")
                )
            })
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_specification_names_and_cli_spellings() {
        assert_eq!("EN16931".parse(), Ok(EInvoiceProfile::En16931));
        assert_eq!("EN 16931".parse(), Ok(EInvoiceProfile::En16931));
        assert_eq!("basic-wl".parse(), Ok(EInvoiceProfile::BasicWl));
        assert_eq!("BASIC WL".parse(), Ok(EInvoiceProfile::BasicWl));
        assert_eq!("XRechnung".parse(), Ok(EInvoiceProfile::XRechnung));
        for profile in EInvoiceProfile::ALL {
            assert_eq!(profile.name().parse(), Ok(profile));
        }
    }

    #[test]
    fn rejects_unknown_profile() {
        let error = "COMFORT".parse::<EInvoiceProfile>().unwrap_err();
        assert!(
            error.contains("unknown ZUGFeRD profile `COMFORT`"),
            "{error}"
        );
    }
}
