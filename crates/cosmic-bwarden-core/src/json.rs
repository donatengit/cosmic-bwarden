use crate::error::{Error, Result};

pub trait DeserializeJsonWithPath {
    fn json_with_path<T: serde::de::DeserializeOwned>(self) -> Result<T>;
}

impl DeserializeJsonWithPath for String {
    fn json_with_path<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        #[cfg(feature = "debug-api")]
        {
            let jd = &mut serde_json::Deserializer::from_str(&self);
            serde_path_to_error::deserialize(jd)
                .map_err(|source| Error::Json { source })
        }
        #[cfg(not(feature = "debug-api"))]
        {
            serde_json::from_str(&self).map_err(|source| Error::Json { source })
        }
    }
}

pub trait DeserializeJsonWithPathAsync {
    #[allow(async_fn_in_trait)]
    async fn json_with_path<T: serde::de::DeserializeOwned>(
        self,
    ) -> Result<T>;
}

impl DeserializeJsonWithPathAsync for reqwest::Response {
    async fn json_with_path<T: serde::de::DeserializeOwned>(
        self,
    ) -> Result<T> {
        let bytes = self
            .bytes()
            .await
            .map_err(|source| Error::Reqwest { source })?;
        #[cfg(feature = "debug-api")]
        {
            let jd = &mut serde_json::Deserializer::from_slice(&bytes);
            serde_path_to_error::deserialize(jd)
                .map_err(|source| Error::Json { source })
        }
        #[cfg(not(feature = "debug-api"))]
        {
            serde_json::from_slice(&bytes).map_err(|source| Error::Json { source })
        }
    }
}
