import datetime

import click

from ..db import list_entries

STATUS_COLOR = {
    "new": "yellow",
    "modified": "yellow",
    "synced": "green",
    "deleted": "red",
}

STATUS_LABEL = {
    "new": "neu",
    "modified": "geändert",
    "synced": "synchron",
    "deleted": "gelöscht (pending)",
}


def week_range(week_date: str | None) -> tuple[str, str]:
    base = datetime.date.fromisoformat(week_date) if week_date else datetime.date.today()
    start = base - datetime.timedelta(days=base.isoweekday() - 1)
    end = start + datetime.timedelta(days=6)
    return start.isoformat(), end.isoformat()


def _entry_status_color(entry_status: str | None) -> str:
    if not entry_status:
        return "white"
    text = entry_status.lower()
    if "genehmigt" in text or "approved" in text:
        return "green"
    if "abgelehnt" in text or "rejected" in text:
        return "red"
    if "eingereicht" in text or "submitted" in text:
        return "cyan"
    return "white"


def print_week(week_date: str | None = None):
    from_date, to_date = week_range(week_date)
    entries = list_entries(from_date, to_date)

    click.secho(f"Woche {from_date} – {to_date}", bold=True)
    if not entries:
        click.secho("  (keine Einträge)", fg="white", dim=True)
        return entries

    total_hours = 0.0
    for e in entries:
        total_hours += e["hours"]
        proj = e["project_name"] or "(kein Projekt)"
        task = f" / {e['task_name']}" if e["task_name"] else ""
        status_label = STATUS_LABEL.get(e["status"], e["status"])
        status_color = "red" if e["error"] else STATUS_COLOR.get(e["status"], "white")
        line = (
            f"  #{str(e['id']).ljust(4)} {e['work_date']}  {str(e['hours']).rjust(4)}h  {proj}{task}"
        )
        if e["description"]:
            line += f"  – {e['description']}"
        click.echo(line, nl=False)
        click.secho(f"  [{status_label}]", fg=status_color, nl=False)
        if e["entry_status"]:
            click.secho(f"  ({e['entry_status']})", fg=_entry_status_color(e["entry_status"]), nl=False)
        if e["error"]:
            click.secho(f" – letzter Fehler: {e['error']}", fg="red")
        else:
            click.echo("")
    click.secho(f"  Summe: {total_hours}h", bold=True)
    return entries
