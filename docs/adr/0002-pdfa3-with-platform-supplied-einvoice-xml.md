# Every Invoice is PDF/A-3; e-invoice XML is supplied by the Platform

German B2B e-invoicing makes hybrid ZUGFeRD invoices (PDF/A-3 with embedded CII XML) a requirement, so every Invoice we produce is PDF/A-3 conformant, with or without an attached E-Invoice XML, and there is only one output path. The E-Invoice XML is supplied finished by the Platform together with its ZUGFeRD profile; we only check it is well-formed CII and embed it, keeping the rule that this service renders and never calculates or owns tax-relevant data.

## Considered Options

- **Built-in Helvetica, PDF/A later**: simplest and smallest output, but PDF/A forbids non-embedded fonts, so it would have meant a second output path or a rewrite of text handling.
- **Generating the CII XML ourselves from Invoice Data**: would require a second, typed data model alongside the pre-formatted strings and make this service responsible for tax-law correctness. May be revisited later.

## Consequences

- Fonts are always embedded and subset; the default is Liberation Sans (metric-compatible with Helvetica/Arial, licence permits embedding), and text is measured from the embedded font.
- An sRGB output intent is embedded in every Invoice; CMYK Backgrounds are rejected when loaded rather than colour-converted.
- Full business-rule validation of the XML (KoSIT, Schematron) is the Platform's job. Our CI checks every test Invoice with veraPDF (PDF/A-3) and a ZUGFeRD validator (e.g. Mustang).
