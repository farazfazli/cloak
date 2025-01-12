use crate::{authentication, errors::CustomError};
use db::queries;
use db::Pool;
use grpc_api::vault::*;
use tonic::{Code, Request, Response, Status};

pub struct VaultService {
    pub pool: Pool,
}

#[tonic::async_trait]
impl grpc_api::vault::vault_server::Vault for VaultService {
    async fn get_service_account(
        &self,
        request: Request<GetServiceAccountRequest>,
    ) -> Result<Response<GetServiceAccountResponse>, Status> {
        let req = request.into_inner();

        // Create a transaction and setup RLS
        let mut client = self
            .pool
            .get()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;
        let transaction = client
            .transaction()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        super::rls::set_row_level_security_ecdh_public_key(&transaction, &req.ecdh_public_key)
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let service_account = queries::service_accounts::get_by_ecdh_public_key()
            .bind(&transaction, &req.ecdh_public_key.as_ref())
            .one()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let secrets = queries::service_account_secrets::get_all_dangerous()
            .bind(&transaction, &service_account.id)
            .all()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let secrets = secrets
            .into_iter()
            .map(|secret| ServiceAccountSecret {
                encrypted_name: secret.name,
                name_blind_index: secret.name_blind_index,
                encrypted_secret_value: secret.secret,
                ecdh_public_key: secret.ecdh_public_key,
            })
            .collect();

        let response = GetServiceAccountResponse {
            service_account_id: service_account.id as u32,
            secrets,
        };

        return Ok(Response::new(response));
    }

    async fn get_vault(
        &self,
        request: Request<GetVaultRequest>,
    ) -> Result<Response<GetVaultResponse>, Status> {
        let authenticated_user = authenticate(&request).await?;

        let req = request.into_inner();

        // Create a transaction and setup RLS
        let mut client = self
            .pool
            .get()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;
        let transaction = client
            .transaction()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;
        super::rls::set_row_level_security_user(&transaction, &authenticated_user)
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let secrets = queries::secrets::get_all()
            .bind(&transaction, &(req.vault_id as i32))
            .all()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let vault = queries::vaults::get()
            .bind(
                &transaction,
                &(req.vault_id as i32),
                &(authenticated_user.user_id as i32),
            )
            .one()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let user_vault = queries::user_vaults::get()
            .bind(
                &transaction,
                &(authenticated_user.user_id as i32),
                &(req.vault_id as i32),
            )
            .one()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let service_accounts = queries::service_accounts::get_by_vault()
            .bind(&transaction, &(req.vault_id as i32))
            .all()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let secrets = secrets
            .into_iter()
            .map(|s| Secret {
                encrypted_name: s.name,
                name_blind_index: s.name_blind_index,
                encrypted_secret_value: s.secret,
                environment_id: s.environment_id as u32,
            })
            .collect();

        let service_accounts = service_accounts
            .into_iter()
            .filter_map(|s| {
                if let Some(env_id) = s.environment_id {
                    Some(ServiceAccount {
                        service_account_id: s.id as u32,
                        environment_id: env_id as u32,
                        public_ecdh_key: s.ecdh_public_key,
                    })
                } else {
                    None
                }
            })
            .collect();

        let response = GetVaultResponse {
            name: vault.name,
            user_vault_encrypted_vault_key: user_vault.encrypted_vault_key,
            user_vault_public_ecdh_key: user_vault.ecdh_public_key,
            secrets,
            service_accounts,
        };

        Ok(Response::new(response))
    }

    async fn create_secrets(
        &self,
        request: Request<CreateSecretsRequest>,
    ) -> Result<Response<CreateSecretsResponse>, Status> {
        let authenticated_user = authenticate(&request).await?;

        // Create a transaction and setup RLS
        let mut client = self
            .pool
            .get()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;
        let transaction = client
            .transaction()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;
        super::rls::set_row_level_security_user(&transaction, &authenticated_user)
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let service_account = request.into_inner();

        for account_secret in service_account.account_secrets {
            // Get the service account this request is trying to access
            let sa = queries::service_accounts::get_dangerous()
                .bind(&transaction, &(account_secret.service_account_id as i32))
                .one()
                .await
                .map_err(|e| CustomError::Database(e.to_string()))?;

            // If the vault is already connected we can do an IDOR check
            // And see if the user actually has access to the vault.
            if let Some(vault_id) = sa.vault_id {
                // Blow up, if the user doesn't have access to the vault.
                queries::service_account_secrets::get_users_vaults()
                    .bind(
                        &transaction,
                        &(authenticated_user.user_id as i32),
                        &vault_id,
                    )
                    .all()
                    .await
                    .map_err(|e| CustomError::Database(e.to_string()))?;
            }

            // If yes, save the secret
            for secret in account_secret.secrets {
                queries::service_account_secrets::insert()
                    .bind(
                        &transaction,
                        &(account_secret.service_account_id as i32),
                        &secret.encrypted_name.as_ref(),
                        &secret.name_blind_index.as_ref(),
                        &secret.encrypted_secret_value.as_ref(),
                        &account_secret.public_ecdh_key.as_ref(),
                    )
                    .await
                    .map_err(|e| CustomError::Database(e.to_string()))?;
            }
        }

        transaction
            .commit()
            .await
            .map_err(|e| CustomError::Database(e.to_string()))?;

        let response = CreateSecretsResponse {};

        Ok(Response::new(response))
    }
}

const X_USER_ID: &str = "x-user-id";

// We have 2 types of authentication
// 1. If we have a header set to "authentication-type" then envoy with have set a x-user-id
// 2. If it is not set then we must have an API-KEY which we can use to get the user if.
async fn authenticate<T>(req: &Request<T>) -> Result<authentication::Authentication, Status> {
    if let Some(api_key) = req.metadata().get(X_USER_ID) {
        let user_id = api_key
            .to_str()
            .map_err(|_| Status::new(Code::Internal, "x-user-id not found"))?;

        let user_id: u32 = user_id
            .parse::<u32>()
            .map_err(|_| Status::new(Code::Internal, "x-user-id not parseable as unsigned int"))?;

        Ok(authentication::Authentication {
            user_id: user_id as i32,
        })
    } else {
        Err(Status::new(
            Code::PermissionDenied,
            "You need to set an API Key",
        ))
    }
}
