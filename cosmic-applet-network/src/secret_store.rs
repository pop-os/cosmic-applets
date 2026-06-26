// Best-effort keyring cache of network secrets. nmrs doesn't surface the
// secrets NM sends with GetSecrets, so the applet caches them itself to
// pre-fill the password dialog, keyed as the pre-nmrs agent did
// (application/uuid/setting_name/name) so older secrets still read.

use std::collections::HashMap;

use secret_service::{EncryptionType, SecretService};
use secure_string::SecureString;

const SECRET_ID: &str = "com.system76.CosmicSettings.NetworkManager";

/// Look up stored secrets for a connection's setting, keyed by secret name
/// (e.g. `"password"`). Empty on any error.
pub async fn lookup(uuid: &str, setting_name: &str) -> HashMap<String, SecureString> {
    match lookup_inner(uuid, setting_name).await {
        Ok(secrets) => secrets,
        Err(e) => {
            tracing::debug!("keyring lookup failed for {uuid}/{setting_name}: {e}");
            HashMap::new()
        }
    }
}

async fn lookup_inner(
    uuid: &str,
    setting_name: &str,
) -> Result<HashMap<String, SecureString>, secret_service::Error> {
    let ss = SecretService::connect(EncryptionType::Dh).await?;
    let collection = ss.get_default_collection().await?;
    let attributes = HashMap::from([
        ("application", SECRET_ID),
        ("uuid", uuid),
        ("setting_name", setting_name),
    ]);
    let items = collection.search_items(attributes).await?;

    let mut secrets = HashMap::new();
    for item in &items {
        let name = item
            .get_attributes()
            .await?
            .get("name")
            .cloned()
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        if let Ok(value) = String::from_utf8(item.get_secret().await?) {
            secrets.insert(name, SecureString::from(value));
        }
    }
    Ok(secrets)
}

/// Persist secrets for a connection's setting, overwriting matching entries.
/// Best-effort: logs on error.
pub async fn store(uuid: &str, setting_name: &str, secrets: &HashMap<String, String>) {
    if let Err(e) = store_inner(uuid, setting_name, secrets).await {
        tracing::warn!("failed to store secret for {uuid}/{setting_name}: {e}");
    }
}

async fn store_inner(
    uuid: &str,
    setting_name: &str,
    secrets: &HashMap<String, String>,
) -> Result<(), secret_service::Error> {
    let ss = SecretService::connect(EncryptionType::Dh).await?;
    let collection = ss.get_default_collection().await?;
    for (name, secret) in secrets {
        let attributes = HashMap::from([
            ("application", SECRET_ID),
            ("uuid", uuid),
            ("setting_name", setting_name),
            ("name", name.as_str()),
        ]);
        collection
            .create_item(
                "NetworkManager Secret",
                attributes,
                secret.as_bytes(),
                true,
                "text/plain",
            )
            .await?;
    }
    Ok(())
}
