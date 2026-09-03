from ..config import load_config
from ..dataverse_client import dv_get, who_am_i


def discover_entity(logical_name: str) -> None:
    data = dv_get(
        f"/EntityDefinitions(LogicalName='{logical_name}')"
        "?$select=LogicalName,EntitySetName,PrimaryIdAttribute,PrimaryNameAttribute"
    )
    print(f"Entity: {data['LogicalName']}")
    print(f"  EntitySetName (für API-Pfad): {data['EntitySetName']}")
    print(f"  PrimaryIdAttribute: {data['PrimaryIdAttribute']}")
    print(f"  PrimaryNameAttribute: {data['PrimaryNameAttribute']}")

    attrs = dv_get(
        f"/EntityDefinitions(LogicalName='{logical_name}')/Attributes"
        "?$select=LogicalName,AttributeType,DisplayName,SchemaName"
    )
    print("  Felder:")
    for a in sorted(attrs["value"], key=lambda x: x["LogicalName"]):
        label = (a.get("DisplayName") or {}).get("UserLocalizedLabel", {}) or {}
        label = label.get("Label", "") if label else ""
        schema_name = a.get("SchemaName", "")
        print(f"    {a['LogicalName']:<35} {str(a['AttributeType']):<15} SchemaName={schema_name:<30} {label}")


def find_entities(like_text: str) -> None:
    data = dv_get(
        "/EntityDefinitions?$select=LogicalName,EntitySetName,DisplayName"
        f"&$filter=contains(LogicalName,'{like_text}')"
    )
    print(f'Entities mit "{like_text}" im Namen:')
    for e in data["value"]:
        label = (e.get("DisplayName") or {}).get("UserLocalizedLabel", {}) or {}
        label = label.get("Label", "") if label else ""
        print(f"  {e['LogicalName']:<35} set={e['EntitySetName']:<35} {label}")


def lookup_bind_name(entity_logical_name: str, attribute_logical_name: str) -> None:
    """Ermittelt den exakten (case-sensitiven) Namen, der fuer '<name>@odata.bind'
    bei einem Lookup-Feld verwendet werden muss (Dataverse erwartet hier den
    SchemaName, nicht den kleingeschriebenen LogicalName)."""
    data = dv_get(
        f"/EntityDefinitions(LogicalName='{entity_logical_name}')"
        f"/Attributes(LogicalName='{attribute_logical_name}')"
        "/Microsoft.Dynamics.CRM.LookupAttributeMetadata"
        "?$select=SchemaName,LogicalName"
    )
    schema_name = data.get("SchemaName")
    if not schema_name:
        print(f"Konnte SchemaName fuer {entity_logical_name}.{attribute_logical_name} nicht ermitteln.")
        return
    bind_name = f"{schema_name}@odata.bind"
    print(f"SchemaName: {schema_name}")
    print(f"Bind-Property fuer @odata.bind: {bind_name}")


def get_own_bookable_resources(user_id: str) -> list[dict]:
    """Sucht bookableresources, die zum angegebenen UserId gehoeren."""
    data = dv_get(
        "/bookableresources?$select=bookableresourceid,name"
        f"&$filter=_userid_value eq {user_id}"
    )
    return data.get("value", [])


def find_my_resource() -> None:
    """Sucht die bookableresource, die zum aktuell angemeldeten Benutzer gehoert.

    Wird ueber die Standard-Lookup-Beziehung bookableresource -> userid gesucht.
    Falls euer Tenant eine andere Verknuepfung nutzt, mit
    'psa discover entity bookableresource' die Felder pruefen.
    """
    who = who_am_i()
    user_id = who["UserId"]
    results = get_own_bookable_resources(user_id)
    if not results:
        print(
            f"Keine bookableresource fuer UserId={user_id} gefunden "
            "(evtl. andere Verknuepfung im Tenant - siehe "
            "'psa discover entity bookableresource')."
        )
        return
    for r in results:
        print(f"resourceId={r['bookableresourceid']}  name={r.get('name', '')}")
    if len(results) == 1:
        print("\nZum Uebernehmen:")
        print(f"  psa config set resourceId {results[0]['bookableresourceid']}")


def find_my_projects() -> None:
    """Testet, ob die Projekt-Team-Mitgliedschaft (mapping.myProjectsEntitySet)
    fuer die aktuelle resourceId funktioniert - das steuert, ob 'psa add' nur
    eigene Projekte anzeigt oder auf alle Projekte zurueckfaellt.
    """
    config = load_config()
    mapping = config["mapping"]
    if not config.get("resourceId"):
        print("resourceId ist nicht gesetzt. Zuerst: psa discover myresource")
        return

    entity_set = mapping["myProjectsEntitySet"]
    resource_field = mapping["myProjectsResourceValueField"]
    project_field = mapping["myProjectsProjectValueField"]

    path = (
        f"/{entity_set}?$filter={resource_field} eq {config['resourceId']}"
        f"&$select={project_field}"
        f"&$expand=msdyn_project($select={mapping['projectNameField']})"
    )
    try:
        data = dv_get(path)
    except Exception as err:  # noqa: BLE001
        print(f"Abfrage gegen '{entity_set}' fehlgeschlagen: {err}")
        print(
            "-> Entweder existiert die Entitaet/Feldnamen so nicht in eurem Tenant, "
            "oder ihr habt keine Berechtigung darauf. Mit 'psa discover find project' "
            "bzw. 'psa discover find team' nach der richtigen Entitaet suchen und "
            "'psa config set mapping.myProjectsEntitySet ...' etc. anpassen. "
            "Bis dahin faellt 'psa add' automatisch auf alle Projekte zurueck."
        )
        return

    rows = data.get("value", [])
    if not rows:
        print(
            f"Die Abfrage gegen '{entity_set}' funktioniert, liefert aber keine "
            "Zeilen fuer diese resourceId. Entweder bist du in keinem Projektteam "
            "eingetragen, oder das Feld-Mapping ist falsch."
        )
        return

    print(f"Projekte, in denen du Teammitglied bist ({len(rows)}):")
    for r in rows:
        project = r.get("msdyn_project") or {}
        name = project.get(mapping["projectNameField"], "?")
        print(f"  {r.get(project_field)}  {name}")
