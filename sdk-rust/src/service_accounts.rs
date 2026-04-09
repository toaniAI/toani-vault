use crate::{
    client::CredBridgeClient,
    types::{
        ApiTokenMetadata, CreateServiceAccountRequest, CreateServiceAccountTokenRequest,
        CreateServiceAccountTokenResponse, RequestOptions, Result, ServiceAccountInfo,
        UpdateServiceAccountRequest,
    },
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ServiceAccountsService {
    client: Arc<CredBridgeClient>,
}

impl ServiceAccountsService {
    pub fn new(client: Arc<CredBridgeClient>) -> Self {
        Self { client }
    }

    pub async fn create(
        &self,
        request: CreateServiceAccountRequest,
        options: Option<RequestOptions>,
    ) -> Result<ServiceAccountInfo> {
        self.client
            .post_with_options("/service-accounts", request, options)
            .await
    }

    pub async fn list(&self, options: Option<RequestOptions>) -> Result<Vec<ServiceAccountInfo>> {
        self.client.get_with_options("/service-accounts", options).await
    }

    pub async fn get(
        &self,
        service_account_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<ServiceAccountInfo> {
        self.client
            .get_with_options(
                &format!("/service-accounts/{}", service_account_id.as_ref()),
                options,
            )
            .await
    }

    pub async fn update(
        &self,
        service_account_id: impl AsRef<str>,
        request: UpdateServiceAccountRequest,
        options: Option<RequestOptions>,
    ) -> Result<ServiceAccountInfo> {
        self.client
            .patch_with_options(
                &format!("/service-accounts/{}", service_account_id.as_ref()),
                request,
                options,
            )
            .await
    }

    pub async fn create_token(
        &self,
        service_account_id: impl AsRef<str>,
        request: CreateServiceAccountTokenRequest,
        options: Option<RequestOptions>,
    ) -> Result<CreateServiceAccountTokenResponse> {
        self.client
            .post_with_options(
                &format!("/service-accounts/{}/tokens", service_account_id.as_ref()),
                request,
                options,
            )
            .await
    }

    pub async fn list_tokens(
        &self,
        service_account_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<Vec<ApiTokenMetadata>> {
        self.client
            .get_with_options(
                &format!("/service-accounts/{}/tokens", service_account_id.as_ref()),
                options,
            )
            .await
    }
}
