import click

from .auth import get_access_token, logout as auth_logout
from .commands.discover import (
    discover_entity,
    find_entities,
    find_my_projects,
    find_my_resource,
    get_own_bookable_resources,
    lookup_bind_name,
)
from .commands.entry import add_entry_interactive, edit_entry_interactive
from .commands.pull import run_pull
from .commands.sync import run_sync
from .commands.week import print_week
from .config import get_config_value, load_config, set_config_value
from .dataverse_client import who_am_i
from .db import list_entries, mark_deleted


@click.group()
def main():
    """CLI zum Erfassen und Synchronisieren von Zeiteinträgen in Dynamics 365 Project Operations."""


def _auto_setup_resource_id(user_id: str) -> None:
    """Wird nach dem Login aufgerufen: setzt resourceId automatisch, falls noch
    nicht konfiguriert und eindeutig anhand des angemeldeten Benutzers ermittelbar."""
    if load_config().get("resourceId"):
        return  # schon gesetzt, nichts zu tun
    try:
        results = get_own_bookable_resources(user_id)
    except Exception as err:  # noqa: BLE001
        click.secho(
            f"Hinweis: resourceId konnte nicht automatisch ermittelt werden ({err}). "
            "Manuell mit 'psa discover myresource' prüfen.",
            fg="yellow",
        )
        return
    if len(results) == 1:
        resource = results[0]
        set_config_value("resourceId", resource["bookableresourceid"])
        click.secho(
            f"resourceId automatisch gesetzt: {resource['bookableresourceid']} "
            f"({resource.get('name', '')})",
            fg="green",
        )
    elif len(results) > 1:
        click.secho(
            "Mehrere bookableresources fuer deinen Benutzer gefunden - bitte manuell wählen:",
            fg="yellow",
        )
        for r in results:
            click.echo(f"  psa config set resourceId {r['bookableresourceid']}   # {r.get('name', '')}")
    else:
        click.secho(
            "Hinweis: Keine bookableresource fuer deinen Benutzer gefunden - "
            "'psa add' zeigt daher zunächst alle Projekte statt nur deiner eigenen. "
            "Mit 'psa discover myresource' prüfen, falls das nicht stimmt.",
            fg="yellow",
        )


@main.command()
def login():
    """Beim Dynamics-Tenant anmelden (Device Code Flow) - ermittelt danach automatisch
    die eigene resourceId, falls diese noch nicht konfiguriert ist."""
    try:
        get_access_token(force_login=True)
        who = who_am_i()
        click.secho(f"Angemeldet. BusinessUnitId={who['BusinessUnitId']} UserId={who['UserId']}", fg="green")
        _auto_setup_resource_id(who["UserId"])
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@main.command()
def logout():
    """Lokal gespeicherte Anmeldung entfernen."""
    auth_logout()
    click.secho("Abgemeldet.", fg="green")


@main.group()
def config():
    """Konfiguration anzeigen/ändern."""


@config.command("show")
def config_show():
    """Aktuelle Konfiguration anzeigen."""
    import json

    click.echo(json.dumps(load_config(), indent=2, ensure_ascii=False))


@config.command("set")
@click.argument("key")
@click.argument("value")
def config_set(key, value):
    """Konfigurationswert setzen, z.B. 'psa config set tenantId <id>'."""
    set_config_value(key, value)
    click.secho(f"{key} = {value}", fg="green")


@config.command("get")
@click.argument("key")
def config_get(key):
    """Konfigurationswert anzeigen."""
    click.echo(get_config_value(key))


@main.command()
def add():
    """Neuen Zeiteintrag interaktiv erfassen (mit Projekt-/Task-Autocomplete)."""
    try:
        add_entry_interactive()
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@main.command()
@click.argument("entry_id", type=int)
def edit(entry_id):
    """Bestehenden Eintrag bearbeiten."""
    try:
        edit_entry_interactive(entry_id)
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@main.command()
@click.argument("entry_id", type=int)
def remove(entry_id):
    """Eintrag löschen (lokal sofort, remote beim nächsten 'psa sync')."""
    try:
        mark_deleted(entry_id)
        click.secho(f"Eintrag #{entry_id} zum Löschen vorgemerkt.", fg="green")
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@main.command()
@click.argument("week_date", required=False)
def week(week_date):
    """Einträge der Woche anzeigen (optional: Datum in dieser Woche, sonst aktuelle Woche)."""
    print_week(week_date)


@main.command("list")
def list_cmd():
    """Alle lokalen Einträge anzeigen."""
    entries = list_entries(include_deleted=True)
    for e in entries:
        line = (
            f"#{e['id']} {e['work_date']} {e['hours']}h "
            f"{e['project_name'] or ''} {e['task_name'] or ''} [{e['status']}]"
        )
        if e["entry_status"]:
            line += f" ({e['entry_status']})"
        if e["error"]:
            line += f" ERROR: {e['error']}"
        click.echo(line)


@main.command()
@click.argument("week_date", required=False)
@click.option("--from", "from_date", help="Startdatum YYYY-MM-DD (statt Wochenbezug)")
@click.option("--to", "to_date", help="Enddatum YYYY-MM-DD (statt Wochenbezug)")
def pull(week_date, from_date, to_date):
    """Bestehende Zeiteinträge aus Dataverse (z.B. aus dem PSA-Web-Tool) lokal laden."""
    try:
        run_pull(from_date=from_date, to_date=to_date, week_date=week_date)
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@main.command()
@click.option("--dry-run", is_flag=True, help="Nur anzeigen, was passieren würde, ohne zu schreiben.")
def sync(dry_run):
    """Ausstehende Änderungen mit Dataverse synchronisieren."""
    try:
        run_sync(dry_run=dry_run)
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@main.group()
def discover():
    """Dataverse-Schema erkunden (hilft beim Mapping)."""


@discover.command("entity")
@click.argument("logical_name")
def discover_entity_cmd(logical_name):
    """Felder einer Entität auflisten, z.B. 'psa discover entity msdyn_timeentry'."""
    try:
        discover_entity(logical_name)
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@discover.command("find")
@click.argument("text")
def discover_find_cmd(text):
    """Entitäten suchen, deren logischer Name den Text enthält, z.B. 'psa discover find project'."""
    try:
        find_entities(text)
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@discover.command("myresource")
def discover_myresource_cmd():
    """Eigene bookableresource-ID anhand des angemeldeten Benutzers suchen."""
    try:
        find_my_resource()
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@discover.command("myprojects")
def discover_myprojects_cmd():
    """Testet die Projekt-Team-Filterung (mapping.myProjects*) für die eigene resourceId."""
    try:
        find_my_projects()
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


@discover.command("bindname")
@click.argument("entity_logical_name")
@click.argument("attribute_logical_name")
def discover_bindname_cmd(entity_logical_name, attribute_logical_name):
    """Ermittelt den exakten '<Name>@odata.bind'-Namen eines Lookup-Feldes, z.B.
    'psa discover bindname msdyn_timeentry msdyn_projecttask'."""
    try:
        lookup_bind_name(entity_logical_name, attribute_logical_name)
    except Exception as err:  # noqa: BLE001
        click.secho(str(err), fg="red")
        raise SystemExit(1)


if __name__ == "__main__":
    main()
