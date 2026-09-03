import datetime as dt
from zoneinfo import ZoneInfo

import click

from ..config import load_config
from ..dataverse_client import dv_get
from ..db import get_by_remote_id, insert_synced_entry, list_entries, update_synced_fields, delete_local
from .week import week_range


def _duration_to_hours(value, mapping: dict) -> float:
    if mapping["durationUnit"] == "hours":
        return float(value)
    return float(value) / 60.0


def _formatted(record: dict, value_field: str):
    return record.get(f"{value_field}@OData.Community.Display.V1.FormattedValue")


def _utc_to_local_date(raw_value: str, mapping: dict) -> str:
    """Wandelt einen von Dataverse gelieferten UTC-Zeitstempel (z.B.
    '2026-08-31T22:00:00Z') in den Kalendertag der konfigurierten Zeitzone um
    (z.B. '2026-09-01' bei Europe/Vienna). Ohne diese Umrechnung wuerde ein am
    Abend (lokaler Zeit) erfasster Eintrag auf den Vortag fallen."""
    if not raw_value:
        return raw_value
    if mapping.get("dateOnly"):
        return raw_value[:10]
    normalized = raw_value.replace("Z", "+00:00")
    try:
        utc_dt = dt.datetime.fromisoformat(normalized)
    except ValueError:
        return raw_value[:10]
    if utc_dt.tzinfo is None:
        utc_dt = utc_dt.replace(tzinfo=dt.timezone.utc)
    local_dt = utc_dt.astimezone(ZoneInfo(mapping.get("timezone") or "UTC"))
    return local_dt.date().isoformat()


def run_pull(from_date: str | None = None, to_date: str | None = None, week_date: str | None = None) -> dict:
    config = load_config()
    mapping = config["mapping"]

    if not config.get("resourceId"):
        raise RuntimeError(
            "resourceId ist nicht gesetzt. Bitte zuerst ermitteln und setzen:\n"
            "  psa discover myresource\n"
            "  psa config set resourceId <GUID>"
        )

    if not from_date and not to_date:
        from_date, to_date = week_range(week_date)

    filters = [f"{mapping['resourceLookupValueField']} eq {config['resourceId']}"]
    if from_date:
        filters.append(f"{mapping['dateField']} ge {from_date}T00:00:00Z")
    if to_date:
        filters.append(f"{mapping['dateField']} le {to_date}T23:59:59Z")

    select_fields = [
        mapping["timeEntryIdField"],
        mapping["dateField"],
        mapping["durationField"],
        mapping["descriptionField"],
        mapping["projectLookupValueField"],
        mapping["taskLookupValueField"],
        mapping["entryStatusField"],
    ]
    path = (
        f"/{mapping['timeEntryEntitySet']}"
        f"?$filter={' and '.join(filters)}"
        f"&$select={','.join(select_fields)}"
        f"&$orderby={mapping['dateField']} asc"
    )
    data = dv_get(path, annotate=True)
    remote_rows = data.get("value", [])

    summary = {"created": 0, "updated": 0, "conflicts": 0, "removed": 0}
    seen_remote_ids = set()

    for r in remote_rows:
        remote_id = r[mapping["timeEntryIdField"]]
        seen_remote_ids.add(remote_id)

        raw_date = r.get(mapping["dateField"]) or ""
        work_date = _utc_to_local_date(raw_date, mapping)
        hours = _duration_to_hours(r.get(mapping["durationField"]) or 0, mapping)
        description = r.get(mapping["descriptionField"]) or ""
        project_id = r.get(mapping["projectLookupValueField"])
        project_name = _formatted(r, mapping["projectLookupValueField"])
        task_id = r.get(mapping["taskLookupValueField"])
        task_name = _formatted(r, mapping["taskLookupValueField"])
        entry_status = _formatted(r, mapping["entryStatusField"]) or r.get(mapping["entryStatusField"])

        fields = {
            "work_date": work_date,
            "project_id": project_id,
            "project_name": project_name,
            "task_id": task_id,
            "task_name": task_name,
            "hours": hours,
            "description": description,
            "entry_status": entry_status,
        }

        local = get_by_remote_id(remote_id)
        if local is None:
            insert_synced_entry(fields, remote_id)
            summary["created"] += 1
        elif local["status"] == "synced":
            update_synced_fields(local["id"], fields)
            summary["updated"] += 1
        else:
            summary["conflicts"] += 1
            click.secho(
                f"⚠ Konflikt bei Eintrag #{local['id']} ({local['work_date']}): "
                f"lokal Status='{local['status']}', aber auch remote vorhanden. "
                "Nicht automatisch überschrieben - bitte manuell prüfen "
                "(psa edit / psa remove) und dann erneut 'psa sync' bzw. 'psa pull'.",
                fg="yellow",
            )

    # Lokale, bereits synchronisierte Eintraege im Zeitraum, die remote nicht mehr
    # auftauchen -> wurden im Web-Tool geloescht -> lokal ebenfalls entfernen.
    for e in list_entries(from_date, to_date):
        if e["status"] == "synced" and e["remote_id"] and e["remote_id"] not in seen_remote_ids:
            delete_local(e["id"])
            summary["removed"] += 1

    click.secho(
        f"Pull fertig ({from_date} – {to_date}): "
        f"{summary['created']} neu, {summary['updated']} aktualisiert, "
        f"{summary['removed']} lokal entfernt (remote gelöscht), "
        f"{summary['conflicts']} Konflikt(e).",
        bold=True,
    )
    return summary
