# HEIC provider — experimental foundation

Pure-Rust, import-free `heic.convert`: bounded caller-supplied HEIC → RGBA8 PNG attachment. **Not a working HEIC chat→GPT Image pipeline or a general phone-photo converter.** No filesystem, subprocesses, network, credentials, storage, paid API calls or reusable asset IDs. JPEG output is unsupported.

## Model-facing contract

Command word: `heic`. Capability: `heic.convert`. Provider: `heic`. Effect **read-only**, risk **Low**; attachment delivery is a separately gated gateway operation.

```sh
heic --help
heic 'data:image/heic;base64,<standard-padded-base64>'
```

Direct invocation (closed schema; only `source` is accepted):

```json
{"source":"data:image/heic;base64,<standard-padded-base64>"}
```

`source` must be an exact `data:image/heic;base64,` or `data:image/heif;base64,` URL. Bare base64, remote URLs, paths, stdin, MIME parameters, whitespace in base64 and unresolved markers are rejected. `heic 'chat-asset:1'` makes a pure proposal suitable for eventual gateway expansion; **today the gateway rejects HEIC before invoke**. Do not copy image bytes into model prompts.

Schema: object; required string `source`; `additionalProperties: false`; `minLength: 1`, `maxLength: 699076`. Invoke independently enforces schema, base64/signature, metadata dimensions and actual decoded dimensions. JSON Schema is descriptive, not an authorization boundary.

Example result (base64 abbreviated):

```json
{"format":"png","width":64,"height":64,"bytes":181,"attachments":[{"mediaType":"image/png","base64":"iVBOR..."}]}
```

`attachments` is the actual reserved gateway delivery shape, not an invented handle. Gateway removes bytes and may report `attached` / `attachmentNote`; delivery can fail. No filename or reusable `chat-asset:N` is returned. External embeddings must strip attachment bytes themselves.

Stable payload-free error codes: `unsupported-capability`, `invalid-input`, `invalid-source`, `invalid-base64`, `invalid-heic`, `input-limit`, `dimension-limit`, `decode-failed`, `encode-failed`, `output-limit`. A host fuel/memory/deadline trap is a host failure, not a provider error. Unsupported HEVC features can fail decoding. Never retry a resource failure with automatically increased budgets.

## Bounds and decoder safety

| Bound | Enforced value |
|---|---|
| Decoded HEIC input | 524288 bytes (512 KiB) |
| Source string | 699076 bytes |
| Coded and displayed width/height | 1–4096 each |
| Pixels | 16777216 |
| Encoded PNG output | 8388608 bytes (8 MiB), bounded writer |
| Explicit component-test budget (not broker default) | 24000000000 fuel (24B), 64 MiB linear memory, default 30s deadline |
| Component-test input/output host envelopes | 1 MiB input, 12 MiB output (includes base64 and JSON) |

Input remains **512 KiB**; the larger PNG output cap does not widen accepted source bytes. An 8 MiB PNG needs 11184812 base64 bytes plus JSON overhead, so the default 1 MiB host output envelope is insufficient; the tests use 12 MiB. Broker constraints and gateway delivery limits can narrow output further. The pinned core's real broker default remains **8000000000 fuel (8B)**. The 24B test budget is explicit, not a change to any host-library default or deployment configuration.

Dimensions and pixels are upper guardrails, **not a guarantee that every 4K image fits**: 4096x4096 RGBA pixels alone consume 64 MiB, before decoder state or output buffers. Resource traps below these caps are expected; retain bounded host memory and a finite deadline.

**Host isolation is mandatory for hostile bytes.** `heic-rs` checks primary declared pixels but does not propagate that ceiling into HEVC SPS/internal plane allocations; grids retain multiple decoded tiles. Container probing is not a memory guarantee. Wasmtime must enforce memory, fuel, deadline and wire ceilings. The native Rust entry point exists for trusted test fixtures only. No claim of decoder hardening/fuzz coverage or native-safe arbitrary input is made. Source buffers and errors are not logged by this provider.

`heic-rs =0.1.1`, defaults off, `std` only (no Rayon/native codec). PNG encoding uses `png =0.18.0`. Dependency copyright licenses are permissive; HEVC patent rights remain unresolved and are not granted by this repository's MIT license.

Output is always 8-bit RGBA. Upstream decoder attempts HEIF `clap`/`irot`/`imir`, auxiliary alpha, Main/Main10 and grids with strict primary-property handling. Only single-item 8-bit public fixtures with no alpha and zero rotation have independent pixel-reference assertions here. A separate local photo diagnostic established bounded conversion, not general camera or color correctness. Alpha, nonzero rotation/mirror, grids and 10-bit remain **unvalidated**. Untagged provider output differs from Apple color-managed output; the decoder/color-fidelity discrepancy is unresolved. 10-bit output is reduced to 8-bit; no HDR tone mapping, ICC color management, EXIF-only orientation or metadata preservation is promised. No resizing bypasses decode bounds. 12-bit, video/inter-prediction, overlays and other codecs are not supported claims.

