use super::*;
use dekopon_provider_sdk::asset::Info;
use std::cell::RefCell;

struct Fake {
    bytes: Vec<u8>,
    info: Info,
    output: RefCell<Vec<u8>>,
    calls: RefCell<Vec<&'static str>>,
    fail: Option<&'static str>,
}
impl Fake {
    fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            info: Info {
                id: Some(1),
                content_type: "image/heic".into(),
                encoding: Encoding::Identity,
                stored_bytes: Some(bytes.len() as u64),
                seekable: true,
                origin: "chat".into(),
                sent: false,
            },
            output: RefCell::new(vec![]),
            calls: RefCell::new(vec![]),
            fail: None,
        }
    }
    fn call(&self, name: &'static str) -> Result<(), ProviderError> {
        self.calls.borrow_mut().push(name);
        if self.fail == Some(name) {
            Err(error("denied", "fake failure"))
        } else {
            Ok(())
        }
    }
}
impl Assets for Fake {
    type Input = ();
    type Output = ();
    fn open(&self, r: &str) -> Result<(), ProviderError> {
        assert_eq!(r, "chat-asset:1");
        self.call("open")
    }
    fn info(&self, _: &()) -> Info {
        self.info.clone()
    }
    fn read_all(&self, _: &()) -> Result<Vec<u8>, ProviderError> {
        self.call("read_all")?;
        Ok(self.bytes.clone())
    }
    fn allocate(&self, ty: &str, encoding: Encoding) -> Result<(), ProviderError> {
        assert_eq!(ty, "image/png");
        assert!(matches!(encoding, Encoding::Identity));
        self.call("allocate")
    }
    fn write_all(&self, _: &(), bytes: &[u8]) -> Result<(), ProviderError> {
        self.call("write_all")?;
        self.output.replace(bytes.to_vec());
        Ok(())
    }
    fn attach(&self, _: ()) -> Result<Info, ProviderError> {
        self.call("attach")?;
        Ok(Info {
            id: None,
            content_type: "image/png".into(),
            encoding: Encoding::Identity,
            stored_bytes: Some(self.output.borrow().len() as u64),
            origin: "provider:heic.convert".into(),
            ..self.info.clone()
        })
    }
}
fn invoke(f: &Fake) -> Result<Value, ProviderError> {
    invoke_with(
        &CONVERT.parse().unwrap(),
        json!({"source":"chat-asset:1"}),
        f,
    )
}
fn assert_reference(bytes: &[u8], reference: &[u8], dimension: u32, tolerance: u8) {
    let decode = |bytes: &[u8]| {
        let mut r = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; r.output_buffer_size().unwrap()];
        let info = r.next_frame(&mut pixels).unwrap();
        pixels.truncate(info.buffer_size());
        (info, pixels)
    };
    let (info, pixels) = decode(bytes);
    let (refinfo, reference) = decode(reference);
    assert_eq!((info.width, info.height), (dimension, dimension));
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(refinfo.color_type, png::ColorType::Rgb);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!(refinfo.bit_depth, png::BitDepth::Eight);
    assert_eq!(pixels.len() / 4, reference.len() / 3);
    for (actual, expected) in pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reference.as_chunks::<3>().0)
    {
        assert_eq!(actual[3], 255);
        for c in 0..3 {
            assert!(actual[c].abs_diff(expected[c]) <= tolerance);
        }
    }
}
#[test]
fn real_fixtures_flow_through_handles_and_attach_metadata_only() {
    for (bytes, reference, dimension, tolerance) in [
        (
            include_bytes!("../tests/fixtures/flat-64.heic").as_slice(),
            include_bytes!("../tests/fixtures/flat-64.ref.png").as_slice(),
            64,
            8,
        ),
        (
            include_bytes!("../tests/fixtures/gradient-512.heic").as_slice(),
            include_bytes!("../tests/fixtures/gradient-512.ref.png").as_slice(),
            512,
            16,
        ),
    ] {
        let f = Fake::new(bytes);
        let out = invoke(&f).unwrap();
        assert_reference(&f.output.borrow(), reference, dimension, tolerance);
        assert_eq!(
            *f.calls.borrow(),
            ["open", "read_all", "allocate", "write_all", "attach"]
        );
        assert_eq!(out["width"], dimension);
        assert_eq!(out["height"], dimension);
        assert_eq!(out["assetNote"]["id"], Value::Null);
        assert_eq!(out["assetNote"]["contentType"], "image/png");
        assert_eq!(out["assetNote"]["bytes"], f.output.borrow().len());
        assert!(out.get("attachments").is_none());
        assert!(serde_json::to_vec(&out).unwrap().len() < 1024);
    }
}
#[test]
fn base64_storage_is_decoded_by_the_handle_not_by_the_provider() {
    let bytes = include_bytes!("../tests/fixtures/flat-64.heic");
    let mut f = Fake::new(bytes);
    f.info.encoding = Encoding::Base64;
    f.info.stored_bytes = Some((bytes.len().div_ceil(3) * 4) as u64);
    let out = invoke(&f).unwrap();
    assert_eq!(out["width"], 64);
    assert_eq!(out["assetNote"]["bytes"], f.output.borrow().len());
    assert!(out.get("attachments").is_none());
}
#[test]
fn closed_input_and_reference_validation_precede_imports() {
    let f = Fake::new(&[]);
    for input in [
        json!({}),
        json!({"source":42}),
        json!({"source":"chat-asset:1","extra":true}),
        json!({"source":"data:image/heic;base64,PRIVATE"}),
        json!({"source":"https://private/a"}),
        json!({"source":"/private/a"}),
        json!({"source":"chat-asset:"}),
        json!({"source":"chat-asset:-1"}),
        json!({"source":"chat-asset:18446744073709551616"}),
    ] {
        let e = invoke_with(&CONVERT.parse().unwrap(), input, &f).unwrap_err();
        assert!(!format!("{e:?}").contains("PRIVATE"));
    }
    assert!(
        invoke_with(
            &"heic.other".parse().unwrap(),
            json!({"source":"chat-asset:1"}),
            &f
        )
        .is_err()
    );
    assert!(f.calls.borrow().is_empty());
}
#[test]
fn input_ceiling_is_checked_before_read_and_against_decoded_bytes() {
    for encoding in [Encoding::Identity, Encoding::Base64] {
        let mut f = Fake::new(&[]);
        f.info.encoding = encoding;
        let limit = if matches!(encoding, Encoding::Identity) {
            MAX_INPUT_BYTES
        } else {
            MAX_INPUT_BYTES.div_ceil(3) * 4
        };
        for len in [None, Some(limit as u64 + 1)] {
            f.info.stored_bytes = len;
            assert!(format!("{:?}", invoke(&f).unwrap_err()).contains("input-limit"));
        }
        assert!(!f.calls.borrow().contains(&"read_all"));
    }
    let mut f = Fake::new(&vec![0; MAX_INPUT_BYTES + 1]);
    f.info.stored_bytes = Some(MAX_INPUT_BYTES as u64);
    assert!(format!("{:?}", invoke(&f).unwrap_err()).contains("input-limit"));
    assert!(!f.calls.borrow().contains(&"allocate"));
    let f = Fake::new(&vec![0; MAX_INPUT_BYTES]);
    assert!(format!("{:?}", invoke(&f).unwrap_err()).contains("invalid-heic"));
    assert!(f.calls.borrow().contains(&"read_all"));
}
#[test]
fn failures_do_not_retry_or_attach_partial_outputs() {
    let calls = ["open", "read_all", "allocate", "write_all", "attach"];
    for (i, fail) in calls.iter().enumerate() {
        let mut f = Fake::new(include_bytes!("../tests/fixtures/flat-64.heic"));
        f.fail = Some(fail);
        assert!(invoke(&f).is_err());
        assert_eq!(*f.calls.borrow(), calls[..=i]);
    }
}
#[test]
fn invalid_truncated_and_oversized_dimensions_refuse_before_allocation() {
    let bytes = include_bytes!("../tests/fixtures/flat-64.heic");
    for end in [0, 12, 32, 100, bytes.len() - 16] {
        let f = Fake::new(&bytes[..end]);
        assert!(invoke(&f).is_err());
        assert!(!f.calls.borrow().contains(&"allocate"));
    }
    for (width, height) in [(4097_u32, 64_u32), (64, 4097), (u32::MAX, u32::MAX)] {
        let mut bytes = bytes.to_vec();
        let ispe = bytes.windows(4).position(|b| b == b"ispe").unwrap();
        bytes[ispe + 8..ispe + 12].copy_from_slice(&width.to_be_bytes());
        bytes[ispe + 12..ispe + 16].copy_from_slice(&height.to_be_bytes());
        let f = Fake::new(&bytes);
        assert!(format!("{:?}", invoke(&f).unwrap_err()).contains("dimension-limit"));
    }
    for (w, h) in [(1352, 2185), (4096, 4096)] {
        assert!(dimensions(w, h).is_ok());
    }
    for (w, h) in [(0, 1), (1, 0), (4097, 1), (u32::MAX, u32::MAX)] {
        assert!(dimensions(w, h).is_err());
    }
}
#[test]
fn bounded_output_and_manifest_contract() {
    let mut w = BoundedOutput(vec![0; MAX_OUTPUT_BYTES - 1]);
    assert!(w.write_all(&[0, 1]).is_err());
    w.write_all(&[1]).unwrap();
    assert_eq!(w.0.len(), MAX_OUTPUT_BYTES);
    assert!(w.write_all(&[2]).is_err());
    assert_eq!(
        include_str!("../wit/deps/provider.wit"),
        dekopon_provider_sdk::PROVIDER_WIT
    );
    assert_eq!(
        include_str!("../wit/deps/asset.wit"),
        dekopon_provider_sdk::ASSET_WIT
    );
    assert_eq!(
        HeicProvider::manifest().capabilities[0].input_schema["properties"]["source"]["pattern"],
        "^chat-asset:"
    );
    assert!(matches!(
        HeicProvider::manifest().capabilities[0].effect,
        EffectKind::LocalWrite
    ));
}
