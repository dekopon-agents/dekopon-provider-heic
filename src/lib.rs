//! Bounded HEIC conversion through broker-owned assets. Host isolation is mandatory for decoding.
mod assets;
use assets::{Assets, Host};
use dekopon_provider_sdk::{
    CapabilityId, CommandInvocation, CommandRun, EffectKind, Provider, ProviderApiVersion,
    ProviderCapability, ProviderError, ProviderManifest, RiskLevel,
    asset::Encoding,
    clap::{Arg, Command},
    cli,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::Write;

const CONVERT: &str = "heic.convert";
const MAX_INPUT_BYTES: usize = 524_288;
const MAX_SOURCE: usize = 31; // chat-asset: followed by at most twenty decimal digits.
const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: u64 = 16_777_216;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[allow(unsafe_code)]
mod bindings {
    wit_bindgen::generate!({path: "wit", world: "provider", generate_all});
}
#[allow(unsafe_code)]
mod export {
    use super::bindings;
    dekopon_provider_sdk::export_provider_with_cli!(super::HeicProvider, bindings);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    source: String,
}
fn error(code: &str, message: &str) -> ProviderError {
    ProviderError::new(code, message)
}
fn reference(source: &str) -> Result<(), ProviderError> {
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
impl Provider for HeicProvider {
    fn manifest() -> ProviderManifest {
        ProviderManifest {
            api_version: ProviderApiVersion::V1Alpha1,
            id: "heic".parse().expect("static ID"),
            description: "Experimental bounded HEIC asset to reusable PNG asset".into(),
            command_words: vec!["heic".into()],
            capabilities: vec![ProviderCapability {
                id: CONVERT.parse().expect("static ID"),
                description: "Decode at most 512 KiB HEIC, 4096x4096 / 16777216 pixels, into at most 8 MiB PNG. Host resource limits may refuse smaller images. Attaches without sending.".into(),
                effect: EffectKind::LocalWrite, risk: RiskLevel::Low,
                input_schema: json!({"type":"object", "required":["source"], "additionalProperties":false,
                    "properties":{"source":{"type":"string","minLength":12,"maxLength":MAX_SOURCE,
                    "pattern":"^chat-asset:","description":"chat-asset:<N>; at most 512 KiB decoded HEIC input."}}}),
            }],
        }
    }
    fn invoke(capability: &CapabilityId, input: Value) -> Result<Value, ProviderError> {
        invoke_with(capability, input, &Host)
    }
    fn run_command(argv: &[String], stdin: Option<&str>) -> Result<CommandRun, ProviderError> {
        if argv.len() > 3 || argv.iter().any(|arg| arg.len() > MAX_SOURCE) {
            return Err(error(
                "input-limit",
                "command exceeds bounded argument limits",
            ));
        }
        let result = cli::run_command(Command::new("heic").version(env!("CARGO_PKG_VERSION"))
            .about("HEIC asset to PNG: 512 KiB input, 8 MiB output, 4096x4096, 16777216 pixels")
            .after_help("Host resource limits may refuse smaller images. Attaches a reusable asset; use asset send to deliver it.")
            .arg(Arg::new("source").required(true).help("chat-asset:<N>")), argv, stdin, |matches, stdin| {
                if stdin.is_some() { return Err(error("invalid-input", "stdin is unsupported; supply one source argument")); }
                let source = matches.get_one::<String>("source").expect("required argument");
                reference(source)?;
                Ok(CommandInvocation { capability: CONVERT.parse().expect("static ID"), input: json!({"source":source}), secret_use: None })
            })?;
        // Never reflect an invalid input payload through clap's usage rendering.
        Ok(match result {
            CommandRun::Rendered { status, .. } if status != 0 => CommandRun::Rendered {
                stdout: String::new(),
                stderr: "Usage: heic <chat-asset:N>; use heic --help for limits\n".into(),
                status,
            },
            other => other,
        })
    }
}

fn invoke_with(
    capability: &CapabilityId,
    input: Value,
    assets: &impl Assets,
) -> Result<Value, ProviderError> {
    if capability.as_str() != CONVERT {
        return Err(error(
            "unsupported-capability",
            "only heic.convert is supported",
        ));
    }
    let input: Input = serde_json::from_value(input)
        .map_err(|_| error("invalid-input", "expected only a string source field"))?;
    reference(&input.source)?;
    let handle = assets.open(&input.source)?;
    let info = assets.info(&handle);
    let stored_limit = match info.encoding {
        Encoding::Identity => MAX_INPUT_BYTES,
        Encoding::Base64 => MAX_INPUT_BYTES.div_ceil(3) * 4,
    };
    // Bound read_all's reservation before reading. Gateway-provided file lengths are known.
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

fn dimensions(width: u32, height: u32) -> Result<(), ProviderError> {
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

#[cfg(test)]
mod tests;
