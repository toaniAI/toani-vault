use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CreateServiceAccountTokenRequest {
    scopes: Vec<String>,
    #[serde(default)]
    ttl_seconds: Option<u64>,
    #[serde(default)]
    display_name: Option<String>,
}

fn main() {
    let payload = r#"{"expires_in":3600}"#;
    let parsed = serde_json::from_str::<CreateServiceAccountTokenRequest>(payload);

    match parsed {
        Ok(value) => {
            println!("unexpected success: {:?}", value.scopes);
            println!("ttl_seconds={:?}", value.ttl_seconds);
            println!("display_name={:?}", value.display_name);
        }
        Err(error) => println!("{error}"),
    }
}
