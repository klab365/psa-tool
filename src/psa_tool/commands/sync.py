import click

from ..config import load_config
from ..dataverse_client import dv_create, dv_delete, dv_update
from ..db import delete_local, list_pending, set_sync_error, set_synced


def _duration_value(hours: float, mapping: dict) -> float:
    if mapping["durationUnit"] == "hours":
        return hours
    return round(hours * 60)  # minutes


def _date_value(work_date: str, mapping: dict) -> str:
    """msdyn_date ist ein DateTime-Feld -> volles ISO-Datetime senden.

    Falls mapping.dateOnly=true gesetzt wird (z.B. bei einem reinen Date-Feld),
    wird nur das Datum gesendet.
    """
    if mapping.get("dateOnly"):
        return work_date
    return f"{work_date}T00:00:00Z"


def _build_body(entry, config: dict) -> dict:
    mapping = config["mapping"]
    body = {
        mapping["dateField"]: _date_value(entry["work_date"], mapping),
        mapping["durationField"]: _duration_value(entry["hours"], mapping),
        mapping["descriptionField"]: entry["description"] or "",
    }
    if entry["project_id"]:
        body[mapping["projectLookupBind"]] = f"/{mapping['projectEntitySet']}({entry['project_id']})"
    if entry["task_id"]:
        body[mapping["taskLookupBind"]] = f"/{mapping['taskEntitySet']}({entry['task_id']})"
    if config.get("resourceId"):
        body[mapping["resourceLookupBind"]] = f"/bookableresources({config['resourceId']})"
    return body


def run_sync(dry_run: bool = False) -> dict:
    config = load_config()
    mapping = config["mapping"]
    pending = list_pending()

    summary = {"created": 0, "updated": 0, "deleted": 0, "failed": 0}

    if not pending:
        click.secho("Nichts zu synchronisieren – alles aktuell.", fg="green")
        return summary

    click.secho(f"{len(pending)} ausstehende Änderung(en) gefunden. Synchronisiere...", fg="cyan")

    for entry in pending:
        label = f"{entry['work_date']} | {entry['project_name'] or '?'} | {entry['hours']}h"
        try:
            if entry["status"] == "deleted":
                if dry_run:
                    click.secho(f"[dry-run] DELETE {label} (remote {entry['remote_id']})", fg="white", dim=True)
                else:
                    dv_delete(mapping["timeEntryEntitySet"], entry["remote_id"])
                    delete_local(entry["id"])
                summary["deleted"] += 1
                continue

            body = _build_body(entry, config)

            if entry["status"] == "new":
                if dry_run:
                    click.secho(f"[dry-run] CREATE {label}", fg="white", dim=True)
                else:
                    created = dv_create(mapping["timeEntryEntitySet"], body)
                    remote_id = created[mapping["timeEntryIdField"]]
                    set_synced(entry["id"], remote_id)
                summary["created"] += 1
                click.secho(f"✔ erstellt: {label}", fg="green")
            elif entry["status"] == "modified":
                if dry_run:
                    click.secho(f"[dry-run] UPDATE {label} (remote {entry['remote_id']})", fg="white", dim=True)
                else:
                    dv_update(mapping["timeEntryEntitySet"], entry["remote_id"], body)
                    set_synced(entry["id"], entry["remote_id"])
                summary["updated"] += 1
                click.secho(f"✔ aktualisiert: {label}", fg="green")
        except Exception as err:  # noqa: BLE001
            summary["failed"] += 1
            if not dry_run:
                set_sync_error(entry["id"], str(err))
            click.secho(f"✘ Fehler bei {label}: {err}", fg="red")

    click.echo("")
    click.secho(
        f"Fertig: {summary['created']} neu, {summary['updated']} aktualisiert, "
        f"{summary['deleted']} gelöscht, {summary['failed']} fehlgeschlagen.",
        bold=True,
    )
    return summary
