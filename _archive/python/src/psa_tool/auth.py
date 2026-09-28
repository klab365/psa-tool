import msal

from .config import load_config
from .paths import TOKEN_CACHE_FILE


class AuthConfigError(RuntimeError):
    pass


def _assert_auth_config(config: dict) -> None:
    if not config.get("environmentUrl"):
        raise AuthConfigError(
            "environmentUrl fehlt in der Config. Bitte eure Dataverse/PSA-Umgebung setzen, z.B.:\n"
            "  psa config set environmentUrl https://<eureorg>.crm4.dynamics.com"
        )
    if not config.get("clientId"):
        raise AuthConfigError(
            "clientId fehlt in der Config. Standardmaessig wird Microsofts oeffentliche "
            "Sample-App genutzt - falls das ueberschrieben wurde, bitte neu setzen, z.B.:\n"
            "  psa config set clientId <APP_CLIENT_ID>\n"
            "Alternativ: eigene Azure AD App Registration (Public Client, "
            '"Allow public client flows" = Yes) mit API-Berechtigung '
            '"Dynamics CRM -> user_impersonation" (delegiert).'
        )


def _load_cache() -> msal.SerializableTokenCache:
    cache = msal.SerializableTokenCache()
    if TOKEN_CACHE_FILE.exists():
        cache.deserialize(TOKEN_CACHE_FILE.read_text(encoding="utf-8"))
    return cache


def _persist_cache(cache: msal.SerializableTokenCache) -> None:
    if cache.has_state_changed:
        TOKEN_CACHE_FILE.write_text(cache.serialize(), encoding="utf-8")
        TOKEN_CACHE_FILE.chmod(0o600)


def _get_app(config: dict, cache: msal.SerializableTokenCache) -> msal.PublicClientApplication:
    tenant = config.get("tenantId") or "organizations"
    authority = f"https://login.microsoftonline.com/{tenant}"
    return msal.PublicClientApplication(
        client_id=config["clientId"],
        authority=authority,
        token_cache=cache,
    )


def _scopes(config: dict) -> list[str]:
    resource = config["environmentUrl"].rstrip("/")
    return [f"{resource}/.default"]


def get_access_token(force_login: bool = False) -> str:
    config = load_config()
    _assert_auth_config(config)
    cache = _load_cache()
    app = _get_app(config, cache)
    scopes = _scopes(config)

    if not force_login:
        accounts = app.get_accounts()
        if accounts:
            result = app.acquire_token_silent(scopes, account=accounts[0])
            if result and "access_token" in result:
                _persist_cache(cache)
                return result["access_token"]

    flow = app.initiate_device_flow(scopes=scopes)
    if "user_code" not in flow:
        raise RuntimeError(f"Device-Code-Flow konnte nicht gestartet werden: {flow}")

    print("\nBitte im Browser anmelden:", flush=True)
    print(flow["verification_uri"], flush=True)
    print(f"Code: {flow['user_code']}\n", flush=True)

    result = app.acquire_token_by_device_flow(flow)
    _persist_cache(cache)

    if "access_token" not in result:
        raise RuntimeError(
            f"Login fehlgeschlagen: {result.get('error')}: {result.get('error_description')}"
        )
    return result["access_token"]


def logout() -> None:
    config = load_config()
    cache = _load_cache()
    app = _get_app(config, cache)
    for account in app.get_accounts():
        app.remove_account(account)
    _persist_cache(cache)
