#[cfg(feature = "keyring")]
use oo7::dbus::Service;

#[cfg(feature = "keyring")]
const APP_ID: &str = "com.enikeev.cosmic_bwarden";

pub async fn store_tokens(
    server: &str,
    email: &str,
    access_token: &str,
    refresh_token: &str,
) -> anyhow::Result<()> {
    #[cfg(feature = "keyring")]
    {
        let service = Service::new().await?;
        // Never create a second collection that get_tokens cannot see.
        let collection = service.default_collection().await?;

        let mut attributes = std::collections::HashMap::new();
        attributes.insert("app_id", APP_ID);
        attributes.insert("server", server);
        attributes.insert("email", email);

        let secret = format!("{}:{}", access_token, refresh_token);

        collection
            .create_item(
                &format!("Bitwarden Session for {} ({})", email, server),
                &attributes,
                secret.as_bytes(),
                true,
                "text/plain",
            )
            .await?;
        Ok(())
    }
    #[cfg(not(feature = "keyring"))]
    {
        let _ = (server, email, access_token, refresh_token);
        Ok(())
    }
}

pub async fn get_tokens(server: &str, email: &str) -> anyhow::Result<Option<(String, String)>> {
    #[cfg(feature = "keyring")]
    {
        let service = Service::new().await?;
        let collection = service.default_collection().await?;

        let mut attributes = std::collections::HashMap::new();
        attributes.insert("app_id", APP_ID);
        attributes.insert("server", server);
        attributes.insert("email", email);

        let items = collection.search_items(&attributes).await?;
        if let Some(item) = items.first() {
            let secret_bytes = item.secret().await?;
            let secret_str = String::from_utf8(secret_bytes.to_vec())?;
            if let Some((at, rt)) = secret_str.split_once(':') {
                return Ok(Some((at.to_string(), rt.to_string())));
            }
        }
        Ok(None)
    }
    #[cfg(not(feature = "keyring"))]
    {
        let _ = (server, email);
        Ok(None)
    }
}

pub async fn delete_tokens(server: &str, email: &str) -> anyhow::Result<()> {
    #[cfg(feature = "keyring")]
    {
        let service = Service::new().await?;
        let collection = service.default_collection().await?;

        let mut attributes = std::collections::HashMap::new();
        attributes.insert("app_id", APP_ID);
        attributes.insert("server", server);
        attributes.insert("email", email);

        let items = collection.search_items(&attributes).await?;
        for item in items {
            item.delete().await?;
        }
        Ok(())
    }
    #[cfg(not(feature = "keyring"))]
    {
        let _ = (server, email);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn store_and_get_share_default_collection() {
        let src = include_str!("keyring.rs");
        assert!(
            src.contains("default_collection"),
            "store/get/delete must use the default collection"
        );
        let create = ["create", "_", "collection"].concat();
        assert!(
            !src.contains(&create),
            "must not create a second collection get_tokens cannot see"
        );
    }
}
