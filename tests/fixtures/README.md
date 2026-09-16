# Public synthetic fixtures

Copied unchanged from [heic-rs commit 4f0d4df474c773dfc3b14fa80e7219be6898866e](https://github.com/tbraun96/heic-rs/tree/4f0d4df474c773dfc3b14fa80e7219be6898866e/tests/fixtures).
Upstream explicitly licenses these fixtures MIT OR Apache-2.0; distributed here under MIT (see LICENSE-MIT).
No private/camera photos. Upstream generated pixels in Python, encoded PNG→HEIC with macOS sips, then independently decoded HEIC→reference PNG with sips. Reference images are not original pre-lossy sources. See upstream scripts/gen-pngs.py and scripts/make-fixtures.sh at the same commit.

| File | SHA-256 |
|---|---|
| flat-64.heic | 832f19c74d689d7b9d235f5dfe6e81ed824d60900f0360d1953b413b736816ac |
| flat-64.ref.png | 39969b10938938add37de1249227d5f5a52dc9fac2c28e41a6fc0ceab494c4ca |
| gradient-512.heic | a618edb8cc6f7b53ee61bc44178014bf16c51dfb55a592be35c83611d7a236f8 |
| gradient-512.ref.png | 73b26354e73a448af511c1638840eb35614f5ba51f25bc64cc99836b49abdafe |

flat-64: 64x64 RGB (200,30,60); gradient-512: 512x512 red/green ramps, blue 128. Both single-item 8-bit HEVC, no alpha, nonzero rotation, mirror or grid. Native tests compare every RGB channel against independent references with tolerances 8 and 16 respectively; observed maxima 2 and 5. These tolerances permit lossy color-conversion rounding, not structural or dimension differences. PNG alpha must be uniformly 255.
