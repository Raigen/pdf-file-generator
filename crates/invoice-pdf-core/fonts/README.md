# Embedded fonts

The default fonts of every Invoice (ADR 0002), compiled into the binary with
`include_bytes!` by `src/font/mod.rs`.

| File                          | Layout font |
| ----------------------------- | ----------- |
| `LiberationSans-Regular.ttf`  | `regular`   |
| `LiberationSans-Bold.ttf`     | `bold`      |

- Upstream: Liberation Fonts 2.1.5, https://github.com/liberationfonts/liberation-fonts/releases/tag/2.1.5
- Archive: https://github.com/liberationfonts/liberation-fonts/files/7261482/liberation-fonts-ttf-2.1.5.tar.gz
  (SHA-256 `7191c669bf38899f73a2094ed00f7b800553364f90e2637010a69c0e268f25d0`)
- Files copied unmodified from that archive:
  - `LiberationSans-Regular.ttf` (SHA-256 `76d04c18ea243f426b7de1f3ad208e927008f961dc5945e5aad352d0dfde8ee8`)
  - `LiberationSans-Bold.ttf` (SHA-256 `788abee4c806d660e8aee46689dd8540cd4bb98da03dcc9d171ce3efd99a9173`)
  - `LICENSE` (the archive's licence file)

## Licence

Liberation Sans is licensed under the SIL Open Font License, Version 1.1; the
full text is in [`LICENSE`](LICENSE). The OFL permits embedding the fonts (and
subsets of them) in documents. "Liberation" is a Reserved Font Name: the font
files must not be modified and redistributed under that name. Subsets embedded
in PDF documents are not such a redistribution.
