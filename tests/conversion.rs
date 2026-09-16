use base64::{Engine, engine::general_purpose::STANDARD};
use dekopon_heic_provider::HeicProvider;
use dekopon_provider_sdk::Provider;
use serde_json::{Value, json};

fn input() -> Value {
    json!({"source":format!("data:image/heic;base64,{}",STANDARD.encode(include_bytes!("fixtures/flat-64.heic")))})
}

fn assert_reference(output: &Value, reference: &[u8], dimension: u32, tolerance: u8) {
    assert_eq!(output["format"], "png");
    assert_eq!(output["width"], dimension);
    assert_eq!(output["height"], dimension);
    assert_eq!(output["attachments"][0]["mediaType"], "image/png");
    let bytes = STANDARD
        .decode(output["attachments"][0]["base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(output["bytes"], bytes.len());
    let decode = |bytes: &[u8]| {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        pixels.truncate(info.buffer_size());
        (info, pixels)
    };
    let (info, pixels) = decode(&bytes);
    let (reference_info, reference) = decode(reference);
    assert_eq!((info.width, info.height), (dimension, dimension));
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!(reference_info.color_type, png::ColorType::Rgb);
    assert_eq!(reference_info.bit_depth, png::BitDepth::Eight);
    let mut max_error = 0;
    for (actual, expected) in pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reference.as_chunks::<3>().0)
    {
        assert_eq!(actual[3], 255);
        for channel in 0..3 {
            max_error = max_error.max(actual[channel].abs_diff(expected[channel]));
        }
    }
    assert!(
        max_error <= tolerance,
        "maximum per-channel difference from independent Apple decode: {max_error}"
    );
    println!(
        "{dimension}x{dimension} HEIC -> {} PNG bytes; max reference channel error {max_error}",
        bytes.len()
    );
    assert!(serde_json::to_vec(output).unwrap().len() < 1_048_576);
}

fn assert_pixels(output: &Value) {
    assert_reference(output, include_bytes!("fixtures/flat-64.ref.png"), 64, 8);
}

#[test]
fn heic_convert_decodes_real_pixels_against_independent_reference() {
    let result = HeicProvider::invoke(&"heic.convert".parse().unwrap(), input()).unwrap();
    assert_pixels(&result);
    let gradient = json!({"source":format!("data:image/heic;base64,{}",STANDARD.encode(include_bytes!("fixtures/gradient-512.heic")))});
    let result = HeicProvider::invoke(&"heic.convert".parse().unwrap(), gradient).unwrap();
    assert_reference(
        &result,
        include_bytes!("fixtures/gradient-512.ref.png"),
        512,
        16,
    );
}

#[test]
fn heic_convert_refuses_invalid_closed_inputs_without_echoing_payload() {
    for input in [
        json!({}),
        json!({"source":42}),
        json!({"source":"chat-asset:1"}),
        json!({"source":"https://example.com/private.heic"}),
        json!({"source":"/private/photo.heic"}),
        json!({"source":"data:image/png;base64,AAAA"}),
        json!({"source":"data:image/heic;base64,!!!"}),
        json!({"source":"data:image/heic;base64,AAAA"}),
        json!({"source":"secret", "extra":true}),
    ] {
        let err = HeicProvider::invoke(&"heic.convert".parse().unwrap(), input).unwrap_err();
        assert!(!format!("{err:?}").contains("private"));
        assert!(!format!("{err:?}").contains("secret"));
    }
    assert!(HeicProvider::invoke(&"heic.other".parse().unwrap(), input()).is_err());
    let bytes = include_bytes!("fixtures/flat-64.heic");
    for end in [12, 32, 100, bytes.len() - 16] {
        assert!(HeicProvider::invoke(&"heic.convert".parse().unwrap(),json!({"source":format!("data:image/heic;base64,{}",STANDARD.encode(&bytes[..end]))})).is_err());
    }
}

#[test]
fn heic_convert_refuses_oversized_container_dimensions_before_decode() {
    let mut bytes = include_bytes!("fixtures/flat-64.heic").to_vec();
    let ispe = bytes.windows(4).position(|bytes| bytes == b"ispe").unwrap();
    bytes[ispe + 8..ispe + 12].copy_from_slice(&513_u32.to_be_bytes());
    let result = HeicProvider::invoke(
        &"heic.convert".parse().unwrap(),
        json!({"source":format!("data:image/heic;base64,{}",STANDARD.encode(bytes))}),
    );
    assert!(format!("{:?}", result.unwrap_err()).contains("dimension-limit"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn heic_convert_runs_in_bounded_broker_component() -> Result<(), Box<dyn std::error::Error>> {
    use dekopon_provider_sdk_testkit::{BrokerHostLimits, CommandRunOutcome, FakeBroker};
    let component = std::path::PathBuf::from(
        std::env::var_os("DEKOPON_PROVIDER_COMPONENT")
            .expect("DEKOPON_PROVIDER_COMPONENT must name freshly built component"),
    );
    let wit = std::process::Command::new("wasm-tools")
        .args(["component", "wit"])
        .arg(&component)
        .output()?;
    assert!(wit.status.success());
    let wit = String::from_utf8(wit.stdout)?;
    assert_eq!(
        wit.lines()
            .filter(|line| line.trim_start().starts_with("import "))
            .count(),
        0,
        "{wit}"
    );
    assert_eq!(
        wit.lines()
            .filter(|line| line.trim_start().starts_with("export "))
            .count(),
        3,
        "{wit}"
    );
    for export in ["describe", "invoke", "run-command"] {
        assert!(wit.contains(&format!("export {export}:")));
    }
    let limits = BrokerHostLimits {
        fuel: 350_000_000,
        ..Default::default()
    };
    assert_eq!(limits.max_memory_bytes, 64 * 1024 * 1024);
    let broker = FakeBroker::builder()
        .component(&component)
        .provider("heic")
        .host_limits(limits.clone())
        .build()
        .await?;
    assert_pixels(&broker.invoke("heic.convert", input()).await?);
    let gradient = json!({"source":format!("data:image/heic;base64,{}",STANDARD.encode(include_bytes!("fixtures/gradient-512.heic")))});
    let start = std::time::Instant::now();
    let err = broker.invoke("heic.convert", gradient).await.unwrap_err();
    assert!(format!("{err:?}").contains("all fuel consumed"), "{err:?}");
    println!(
        "350M fuel: 512x512 invoke exhausts fuel after {:?}",
        start.elapsed()
    );
    let CommandRunOutcome::Proposed {
        capability,
        input,
        secret_use,
    } = broker
        .run_command("heic", &["chat-asset:1".into()], None)
        .await?
    else {
        panic!("expected pure proposal")
    };
    assert_eq!(capability.as_str(), "heic.convert");
    assert_eq!(input, json!({"source":"chat-asset:1"}));
    assert!(secret_use.is_none());
    assert!(broker.invoke("heic.convert", input).await.is_err());
    let CommandRunOutcome::Rendered { status, stdout, .. } =
        broker.run_command("heic", &["--help".into()], None).await?
    else {
        panic!("expected help")
    };
    assert_eq!(status, 0);
    assert!(stdout.contains("512"));
    // A store can describe the provider yet lacks fuel for even this small valid decode.
    let constrained = FakeBroker::builder()
        .component(&component)
        .provider("heic")
        .host_limits(BrokerHostLimits {
            fuel: 1_000_000,
            ..limits
        })
        .build()
        .await?;
    let err = constrained
        .invoke("heic.convert", crate::input())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        dekopon_provider_sdk_testkit::FakeBrokerError::Invocation(_)
    ));
    assert!(format!("{err:?}").contains("all fuel consumed"), "{err:?}");
    println!("1,000,000 fuel: describe succeeds, invoke traps on fuel exhaustion");
    Ok(())
}

#[test]
fn heic_command_never_renders_source_bytes_on_usage_errors() {
    use dekopon_provider_sdk::CommandRun;
    for argv in [
        vec!["chat-asset:1".into(), "PRIVATE_BASE64".into()],
        vec!["--PRIVATE_BASE64".into()],
    ] {
        let CommandRun::Rendered {
            status,
            stdout,
            stderr,
        } = HeicProvider::run_command(&argv, None).unwrap()
        else {
            panic!("expected usage error")
        };
        assert_eq!(status, 2);
        assert!(stdout.is_empty());
        assert!(!stderr.contains("PRIVATE_BASE64"));
    }
}
