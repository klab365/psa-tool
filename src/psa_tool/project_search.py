from .config import load_config
from .dataverse_client import DataverseError, dv_get

# Modul-weiter Cache pro Prozesslauf, damit "psa add" nicht bei jedem Aufruf
# (Projekt- und Task-Suche) erneut die Team-Mitgliedschaft abfragt.
_my_project_ids_cache: dict[str, list[str] | None] = {}
_warned_fallback = False


def _escape(value: str) -> str:
    return (value or "").replace("'", "''")


def get_my_project_ids(config: dict) -> list[str] | None:
    """Liefert die Projekt-IDs, in denen die konfigurierte resourceId Teammitglied ist.

    Gibt None zurueck, wenn die Abfrage fehlschlaegt (z.B. falsches Mapping) -
    dann wird vom Aufrufer NICHT gefiltert (Fallback: alle Projekte)."""
    global _warned_fallback
    resource_id = config.get("resourceId")
    if not resource_id:
        return None
    if resource_id in _my_project_ids_cache:
        return _my_project_ids_cache[resource_id]

    mapping = config["mapping"]
    entity_set = mapping["myProjectsEntitySet"]
    resource_field = mapping["myProjectsResourceValueField"]
    project_field = mapping["myProjectsProjectValueField"]
    path = f"/{entity_set}?$filter={resource_field} eq {resource_id}&$select={project_field}"

    try:
        data = dv_get(path)
        ids = [row[project_field] for row in data.get("value", []) if row.get(project_field)]
        _my_project_ids_cache[resource_id] = ids
        return ids
    except DataverseError:
        if not _warned_fallback:
            print(
                "Hinweis: Konnte Projekt-Team-Mitgliedschaft nicht ermitteln "
                f"(Entitaet '{entity_set}') - zeige stattdessen alle Projekte. "
                "Mit 'psa discover myprojects' pruefen/anpassen."
            )
            _warned_fallback = True
        _my_project_ids_cache[resource_id] = None
        return None


def search_projects(query: str, top: int = 50) -> tuple[list[dict], bool]:
    """Sucht Projekte. Gibt (Treffer, has_more) zurueck, wobei has_more anzeigt,
    dass es mehr Treffer gibt, als angezeigt werden (Suche weiter eingrenzen).

    Wenn mapping.restrictToMyProjects aktiv ist und die eigene resourceId gesetzt
    ist, wird zusaetzlich auf Projekte eingeschraenkt, in denen man Teammitglied
    ist (siehe get_my_project_ids)."""
    config = load_config()
    mapping = config["mapping"]
    q = _escape(query)

    filters = []
    if q:
        filters.append(f"contains({mapping['projectNameField']},'{q}')")

    if mapping.get("restrictToMyProjects"):
        my_ids = get_my_project_ids(config)
        if my_ids is not None:
            if not my_ids:
                return [], False
            id_filter = " or ".join(f"{mapping['projectIdField']} eq {pid}" for pid in my_ids)
            filters.append(f"({id_filter})")

    filter_part = f"$filter={' and '.join(filters)}&" if filters else ""
    path = (
        f"/{mapping['projectEntitySet']}?{filter_part}"
        f"$select={mapping['projectIdField']},{mapping['projectNameField']}"
        f"&$top={top + 1}&$orderby={mapping['projectNameField']} asc"
    )
    data = dv_get(path)
    rows = data.get("value", [])
    has_more = len(rows) > top
    rows = rows[:top]
    results = [
        {"id": r[mapping["projectIdField"]], "name": r[mapping["projectNameField"]]} for r in rows
    ]
    return results, has_more


def search_tasks(query: str, project_id: str | None, top: int = 50) -> tuple[list[dict], bool]:
    config = load_config()
    mapping = config["mapping"]
    q = _escape(query)
    filters = []
    if q:
        filters.append(f"contains({mapping['taskNameField']},'{q}')")
    if project_id:
        filters.append(f"{mapping['taskProjectLookupField']} eq {project_id}")
    filter_part = f"$filter={' and '.join(filters)}&" if filters else ""
    path = (
        f"/{mapping['taskEntitySet']}?{filter_part}"
        f"$select={mapping['taskIdField']},{mapping['taskNameField']}"
        f"&$top={top + 1}&$orderby={mapping['taskNameField']} asc"
    )
    data = dv_get(path)
    rows = data.get("value", [])
    has_more = len(rows) > top
    rows = rows[:top]
    results = [{"id": r[mapping["taskIdField"]], "name": r[mapping["taskNameField"]]} for r in rows]
    return results, has_more
