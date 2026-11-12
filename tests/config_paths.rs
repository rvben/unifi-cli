#![cfg(unix)]

use std::process::Command;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

#[tokio::test]
async fn xdg_override_preserves_native_config_until_preferred_file_exists() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let xdg = root.path().join("xdg");
    let native = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        home.join(".config")
    };
    let legacy = native.join("unifi/config.toml");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    let server = MockServer::start().await;
    for key in ["legacy-key", "preferred-key"] {
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/health"))
            .and(header("x-api-key", key))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"meta":{"rc":"ok"},"data":[]})),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    let config = |key| format!("host = {:?}\napi_key = {key:?}\n", server.uri());
    std::fs::write(&legacy, config("legacy-key")).unwrap();

    for preferred_exists in [false, true] {
        if preferred_exists {
            let preferred = xdg.join("unifi/config.toml");
            std::fs::create_dir_all(preferred.parent().unwrap()).unwrap();
            std::fs::write(preferred, config("preferred-key")).unwrap();
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_unifi"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("UNIFI_") {
                command.env_remove(key);
            }
        }
        command
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &xdg)
            .args(["system", "health", "--output=json"]);
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "preferred_exists={preferred_exists}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(&legacy).unwrap(),
            config("legacy-key")
        );
    }
}
