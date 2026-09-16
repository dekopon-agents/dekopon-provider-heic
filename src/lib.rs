//! Import-free experimental HEIC conversion. Untrusted decoding requires a bounded Wasm host.
use base64::{Engine, engine::general_purpose::STANDARD};
use dekopon_provider_sdk::{
    CapabilityId, CommandInvocation, CommandRun, EffectKind, Provider, ProviderApiVersion,
    ProviderCapability, ProviderError, ProviderManifest, RiskLevel,
    clap::{Arg, Command},
    cli,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::Write;

const CONVERT: &str = "heic.convert";
const MAX_BYTES: usize = 524_288;
const MAX_SOURCE: usize = 699_076;
const MAX_DIMENSION: u32 = 512;
const MAX_PIXELS: u64 = 262_144;

#[allow(unsafe_code)]
mod bindings {
    wit_bindgen::generate!({path: "wit", world: "provider"});
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

/// Native entry point exists for tests only; hostile input belongs in bounded Wasm, not native.
pub struct HeicProvider;
impl Provider for HeicProvider {
    fn manifest() -> ProviderManifest {
        ProviderManifest {
            api_version: ProviderApiVersion::V1Alpha1,
            id: "heic".parse().expect("static ID"),
            description: "Experimental caller-fed HEIC to PNG; no filesystem or network access".into(),
            command_words: vec!["heic".into()],
            capabilities: vec![ProviderCapability {
                id: CONVERT.parse().expect("static ID"),
                description: "Decode at most 512 KiB HEIC, 512x512 / 262144 pixels, into at most 512 KiB PNG attachment. Current gateway cannot expand HEIC chat assets or return reusable asset IDs.".into(),
                effect: EffectKind::ReadOnly, risk: RiskLevel::Low,
                input_schema: json!({"type":"object", "required":["source"], "additionalProperties":false,
                    "properties":{"source":{"type":"string","minLength":1,"maxLength":MAX_SOURCE,
                    "description":"Exact data:image/heic;base64,... or data:image/heif;base64,... with standard padded base64 (512 KiB decoded maximum). No paths, URLs, raw base64 or unresolved chat-asset markers."}}}),
            }],
        }
    }
    fn invoke(capability: &CapabilityId, input: Value) -> Result<Value, ProviderError> {
        if capability.as_str() != CONVERT {
            return Err(error(
                "unsupported-capability",
                "only heic.convert is supported",
            ));
        }
        let input: Input = serde_json::from_value(input)
            .map_err(|_| error("invalid-input", "expected only a string source field"))?;
        let bytes = source_bytes(&input.source)?;
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
                .map_err(|_| error("output-limit", "PNG encoding failed or exceeds 512 KiB"))?;
            writer
                .finish()
                .map_err(|_| error("output-limit", "PNG encoding failed or exceeds 512 KiB"))?;
        }
        Ok(
            json!({"format":"png", "width":image.width,"height":image.height,"bytes":output.0.len(),
            "attachments":[{"mediaType":"image/png","base64":STANDARD.encode(output.0)}]}),
        )
    }
    fn run_command(argv: &[String], stdin: Option<&str>) -> Result<CommandRun, ProviderError> {
        if argv.len() > 3
            || argv.iter().any(|arg| arg.len() > MAX_SOURCE)
            || argv.iter().map(String::len).sum::<usize>() > MAX_SOURCE + 16
        {
            return Err(error(
                "input-limit",
                "command exceeds bounded argument limits",
            ));
        }
        let result = cli::run_command(Command::new("heic").version(env!("CARGO_PKG_VERSION"))
            .about("Experimental HEIC to PNG: 512 KiB input/output, 512x512, 262144 pixels; no file or URL access")
            .after_help("Generated PNGs have no reusable chat asset references and cannot currently chain to GPT Image edit.")
            .arg(Arg::new("source").required(true).help("HEIC data URL or chat-asset:N (gateway expansion currently blocks HEIC)")), argv, stdin, |matches, stdin| {
            if stdin.is_some() { return Err(error("invalid-input", "stdin is unsupported; supply one source argument")); }
            Ok(CommandInvocation { capability: CONVERT.parse().expect("static ID"),
                input: json!({"source":matches.get_one::<String>("source").expect("required argument")}), secret_use: None })
        })?;
        // Clap usage errors can echo arguments; source bytes must never enter rendered errors.
        Ok(match result {
            CommandRun::Rendered { status, .. } if status != 0 => CommandRun::Rendered {
                stdout: String::new(),
                stderr: "Usage: heic <SOURCE>; use heic --help for limits\n".into(),
                status,
            },
            other => other,
        })
    }
}

fn source_bytes(source: &str) -> Result<Vec<u8>, ProviderError> {
    if source.len() > MAX_SOURCE {
        return Err(error(
            "input-limit",
            "source exceeds 699076 bytes (512 KiB decoded maximum)",
        ));
    }
    let encoded = source.strip_prefix("data:image/heic;base64,")
        .or_else(|| source.strip_prefix("data:image/heif;base64,"))
        .ok_or_else(|| error("invalid-source", "source must be a HEIC/HEIF base64 data URL; paths, URLs and unresolved asset markers are unsupported"))?;
    let bytes = STANDARD.decode(encoded).map_err(|_| {
        error(
            "invalid-base64",
            "source requires canonical standard padded base64",
        )
    })?;
    if bytes.len() > MAX_BYTES {
        return Err(error("input-limit", "decoded HEIC exceeds 512 KiB"));
    }
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return Err(error(
            "invalid-heic",
            "source has no HEIF file-type signature",
        ));
    }
    Ok(bytes)
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
            "coded and displayed dimensions must be 1..512 with at most 262144 pixels",
        ));
    }
    Ok(())
}

struct BoundedOutput(Vec<u8>);
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.0.len()) {
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
mod tests {
    use super::*;
    #[test]
    fn wit_mirror() {
        assert_eq!(
            include_str!("../wit/deps/provider.wit"),
            dekopon_provider_sdk::PROVIDER_WIT
        );
    }
    #[test]
    fn limits() {
        assert!(dimensions(512, 512).is_ok());
        for (w, h) in [(0, 1), (513, 1), (1, 513), (u32::MAX, u32::MAX)] {
            assert!(dimensions(w, h).is_err());
        }
        assert!(source_bytes(&"a".repeat(MAX_SOURCE + 1)).is_err());
        let mut writer = BoundedOutput(vec![0; MAX_BYTES - 1]);
        assert!(writer.write_all(&[0, 1]).is_err());
        assert_eq!(writer.0.len(), MAX_BYTES - 1);
    }
}
