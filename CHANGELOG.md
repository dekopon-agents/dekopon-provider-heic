# Changelog

## [0.3.0] - 2026-10-06

- Move heic.convert to typed stdio proposals and JSON-line receipts while preserving bounded asset attachment.
- Require only SDK Assets and stdio imports, with component conformance and native pixel-reference coverage.
- Malformed heic arguments return the SDK's standard usage error, which names the rejected argument.

## [0.2.0] - 2026-09-20

- Move to Dekopon SDK/testkit 0.18.0. Read only chat asset handles and attach reusable PNG outputs without byte envelopes; attachment now declares a local-write effect.
- Preserve the 512 KiB HEIC input, dimension and 8 MiB PNG bounds; test handle I/O natively and component proposals/refusals.
- Add shared release workflows for the first tagged release.

## [0.1.0] - 2026-09-20

- Experimental bounded HEIC decoder foundation (untagged).
