import questionary

from ..db import get_entry, insert_entry, update_entry
from ..project_search import search_projects, search_tasks


def _resolve_project(typed: str, pool: list[dict]):
    matches = [p for p in pool if p["name"] == typed]
    if len(matches) == 1:
        return matches[0]
    if len(matches) > 1:
        choices = [questionary.Choice(title=f"{p['name']}  ({p['id']})", value=p) for p in matches]
        return questionary.select("Mehrere Treffer mit gleichem Namen - bitte wählen:", choices=choices).ask()
    # Getippter Text war nicht (mehr) exakt im lokalen Pool -> live nachsuchen
    results, _ = search_projects(typed)
    if not results:
        print(f"Kein Projekt gefunden für '{typed}'.")
        return None
    if len(results) == 1:
        return results[0]
    choices = [questionary.Choice(title=p["name"], value=p) for p in results]
    return questionary.select("Mehrere Treffer - bitte wählen:", choices=choices).ask()


def _resolve_task(typed: str, pool: list[dict], project):
    matches = [t for t in pool if t["name"] == typed]
    if len(matches) == 1:
        return matches[0]
    if len(matches) > 1:
        choices = [questionary.Choice(title=f"{t['name']}  ({t['id']})", value=t) for t in matches]
        return questionary.select("Mehrere Treffer mit gleichem Namen - bitte wählen:", choices=choices).ask()
    results, _ = search_tasks(typed, project["id"] if project else None)
    if not results:
        print(f"Kein Task gefunden für '{typed}'.")
        return None
    if len(results) == 1:
        return results[0]
    choices = [questionary.Choice(title=t["name"], value=t) for t in results]
    return questionary.select("Mehrere Treffer - bitte wählen:", choices=choices).ask()


def _pick_project():
    """Echtes Suchfeld: tippen, Vorschläge erscheinen automatisch, Enter wählt aus."""
    pool, has_more = search_projects("", top=200)
    if has_more:
        print(
            f"(Es gibt mehr als {len(pool)} Projekte - falls deins nicht in den "
            "Vorschlägen auftaucht, einfach den vollen/genaueren Namen zu Ende tippen "
            "und Enter drücken, dann wird live nachgesucht.)"
        )
    if not pool:
        print("Keine Projekte gefunden.")
        return None
    names = [p["name"] for p in pool]
    typed = questionary.autocomplete(
        "Projekt (tippen zum Suchen, Enter zum Übernehmen; leer = kein Projekt):",
        choices=names,
    ).ask()
    if not typed:
        return None
    return _resolve_project(typed, pool)


def _pick_task(project):
    pool, has_more = search_tasks("", project["id"] if project else None, top=200)
    if has_more:
        print(
            f"(Es gibt mehr als {len(pool)} Tasks - falls deiner nicht in den "
            "Vorschlägen auftaucht, einfach den vollen/genaueren Namen zu Ende tippen "
            "und Enter drücken, dann wird live nachgesucht.)"
        )
    names = ["(kein Task)"] + [t["name"] for t in pool]
    typed = questionary.autocomplete(
        "Task (tippen zum Suchen, Enter zum Übernehmen; leer/'(kein Task)' = keiner):",
        choices=names,
    ).ask()
    if not typed or typed == "(kein Task)":
        return None
    return _resolve_task(typed, pool, project)


def _validate_date(text: str) -> bool | str:
    import datetime

    try:
        datetime.date.fromisoformat(text)
        return True
    except ValueError:
        return "Bitte im Format YYYY-MM-DD"


def _validate_hours(text: str) -> bool | str:
    try:
        value = float(text)
        return value > 0 or "Bitte eine positive Zahl"
    except ValueError:
        return "Bitte eine Zahl"


def add_entry_interactive(defaults: dict | None = None) -> int:
    import datetime

    defaults = defaults or {}
    project = _pick_project()
    task = _pick_task(project)

    work_date = questionary.text(
        "Datum (YYYY-MM-DD):",
        default=defaults.get("work_date", datetime.date.today().isoformat()),
        validate=_validate_date,
    ).ask()
    hours = questionary.text(
        "Stunden:", default=str(defaults.get("hours", "8")), validate=_validate_hours
    ).ask()
    description = questionary.text("Beschreibung:", default=defaults.get("description", "")).ask()

    entry = {
        "work_date": work_date,
        "project_id": project["id"] if project else None,
        "project_name": project["name"] if project else None,
        "task_id": task["id"] if task else None,
        "task_name": task["name"] if task else None,
        "hours": float(hours),
        "description": description,
    }
    entry_id = insert_entry(entry)
    print(f"Eintrag #{entry_id} gespeichert (lokal, noch nicht synchronisiert).")
    return entry_id


def edit_entry_interactive(entry_id: int) -> None:
    existing = get_entry(entry_id)
    if existing is None:
        print(f"Eintrag {entry_id} nicht gefunden.")
        return

    print(f"Bearbeite Eintrag #{entry_id} (Enter = übernehmen).")
    project = _pick_project()
    task = _pick_task(project)
    work_date = questionary.text("Datum:", default=existing["work_date"], validate=_validate_date).ask()
    hours = questionary.text("Stunden:", default=str(existing["hours"]), validate=_validate_hours).ask()
    description = questionary.text("Beschreibung:", default=existing["description"] or "").ask()
    update_entry(
        entry_id,
        {
            "work_date": work_date,
            "hours": float(hours),
            "description": description,
            "project_id": project["id"] if project else existing["project_id"],
            "project_name": project["name"] if project else existing["project_name"],
            "task_id": task["id"] if task else existing["task_id"],
            "task_name": task["name"] if task else existing["task_name"],
        },
    )
    print(f"Eintrag #{entry_id} aktualisiert.")
