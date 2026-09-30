//! Health checks for configuration, authentication and the local database.

use crate::{commands::AppContext, db, mapping};
use chrono_tz::Tz;
use medi_rs::{MediCommand, medi_handler, medi_module};

/// A single health check with its outcome and a hint on how to fix it.
#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub ok: bool,
    pub label: String,
    pub hint: String,
}

/// Result of the `psa doctor` command.
#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub healthy: bool,
    pub checks: Vec<HealthCheck>,
}

#[derive(MediCommand)]
#[medi_command(return_type = DoctorReport, error_type = String)]
pub struct Doctor;

#[medi_handler]
async fn doctor(context: AppContext, _: Doctor) -> Result<DoctorReport, String> {
    let config = &context.config;
    let mut healthy = true;
    let mut checks = Vec::new();

    fn record(
        healthy: &mut bool,
        checks: &mut Vec<HealthCheck>,
        ok: bool,
        label: String,
        hint: String,
    ) {
        *healthy &= ok;
        checks.push(HealthCheck { ok, label, hint });
    }

    let env = mapping::value(config, "/environmentUrl");
    record(
        &mut healthy,
        &mut checks,
        !env.is_empty(),
        "environmentUrl gesetzt".into(),
        "psa config set environmentUrl <URL>".into(),
    );

    let client_id = mapping::value(config, "/clientId");
    record(
        &mut healthy,
        &mut checks,
        !client_id.is_empty(),
        "clientId gesetzt".into(),
        "psa config set clientId <ID>".into(),
    );

    let resource_id = mapping::value(config, "/resourceId");
    record(
        &mut healthy,
        &mut checks,
        !resource_id.is_empty(),
        "resourceId gesetzt".into(),
        "psa discover myresource".into(),
    );

    let timezone = mapping::value(config, "/mapping/timezone");
    record(
        &mut healthy,
        &mut checks,
        timezone.parse::<Tz>().is_ok(),
        format!("Zeitzone gültig ({timezone})"),
        "psa config set mapping.timezone <IANA>".into(),
    );

    let logged_in = context.paths.token_cache_file.exists();
    record(
        &mut healthy,
        &mut checks,
        logged_in,
        "Angemeldet (Token-Cache vorhanden)".into(),
        "psa login".into(),
    );

    match db::open(&context.paths) {
        Ok(connection) => {
            let total = db::list(&connection, None, None, true)
                .map(|entries| entries.len())
                .unwrap_or(0);
            let pending = db::pending(&connection)
                .map(|entries| entries.len())
                .unwrap_or(0);
            record(
                &mut healthy,
                &mut checks,
                true,
                format!("Datenbank lesbar ({total} Einträge, {pending} ausstehend)"),
                String::new(),
            );
        }
        Err(error) => {
            healthy = false;
            checks.push(HealthCheck {
                ok: false,
                label: format!("Datenbank nicht lesbar: {error}"),
                hint: String::new(),
            });
        }
    }

    if !env.is_empty() && logged_in {
        match mapping::client(&context.paths, config).await {
            Ok(client) => match client.get("/WhoAmI", false).await {
                Ok(who) => {
                    let user = who["UserId"].as_str().unwrap_or("?");
                    record(
                        &mut healthy,
                        &mut checks,
                        true,
                        format!("Dataverse erreichbar (UserId={user})"),
                        String::new(),
                    );
                }
                Err(error) => {
                    healthy = false;
                    checks.push(HealthCheck {
                        ok: false,
                        label: format!("Dataverse nicht erreichbar: {error}"),
                        hint: String::new(),
                    });
                }
            },
            Err(error) => {
                healthy = false;
                checks.push(HealthCheck {
                    ok: false,
                    label: format!("Anmeldung konnte nicht geprüft werden: {error}"),
                    hint: String::new(),
                });
            }
        }
    }

    Ok(DoctorReport { healthy, checks })
}

medi_module! {
    manifest doctor_commands;
    commands {
        crate::commands::doctor::Doctor => crate::commands::doctor::doctor;
    }
}
