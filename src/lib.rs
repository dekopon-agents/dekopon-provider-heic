//! Bounded HEIC conversion through broker-owned assets. Host isolation is mandatory for decoding.
mod assets;
use assets::{AssetAccess, Host};
use clap::Parser;
use dekopon_provider_sdk::asset::Encoding;
use dekopon_provider_sdk::provider::{
    Assets, Capability, Code, Failure, Proposal, Provider, Stdout, Usage,
};
use dekopon_provider_sdk::{EffectKind, RiskLevel};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fmt, io::Write};

const MAX_INPUT_BYTES: usize = 524_288;
const MAX_SOURCE: usize = 31;
const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: u64 = 16_777_216;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Parser)]
#[command(
    name = "heic",
    version,
    about = "HEIC asset to PNG: 512 KiB input, 8 MiB output, 4096x4096, 16777216 pixels",
    after_help = "Host resource limits may refuse smaller images. Attaches a reusable asset; use asset send to deliver it."
)]
pub struct HeicArgs {
    /// A positional `chat-asset:<N>` reference, not a URL or path
    #[arg(value_name = "SOURCE")]
    source: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    /// `chat-asset:<N>`; at most 512 KiB decoded HEIC input
    #[schemars(length(min = 12, max = 31), regex(pattern = "^chat-asset:[0-9]+$"))]
    source: String,
}

#[derive(Debug)]
pub struct HeicError {
    code: &'static str,
    message: &'static str,
}
impl fmt::Display for HeicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}
impl Failure for HeicError {
    fn code(&self) -> Code {
        Code::new(self.code)
    }
}
fn error(code: &'static str, message: &'static str) -> HeicError {
    HeicError { code, message }
}
fn reference(source: &str) -> Result<(), HeicError> {
    let digits = source.strip_prefix("chat-asset:").unwrap_or("");
    if source.len() > MAX_SOURCE
        || digits.is_empty()
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || digits.parse::<u64>().is_err()
    {
        return Err(error(
            "invalid-source",
            "source must be chat-asset:<N>; data URLs, paths and URLs are unsupported",
        ));
    }
    Ok(())
}

/// Provider entry point; native decoding is intended for trusted test fixtures only.
pub struct HeicProvider;
/// One local attachment, never a send.
pub struct Convert;
impl Provider for HeicProvider {
    const ID: &'static str = "heic";
    const COMMAND_WORDS: &'static [&'static str] = &["heic"];
    const DESCRIPTION: &'static str = "Experimental bounded HEIC asset to reusable PNG asset";
    type Args = HeicArgs;
    type Capabilities = (Convert,);
    fn propose(args: Self::Args, stdin_piped: bool) -> Result<Proposal<Self>, Usage> {
        if stdin_piped {
            return Err(Usage::new(
                "stdin is unsupported; supply one source argument",
            ));
        }
        let source = args
            .source
            .ok_or_else(|| Usage::new("supply one positional chat-asset:<N> reference"))?;
        reference(&source).map_err(|_| Usage::new("source must be chat-asset:<N>"))?;
        Ok(Proposal::to::<Convert>(Input { source }))
    }
}
impl Capability for Convert {
    type Provider = HeicProvider;
    const NAME: &'static str = "convert";
    const DESCRIPTION: &'static str = "Decode at most 512 KiB HEIC, 4096x4096 / 16777216 pixels, into at most 8 MiB PNG. Host resource limits may refuse smaller images. Attaches without sending.";
    const EFFECT: EffectKind = EffectKind::LocalWrite;
    const RISK: RiskLevel = RiskLevel::Low;
    type Input = Input;
    type Needs = Assets;
    type Error = HeicError;
    fn run(input: Input, assets: Assets, out: &mut Stdout) -> Result<(), HeicError> {
        let value = convert_with(input, &Host(assets))?;
        emit(&value, out)
    }
}
fn emit(value: &Value, out: &mut impl Write) -> Result<(), HeicError> {
    serde_json::to_writer(&mut *out, value)
        .map_err(|_| error("output-closed", "stdout's reader has gone"))?;
    out.write_all(b"\n")
        .map_err(|_| error("output-closed", "stdout's reader has gone"))
}
fn convert_with(input: Input, assets: &impl AssetAccess) -> Result<Value, HeicError> {
    reference(&input.source)?;
    let handle = assets.open(&input.source)?;
    let info = assets.info(&handle);
    let stored_limit = match info.encoding {
        Encoding::Identity => MAX_INPUT_BYTES,
        Encoding::Base64 => MAX_INPUT_BYTES.div_ceil(3) * 4,
    };
    if info.stored_bytes.is_none_or(|n| n > stored_limit as u64) {
        return Err(error(
            "input-limit",
            "HEIC requires a known length within the 512 KiB decoded ceiling",
        ));
    }
    let bytes = assets.read_all(&handle)?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(error("input-limit", "decoded HEIC exceeds 512 KiB"));
    }
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return Err(error(
            "invalid-heic",
            "source has no HEIF file-type signature",
        ));
    }
    let info = heic_rs::probe(&bytes)
        .map_err(|_| error("invalid-heic", "HEIC metadata is invalid or unsupported"))?;
    dimensions(info.coded_width, info.coded_height)?;
    dimensions(info.width, info.height)?;
    let image = heic_rs::decode(
        &bytes,
        &heic_rs::DecodeOptions {
            layout: heic_rs::PixelLayout::Rgba8,
            max_pixels: Some(MAX_PIXELS),
            strict: true,
            threads: Some(1),
            ..Default::default()
        },
    )
    .map_err(|_| {
        error(
            "decode-failed",
            "HEIC decoding failed or uses an unsupported feature",
        )
    })?;
    dimensions(image.width, image.height)?;
    let mut output = BoundedOutput(Vec::new());
    {
        let mut encoder = png::Encoder::new(&mut output, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| error("encode-failed", "PNG encoding failed"))?;
        writer
            .write_image_data(&image.data)
            .map_err(|_| error("output-limit", "PNG encoding failed or exceeds 8 MiB"))?;
        writer
            .finish()
            .map_err(|_| error("output-limit", "PNG encoding failed or exceeds 8 MiB"))?;
    }
    let writer = assets.allocate("image/png", Encoding::Identity)?;
    assets.write_all(&writer, &output.0)?;
    let attached = assets.attach(writer)?;
    Ok(
        json!({"format":"png","width":image.width,"height":image.height,"bytes":output.0.len(),
        "assetNote":{"id":attached.id,"contentType":attached.content_type,"bytes":attached.stored_bytes}}),
    )
}
fn dimensions(width: u32, height: u32) -> Result<(), HeicError> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(error(
            "dimension-limit",
            "coded and displayed dimensions must be 1..4096 with at most 16777216 pixels",
        ));
    }
    Ok(())
}
struct BoundedOutput(Vec<u8>);
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_OUTPUT_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("PNG output limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
mod guest {
    dekopon_provider_sdk::export!(super::HeicProvider);
}
#[cfg(test)]
mod tests;