## Grants and integration blockers

Effective capability reach comes from the broker's capability answer for the authenticated principal/Agent context, backed by matching broker constraints and Cedar authorization; route attachment delivery is separately gated. In the pinned core, Agent catalog `capabilities`/`providers` fields are informational/inert, not an additional reach requirement or authorization grant. No HTTP/storage grants are needed; no `chat.asset.read/write` grant exists in this ABI. Illustrative fragments, not a runnable deployment:

```yaml
# Broker constraintSets
heic.convert:
  provider: heic
  effect: read-only
  risk: Low
  constraints:
    timeoutMs: 30000
    maxOutputBytes: 12582912
```

```cedar
permit(principal == Dekopon::Principal::"artist",
       action == Dekopon::Action::"heic.convert",
       resource == Dekopon::Provider::"heic")
when { context has agent && context.agent == "image-studio"
    && context has via && context.via == "dekopond-gateway" };
```

Grant `agent.prompt` separately on that exact Agent to the intended principal via the gateway. Route `providerAttachments: {maxPerReply: 1}` requests delivery; `chatAssetInputs: [heic.convert]` opts into input expansion but **does not fix the current MIME refusal** or authorize conversion. No grants should be copied blindly into a deployment.

Pinned [core fcb484bf17581735f47c89c1d7737729d28d6b6d](https://github.com/dekopon-agents/dekopon/tree/fcb484bf17581735f47c89c1d7737729d28d6b6d): `crates/dekopond/src/asset.rs` permits only PNG/JPEG/WebP/GIF at capability fetch; `crates/dekopon-agent/src/attachment.rs` expands only images and returns PNG attachments without reusable IDs. Therefore HEIC chat ingestion and downstream GPT editing remain **blocked on separately authorized core work**. Do not add HEIC to model-readable images as a shortcut. This repository changes no core/deployment code.

Typical phone-photo support cannot responsibly be claimed from these tests: the unchanged 512 KiB input cap still excludes many camera images. Broader support requires independent camera/grid/Main10/orientation fixtures, decoder resource validation, a bounded gateway input/output bridge and owner-approved budgets. Current limits are experimental, not a general conversion service.

## Build and verify

Rust 1.98.1; SDK and native testkit pinned to core Git commit above; lockfile committed. Primary ABI: `wasm32-unknown-unknown` core Wasm wrapped in an import-free component; **not WASI**. An additional `cargo build --locked --lib --release --target wasm32-wasip1` passed with kache; this is compile-only portability evidence, not a supported broker component or WASI execution proof. Guest exports exactly `describe`, `invoke`, `run-command`.

Shared CI is pinned to [provider-workflows 4cb9276ca166bee05c04e4c40ad9bca4b1f1065c](https://github.com/dekopon-agents/provider-workflows/tree/4cb9276ca166bee05c04e4c40ad9bca4b1f1065c). Use its exact `build.sh` from that checkout (wasm-tools 1.259.0), then:

```sh
export RUSTC_WRAPPER=/opt/homebrew/bin/kache # local macOS validation requirement
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --lib --target wasm32-unknown-unknown -- -D warnings
/path/to/pinned/provider-workflows/build.sh
DEKOPON_PROVIDER_COMPONENT="$PWD/heic-provider.wasm" cargo test --locked
```

No component path means tests fail, never skip. Component test asserts zero imports and exact three exports, runs real decode through the broker, checks pixels/signature/dimensions, and proves invoke-stage fuel exhaustion. Native tests cover schema/refusal, truncation, dimensions and output-writer bounds. Public fixture sources/licenses/checksums are in [tests/fixtures/README.md](tests/fixtures/README.md).

Observed locally on Apple Silicon / Wasmtime 48.0.2:
- 64x64 solid: native and 24B-fuel/64MiB component success; PNG 181 bytes; max RGB difference 2 vs independent Apple decode.
- 512x512 gradient: native and 24B-fuel/64MiB component success; PNG 149324 bytes; max RGB difference 5. Earlier diagnostics exhausted 350M and 700M fuel and succeeded at 1.4B. Routine conversion tests now assert success, not low-budget refusal for this fixture.
- 1M fuel: describe succeeds; 64x64 invoke traps specifically for exhausted fuel. This explicit low-fuel refusal remains tested.
- Separate local diagnostic: a 1352x2185 image produced 3346755 PNG bytes / 4462453 JSON bytes in actual import-free Wasm with 64 MiB memory and a 30s deadline. 18B fuel exhausted; 20B and 24B succeeded (roughly 3.8–4.9s). The 24B run passed an 8 MiB JSON output envelope. That diagnostic used scratch dimension/output limits; it informed these production guardrails but is not an exact-head photo regression test. No private image is included in this repository.

These are fixture-specific outcomes on a Mac, not Raspberry Pi timings, measured peak memory/fuel consumption or photo-performance forecasts. Full remote CI, cross-machine reproducibility, fuzzing, camera coverage and deployment are not established. CI runs on main/PR only; no tags, release workflow or publication is configured here.
