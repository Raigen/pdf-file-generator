# Rust with `pdf-writer`

We build the generator in Rust on top of `pdf-writer` (the low-level PDF writer from the Typst project) rather than in Go. Neither language is known to the team, so the deciding factor was the library: Go's PDF libraries (gopdf, fpdf) are higher-level and opinionated about layout, while `pdf-writer` is proven, fast and low-level, leaving pagination of the Line Item Table entirely under our control and allowing page-by-page streamed output later if Invoice sizes grow.

## Consequences

- Text measurement (needed for wrapping and pagination) and font subsetting are ours to supply, via the `ttf-parser` and `subsetter` crates from the same ecosystem; `pdf-writer` does not lay out text.
- v1 buffers the complete PDF before responding, so a failure at any point still produces a proper HTTP error instead of a truncated document.
