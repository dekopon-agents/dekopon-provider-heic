use dekopon_provider_sdk_testkit::{CommandRunOutcome, FakeBroker};
use serde_json::json;

#[tokio::test]
async fn component_contract_and_pure_handle_proposal() -> Result<(), Box<dyn std::error::Error>> {
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
    assert!(wit.contains("import dekopon:asset/asset@0.1.0"), "{wit}");
    for export in ["describe", "invoke", "run-command"] {
        assert!(wit.contains(&format!("export {export}:")));
    }
    let broker = FakeBroker::builder()
        .component(&component)
        .provider("heic")
        .build()
        .await?;
    let CommandRunOutcome::Proposed {
        capability,
        input,
        secret_use,
    } = broker
        .run_command("heic", &["chat-asset:1".into()], None)
        .await?
    else {
        panic!("proposal")
    };
    assert_eq!(capability.as_str(), "heic.convert");
    assert_eq!(input, json!({"source":"chat-asset:1"}));
    assert!(secret_use.is_none());
    // Testkit has no input descriptors/asset grant builder. Missing input must fail, never decode.
    let missing = broker.invoke("heic.convert", input).await.unwrap_err();
    assert!(
        matches!(
            missing.provider_failure().map(|(code, _)| code),
            Some("unknown-reference" | "unconfigured")
        ),
        "{missing:?}"
    );
    assert!(
        broker
            .invoke(
                "heic.convert",
                json!({"source":"data:image/heic;base64,AAAA"})
            )
            .await
            .is_err()
    );
    let CommandRunOutcome::Rendered { status, stdout, .. } =
        broker.run_command("heic", &["--help".into()], None).await?
    else {
        panic!("help")
    };
    assert_eq!(status, 0);
    assert!(stdout.contains("chat-asset"));
    Ok(())
}
