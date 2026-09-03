import requests

from .auth import get_access_token
from .config import load_config


class DataverseError(RuntimeError):
    def __init__(self, message: str, status: int, body=None):
        super().__init__(message)
        self.status = status
        self.body = body


def _api_base(config: dict) -> str:
    return f"{config['environmentUrl'].rstrip('/')}/api/data/v9.2"


def _headers(token: str, annotate: bool = False) -> dict:
    prefer = "return=representation"
    if annotate:
        prefer += ', odata.include-annotations="OData.Community.Display.V1.FormattedValue"'
    return {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json; charset=utf-8",
        "Accept": "application/json",
        "OData-MaxVersion": "4.0",
        "OData-Version": "4.0",
        "Prefer": prefer,
    }


def _raise_if_error(res: requests.Response, context: str) -> None:
    if res.status_code >= 400:
        try:
            body = res.json()
            msg = body.get("error", {}).get("message", str(body))
        except ValueError:
            body = res.text
            msg = body
        raise DataverseError(f"Dataverse-Fehler ({context}): [{res.status_code}] {msg}", res.status_code, body)


def dv_get(path: str, annotate: bool = False) -> dict:
    config = load_config()
    token = get_access_token()
    res = requests.get(f"{_api_base(config)}{path}", headers=_headers(token, annotate=annotate))
    _raise_if_error(res, f"GET {path}")
    return res.json()


def dv_create(entity_set: str, body: dict) -> dict:
    config = load_config()
    token = get_access_token()
    res = requests.post(f"{_api_base(config)}/{entity_set}", headers=_headers(token), json=body)
    _raise_if_error(res, f"POST {entity_set}")
    return res.json()


def dv_update(entity_set: str, entity_id: str, body: dict) -> dict:
    config = load_config()
    token = get_access_token()
    res = requests.patch(
        f"{_api_base(config)}/{entity_set}({entity_id})", headers=_headers(token), json=body
    )
    _raise_if_error(res, f"PATCH {entity_set}({entity_id})")
    return res.json() if res.content else {}


def dv_delete(entity_set: str, entity_id: str) -> None:
    config = load_config()
    token = get_access_token()
    res = requests.delete(f"{_api_base(config)}/{entity_set}({entity_id})", headers=_headers(token))
    if res.status_code == 404:
        return  # bereits geloescht -> ok
    _raise_if_error(res, f"DELETE {entity_set}({entity_id})")


def who_am_i() -> dict:
    return dv_get("/WhoAmI")
