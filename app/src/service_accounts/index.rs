use crate::authentication::Authentication;
use crate::cornucopia::queries;
use crate::errors::CustomError;
use axum::{extract::Extension, response::Html};
use deadpool_postgres::Pool;

pub async fn index(
    current_user: Authentication,
    Extension(pool): Extension<Pool>,
) -> Result<Html<String>, CustomError> {
    let client = pool.get().await?;

    let service_accounts =
        queries::service_accounts::get_all(&client, &(current_user.user_id as i32)).await?;

    //let vaults = queries::vaults::get_all(&client, &(current_user.user_id as i32)).await?;
    let environments_and_vaults =
        queries::environments::get_environments_and_vaults(&client, &(current_user.user_id as i32)).await?;

    if service_accounts.is_empty() {
        let mut buf = Vec::new();
        crate::templates::service_accounts::empty_html(&mut buf, "Your Vaults").unwrap();
        let html = format!("{}", String::from_utf8_lossy(&buf));
    
        Ok(Html(html))
    } else {
        let mut buf = Vec::new();
        crate::templates::service_accounts::index_html(&mut buf, "Your Vaults", service_accounts, environments_and_vaults).unwrap();
        let html = format!("{}", String::from_utf8_lossy(&buf));
    
        Ok(Html(html))
    }
}
