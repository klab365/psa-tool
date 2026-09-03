import json
import os
from copy import deepcopy

from .paths import CONFIG_FILE, app_dir

# Standard-Feldbelegung fuer die Project-Operations "Time Entry" Entitaet
# (msdyn_timeentry). Diese Namen koennen je nach Solution-Version/Customizing
# abweichen -> mit "psa discover entity msdyn_timeentry" verifizieren und in
# der Config (psa config set mapping.xxx <value>) anpassen.
DEFAULT_MAPPING = {
    "timeEntryEntitySet": "msdyn_timeentries",
    "timeEntryIdField": "msdyn_timeentryid",
    "dateField": "msdyn_date",
    "dateOnly": False,  # true = nur Datum senden statt ISO-Datetime (falls Feld vom Typ "Date only" ist)
    "timezone": "UTC",  # IANA-Zeitzone, in der Dataverse-UTC-Zeitstempel als Kalendertag interpretiert werden (fuer 'psa pull') - z.B. "Europe/Zurich", "Europe/Berlin"
    "durationField": "msdyn_duration",  # Dataverse erwartet i.d.R. Minuten
    "durationUnit": "minutes",  # 'minutes' oder 'hours'
    "descriptionField": "msdyn_externaldescription",  # "Externe Kommentare" statt "Interne Notizen" (msdyn_description)
    "entryStatusField": "msdyn_entrystatus",  # Picklist-Wert; Klartext-Label kommt ueber die FormattedValue-Annotation
    "projectLookupField": "msdyn_project",
    "projectLookupBind": "msdyn_project@odata.bind",
    "projectLookupValueField": "_msdyn_project_value",
    "taskLookupField": "msdyn_projecttask",
    "taskLookupBind": "msdyn_projectTask@odata.bind",  # SchemaName ist camelCase, LogicalName kleingeschrieben
    "taskLookupValueField": "_msdyn_projecttask_value",
    "resourceLookupField": "msdyn_bookableresource",
    "resourceLookupBind": "msdyn_bookableresource@odata.bind",
    "resourceLookupValueField": "_msdyn_bookableresource_value",
    "projectEntitySet": "msdyn_projects",
    "projectIdField": "msdyn_projectid",
    "projectNameField": "msdyn_subject",
    "taskEntitySet": "msdyn_projecttasks",
    "taskIdField": "msdyn_projecttaskid",
    "taskNameField": "msdyn_subject",
    "taskProjectLookupField": "_msdyn_project_value",

    # Projekt-Team-Mitgliedschaft, um "psa add" auf die eigenen Projekte einzuschränken.
    # Falls diese Entität/Felder in eurem Tenant anders heissen, mit
    # "psa discover find team" bzw. "psa discover find project" die richtige Entität
    # suchen und hier anpassen. Mit "psa discover myprojects" testen.
    "restrictToMyProjects": True,
    "myProjectsEntitySet": "msdyn_projectteams",
    "myProjectsResourceValueField": "_msdyn_bookableresourceid_value",
    "myProjectsProjectValueField": "_msdyn_project_value",
}

# Von Microsoft in der offiziellen Dataverse-OAuth-Doku veroeffentlichte
# Sample-/Test-App (Public Client, Multi-Tenant), funktioniert i.d.R. ohne
# eigene Azure-AD-App-Registration. Fuer produktiven Einsatz bzw. falls euer
# Tenant Conditional-Access-Richtlinien mit App-Whitelist hat, solltet ihr
# eine eigene App Registration erstellen und clientId/tenantId ueberschreiben.
SAMPLE_CLIENT_ID = "51f81489-12ee-4a9e-aaae-a2591f45987d"

DEFAULT_CONFIG = {
    "tenantId": "",  # leer = Authority "organizations" (Tenant wird beim Login automatisch ermittelt)
    "clientId": SAMPLE_CLIENT_ID,
    "environmentUrl": "",  # z.B. https://<eureorg>.crm4.dynamics.com - MUSS gesetzt werden, siehe README
    "resourceId": "",  # bookableresource-GUID des eigenen Benutzers (optional)
    "mapping": DEFAULT_MAPPING,
}


def _deep_merge(base: dict, override: dict) -> dict:
    out = deepcopy(base)
    for k, v in (override or {}).items():
        if isinstance(v, dict) and isinstance(base.get(k), dict):
            out[k] = _deep_merge(base[k], v)
        else:
            out[k] = v
    return out


def load_config() -> dict:
    app_dir()
    if not CONFIG_FILE.exists():
        return deepcopy(DEFAULT_CONFIG)
    raw = json.loads(CONFIG_FILE.read_text(encoding="utf-8"))
    return _deep_merge(DEFAULT_CONFIG, raw)


def save_config(config: dict) -> None:
    app_dir()
    CONFIG_FILE.write_text(json.dumps(config, indent=2, ensure_ascii=False), encoding="utf-8")
    os.chmod(CONFIG_FILE, 0o600)


def set_config_value(dotted_key: str, value: str):
    config = load_config()
    parts = dotted_key.split(".")
    obj = config
    for part in parts[:-1]:
        if not isinstance(obj.get(part), dict):
            obj[part] = {}
        obj = obj[part]
    obj[parts[-1]] = value
    save_config(config)
    return config


def get_config_value(dotted_key: str):
    config = load_config()
    obj = config
    for part in dotted_key.split("."):
        if not isinstance(obj, dict):
            return None
        obj = obj.get(part)
    return obj
