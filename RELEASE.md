# HEIC provider release

The shared `dekopon-agents/provider-workflows` release workflow builds from an annotated version tag on main (matching the Cargo package version). It publishes exactly three release assets: `heic-provider.wasm`, `heic-provider.wasm.sha256`, and `heic-provider.cdx.json` (CycloneDX SBOM). For a stable release it marks the GitHub release as latest; prereleases do not become latest. The workflow also publishes `ghcr.io/dekopon-agents/provider-heic:<version>` with one `application/wasm` layer.

For v0.3.0, download all three assets from the GitHub release, then check the component and its provenance from the download directory:

```sh
shasum -a 256 -c heic-provider.wasm.sha256
gh attestation verify heic-provider.wasm \
  -R dekopon-agents/dekopon-provider-heic --format json \
  --signer-repo dekopon-agents/provider-workflows \
  --source-ref refs/tags/v0.3.0 --source-digest <main-merge-SHA>
crane manifest ghcr.io/dekopon-agents/provider-heic:0.3.0
crane digest ghcr.io/dekopon-agents/provider-heic:0.3.0
```

Confirm the attested subject digest equals the sidecar's wasm SHA-256 and the annotated tag peels to the main merge SHA. The manifest must have **exactly one** `application/wasm` layer whose `sha256:` digest equals that wasm SHA-256. `crane digest` returns the **manifest** digest, not the wasm layer digest; pin the immutable `ghcr.io/dekopon-agents/provider-heic@sha256:<manifest-digest>` for deployment. Do not infer an image digest from the asset checksum or treat a release job as complete until its published artifacts and provenance verify.
