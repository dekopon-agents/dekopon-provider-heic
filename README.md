# HEIC provider 0.3.0 — experimental bounded conversion

`heic.convert` reads a broker-owned HEIC asset and attaches a reusable RGBA8 PNG asset. Command word `heic`; effect **local-write**, risk **Low**. Uses Dekopon SDK/testkit 0.34.0 and the stdio provider interface. No filesystem paths, network requests, subprocesses, credentials or durable storage.

```sh
heic chat-asset:1
# If separately authorized, send the actual reference reported by the gateway:
asset send <returned-asset-id>
```

Input is exactly `{"source":"chat-asset:1"}`. Only `chat-asset:<N>` references are accepted; data URLs, bare base64, paths, URLs, extra fields and stdin are refused. `run-command` is pure; the gateway passes referenced descriptors separately when authorizing `invoke`. Listing metadata does not grant access to unrelated references.

Invocation: open → bounded read_all → decode HEIC → encode PNG → allocate(image/png, identity) → write_all → attach. It writes metadata JSON followed by one newline to stdout, for example:

```json
{"format":"png","width":64,"height":64,"bytes":181,"assetNote":{"id":null,"contentType":"image/png","bytes":181}}
```

The attach handle has no conversation ID yet. The gateway numbers it and appends its own reference note. **Attach is not send**: delivery requires a separately authorized `asset.send`. Results never contain an `attachments` envelope or image bytes. Failed imports propagate without retry; unattached writers are discarded.

## Bounds and decoder safety

- HEIC input: **512 KiB decoded**, unchanged. A known stored length is checked before `read_all` (accounting for identity/base64 storage); decoded length is checked again. The host performs storage decoding.
- Coded/displayed dimensions: 1–4096 each, at most 16,777,216 pixels.
- PNG output: at most **8 MiB**, enforced by a bounded encoder sink and host asset limits.
- Metadata-only JSON no longer requires a large base64 output envelope. Host fuel, memory and deadlines still constrain decoding; these ceilings are not guarantees that every 4K image fits. 4096² RGBA pixels alone occupy 64 MiB.

**Host isolation is mandatory for hostile input.** heic-rs does not propagate the primary pixel ceiling into all HEVC internal allocations; grids retain decoded tiles. Container probing is not a memory guarantee. Use bounded Wasm memory, fuel and deadline; the native seam is for trusted fixtures only. No retry with increased budgets. The 512 KiB ceiling excludes many phone photos.

Decoder remains `heic-rs =0.1.1`, defaults off, `std` only; PNG encoder `png =0.18.0`. HEVC patent rights are not granted by the repository's MIT license. Output is 8-bit RGBA; no HDR tone mapping, ICC management, EXIF-only orientation or metadata preservation is promised. Independent Apple reference tests cover two single-item 8-bit fixtures only; alpha, nonzero rotation/mirror, grids, Main10 and broader camera fidelity remain unvalidated. No claim of native-safe arbitrary input, fuzz coverage or general phone-photo support.

## Example authorization

Illustrative broker constraints; Cedar must independently allow the principal and capability:

```yaml
capabilities:
  heic:
    constraints:
      timeoutMs: 30000
    capabilities:
      heic.convert:
        constraints:
          asset:
            attach: true
```

The host-wide `hostLimits.maxOutputBytes` bounds stdout; there is no per-provider `maxOutputBytes` key.

Reading a passed input requires no separate asset read grant. No HTTP/storage grants are needed. Send/remove grants are not implied by attach. Configure the broker's asset directory separately. Legacy `providerAttachments`/`chatAssetInputs` route settings are not used.

## Build and tests

Use the pinned Rust toolchain and shared provider-workflows build script:

```sh
cargo fmt --all --check
cargo deny --all-features check bans licenses sources advisories
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked --package dekopon-heic-provider --lib --target wasm32-unknown-unknown -- -D warnings
/path/to/provider-workflows/build.sh
DEKOPON_PROVIDER_COMPONENT="$PWD/heic-provider.wasm" cargo test --locked
```

Component ABI is wasm32-unknown-unknown (not WASI), with only `dekopon:asset/asset@0.1.0` and `dekopon:stdio/streams@0.1.0` imports and the provider 0.4 describe/invoke/run-command exports. SDK/testkit are crates.io `=0.34.0`. CI/release uses provider-workflows at its effective `main` revision.

Native fake-asset tests exercise the complete open/read/allocate/write/attach path, independent pixel references, input bounds, truncation, dimensions, output bounds and fail-fast import errors. Component tests require `DEKOPON_PROVIDER_COMPONENT`, verify imports/exports, pure proposals/help and missing-reference refusal. The published testkit has no asset-input builder, so component tests do not claim a successful asset conversion or the historical data-URL fuel measurements. Fixture provenance is in `tests/fixtures/README.md`.
