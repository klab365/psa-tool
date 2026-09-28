//! Configuration use cases.

use crate::{commands::AppContext, config};
use medi_rs::{MediCommand, medi_handler, medi_module};
use serde_json::Value;

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct ShowConfig;

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct GetConfig {
    pub key: String,
}

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct SetConfig {
    pub key: String,
    pub value: String,
}

#[medi_handler]
async fn show_config(context: AppContext, _: ShowConfig) -> Result<String, String> {
    serde_json::to_string_pretty(&context.config).map_err(|error| error.to_string())
}

#[medi_handler]
async fn get_config(context: AppContext, command: GetConfig) -> Result<String, String> {
    let value = command
        .key
        .split('.')
        .try_fold(&context.config, |value, key| value.get(key));
    Ok(value
        .map(Value::to_string)
        .unwrap_or("null".into())
        .trim_matches('"')
        .to_owned())
}

#[medi_handler]
async fn set_config(context: AppContext, command: SetConfig) -> Result<String, String> {
    let mut config = context.config;
    config::set_value(&mut config, &command.key, command.value.clone())
        .map_err(|error| error.to_string())?;
    config::save(&context.paths, &config).map_err(|error| error.to_string())?;
    Ok(format!("{} = {}", command.key, command.value))
}

medi_module! {
    manifest config_commands;
    commands {
        crate::commands::config::ShowConfig => crate::commands::config::show_config;
        crate::commands::config::GetConfig => crate::commands::config::get_config;
        crate::commands::config::SetConfig => crate::commands::config::set_config;
    }
}
