use pasetors::claims::Claims;
use pasetors::keys::SymmetricKey;
use pasetors::local;
use pasetors::version4::V4;
use std::time::Duration;
use uuid::Uuid;

fn main() {
    let key_bytes = [0u8; 32];
    let sk: SymmetricKey<V4> = SymmetricKey::from(&key_bytes).expect("key");

    let user_id = "019d718c-4fc3-7d82-a51c-6c80c247c4fb";
    let tenant_id = "019d71bb-12ff-7fc1-b93d-69fd51cc58f3";
    let scopes = "credential:read credential:decrypt sandbox:write sandbox:read sandbox:execute";

    let mut claims = Claims::new_expires_in(&Duration::from_secs(3600)).expect("claims");
    claims.issuer("credbridge").expect("iss");
    claims
        .subject(&format!("{tenant_id}:{user_id}"))
        .expect("sub");
    claims.audience(tenant_id).expect("aud");
    claims
        .token_identifier(&Uuid::now_v7().to_string())
        .expect("jti");
    claims
        .add_additional("scope", serde_json::json!(scopes))
        .expect("scope");

    let token = local::encrypt(&sk, &claims, None, None).expect("encrypt");
    println!("{token}");
}
