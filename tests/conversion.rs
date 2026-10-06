//! Component conformance and admission; successful conversion uses native fake assets.
use dekopon_heic_provider::HeicProvider;
use dekopon_provider_sdk::{CommandRunOutcome, provider};
use dekopon_provider_sdk_testkit::{Harness, conformance};
use serde_json::json;
use std::{path::PathBuf, process::Command};

fn component() -> PathBuf {
    std::env::var_os("DEKOPON_PROVIDER_COMPONENT")
        .expect("DEKOPON_PROVIDER_COMPONENT must name freshly built component")
        .into()
}
#[test]
fn real_component_conforms_to_assets_and_stdio_contract() {
    conformance::<HeicProvider>(component()).expect("asset + stdio and typed manifest");
}
#[test]
fn component_has_only_assets_stdio_and_provider_export() {
    let output = Command::new("wasm-tools")
        .args(["component", "wit"])
        .arg(component())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wit = String::from_utf8(output.stdout).unwrap();
    let imports: Vec<_> = wit
        .lines()
        .filter(|line| line.trim_start().starts_with("import "))
        .collect();
    assert_eq!(imports.len(), 2, "{wit}");
    for name in ["dekopon:asset/asset@0.1.0", "dekopon:stdio/streams@0.1.0"] {
        assert!(imports.iter().any(|line| line.contains(name)), "{wit}");
    }
    for export in [
        "export describe: func() -> string",
        "export invoke: func(capability: string, input-json: string) -> result<_, u8>",
        "export run-command: func(argv: list<string>, stdin-piped: bool) -> string",
    ] {
        assert!(wit.contains(export), "missing {export}: {wit}");
    }
    for denied in [
        "wasi:",
        "dekopon:http/",
        "dekopon:clock/",
        "dekopon:settings/",
        "dekopon:storage/",
    ] {
        assert!(!wit.contains(denied), "unexpected import {denied}: {wit}");
    }
}
#[test]
fn missing_descriptor_is_refusal_not_successful_conversion() {
    let path = component();
    let CommandRunOutcome::Proposed {
        capability,
        input,
        secret_use,
    } = provider::command::<HeicProvider>(&["chat-asset:1".into()], false)
    else {
        panic!("proposal")
    };
    assert_eq!(capability.as_str(), "heic.convert");
    assert_eq!(input, json!({"source":"chat-asset:1"}));
    assert!(secret_use.is_none());
    let denied = Harness::<HeicProvider>::get(&path)
        .call("heic.convert", input)
        .expect_err("no descriptor supplied");
    assert!(
        denied.to_string().contains("invalid invocation assets"),
        "{denied}"
    );
    let bad = Harness::<HeicProvider>::get(&path)
        .call("heic.convert", json!({"source":"/etc/passwd"}))
        .expect("guest rejects invalid input");
    assert_ne!(bad.status, 0);
    assert!(bad.stdout.is_empty());
    let CommandRunOutcome::Failed { .. } =
        provider::command::<HeicProvider>(&["chat-asset:1".into()], true)
    else {
        panic!("stdin denied")
    };
}
