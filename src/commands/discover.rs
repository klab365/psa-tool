//! Inspection of Dataverse metadata for diagnostic purposes.

use crate::{commands::AppContext, mapping, project_search};
use medi_rs::{MediCommand, medi_handler, medi_module};

/// Fetches a Dataverse endpoint and returns its pretty-printed JSON.
async fn raw(context: &AppContext, path: &str) -> Result<String, String> {
    let response = mapping::client(&context.paths, &context.config)
        .await?
        .get(path, false)
        .await
        .map_err(|error| error.to_string())?;
    serde_json::to_string_pretty(&response).map_err(|error| error.to_string())
}

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct DiscoverMyProjects;

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct DiscoverEntity {
    pub logical_name: String,
}

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct DiscoverFind {
    pub text: String,
}

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct DiscoverMyresource;

#[derive(MediCommand)]
#[medi_command(return_type = String, error_type = String)]
pub struct DiscoverBindname {
    pub entity_logical_name: String,
    pub attribute_logical_name: String,
}

#[medi_handler]
async fn my_projects(context: AppContext, _: DiscoverMyProjects) -> Result<String, String> {
    let projects = project_search::my_projects(&context.paths, &context.config).await?;
    if projects.is_empty() {
        return Ok("Keine Projekte gefunden.".into());
    }
    Ok(projects
        .into_iter()
        .map(|project| project.to_string())
        .collect::<Vec<_>>()
        .join("\n"))
}

#[medi_handler]
async fn entity(context: AppContext, command: DiscoverEntity) -> Result<String, String> {
    let logical_name = command.logical_name;
    let client = mapping::client(&context.paths, &context.config).await?;
    let metadata = client
        .get(
            &format!(
                "/EntityDefinitions(LogicalName='{logical_name}')?$select=LogicalName,EntitySetName,PrimaryIdAttribute,PrimaryNameAttribute"
            ),
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    let attrs = client
        .get(
            &format!(
                "/EntityDefinitions(LogicalName='{logical_name}')/Attributes?$select=LogicalName,AttributeType"
            ),
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut attributes: Vec<String> = attrs["value"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|attr| {
            Some(format!(
                "{:<40} {}",
                attr["LogicalName"].as_str()?,
                attr["AttributeType"].as_str()?
            ))
        })
        .collect();
    attributes.sort();
    let mut output = serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?;
    output.push_str("\n\nAttribute:\n");
    for attribute in attributes {
        output.push_str(&format!("  {attribute}\n"));
    }
    Ok(output)
}

#[medi_handler]
async fn find(context: AppContext, command: DiscoverFind) -> Result<String, String> {
    raw(
        &context,
        &format!(
            "/EntityDefinitions?$select=LogicalName,EntitySetName,DisplayName&$filter=contains(LogicalName,'{}')",
            command.text.replace('\'', "''")
        ),
    )
    .await
}

#[medi_handler]
async fn my_resource(context: AppContext, _: DiscoverMyresource) -> Result<String, String> {
    raw(&context, "/WhoAmI").await
}

#[medi_handler]
async fn bindname(context: AppContext, command: DiscoverBindname) -> Result<String, String> {
    raw(
        &context,
        &format!(
            "/EntityDefinitions(LogicalName='{}')/Attributes(LogicalName='{}')/Microsoft.Dynamics.CRM.LookupAttributeMetadata?$select=SchemaName,LogicalName",
            command.entity_logical_name, command.attribute_logical_name
        ),
    )
    .await
}

medi_module! {
    manifest discover_commands;
    commands {
        crate::commands::discover::DiscoverMyProjects => crate::commands::discover::my_projects;
        crate::commands::discover::DiscoverEntity => crate::commands::discover::entity;
        crate::commands::discover::DiscoverFind => crate::commands::discover::find;
        crate::commands::discover::DiscoverMyresource => crate::commands::discover::my_resource;
        crate::commands::discover::DiscoverBindname => crate::commands::discover::bindname;
    }
}
