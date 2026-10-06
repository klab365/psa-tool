//! Authentication use cases.

use crate::{auth, commands::AppContext, dataverse::Client};
use medi_rs::{MediCommand, medi_handler, medi_module};

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct Login;

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct Logout;

#[medi_handler]
async fn login(context: AppContext, _: Login) -> Result<String, String> {
    let client = Client::new(&context.paths, &context.config, false)
        .await
        .map_err(|error| error.to_string())?;
    let who = client
        .get("/WhoAmI", false)
        .await
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "Angemeldet. BusinessUnitId={} UserId={}",
        who["BusinessUnitId"], who["UserId"]
    ))
}

#[medi_handler]
async fn logout(context: AppContext, _: Logout) -> Result<String, String> {
    auth::logout(&context.paths).map_err(|error| error.to_string())?;
    Ok("Abgemeldet.".into())
}

medi_module! {
    manifest auth_commands;
    commands {
        crate::commands::auth::Login => crate::commands::auth::login;
        crate::commands::auth::Logout => crate::commands::auth::logout;
    }
}
