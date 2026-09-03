# PSA Tool

Ein CLI-Tool (Python + [uv](https://docs.astral.sh/uv/)) zum Erfassen der
eigenen Wochenarbeitszeit und zum Synchronisieren mit **Microsoft Dynamics
365 Project Operations (PSA)** / Dataverse.

Workflow:

1. Woche mit `psa add` eintragen (Projekt/Task per Autocomplete-Suche, lokal
   in SQLite gespeichert).
2. Mit `psa week` kontrollieren.
3. Mit `psa sync` alle offenen Einträge nach Dataverse hochladen
   (Create/Update/Delete).

## Erste Schritte (Getting Started)

Komplett neu aufsetzen dauert nur ein paar Minuten - der Reihe nach:

```bash
# 1. Installieren (Details siehe "Installation" weiter unten)
mise trust && mise install
uv sync
source .venv/bin/activate

# 2. Eure Dataverse/PSA-Organisation eintragen (PFLICHT)
psa config set environmentUrl https://<eureorg>.crm4.dynamics.com

# 3. Anmelden - setzt beim ersten Mal i.d.R. auch automatisch eure resourceId
psa login

# 4. Eigene Zeitzone setzen (fuer korrekte Datumsanzeige bei "psa pull")
psa config set mapping.timezone Europe/Zurich   # oder eure eigene IANA-Zeitzone

# 5. Optional, aber empfohlen: pruefen, ob das Standard-Feld-Mapping zu eurem
#    Tenant passt (Project Operations kann pro Org leicht unterschiedliche
#    Feldnamen haben) - siehe Abschnitt "Feld-Mapping pruefen/anpassen"
psa discover entity msdyn_timeentry

# 6. Loslegen
psa pull      # bestehende Eintraege aus Dataverse laden
psa add       # eigene Zeit erfassen
psa week      # Kontrolle
psa sync      # hochladen
```

Falls bei Schritt 3 etwas nicht automatisch passt (z.B. mehrere
`bookableresource`-Treffer oder Feldnamen weichen ab), bekommt ihr im
Terminal jeweils einen konkreten Hinweis, welchen Befehl ihr als Nächstes
ausführen sollt - Details dazu in den Abschnitten weiter unten.

## Voraussetzungen

- [uv](https://docs.astral.sh/uv/) (Python-Paketmanager/-Runner) – wird über
  `mise.toml` automatisch in der passenden Version bereitgestellt, falls ihr
  [mise](https://mise.jdx.dev/) nutzt.
- **Keine eigene Azure AD App Registration zwingend nötig.** Das Tool nutzt
  standardmäßig Microsofts öffentliche Sample-/Test-App aus der offiziellen
  Dataverse-OAuth-Doku (Multi-Tenant Public Client, i.d.R. bereits in jedem
  Tenant nutzbar) sowie die Authority `organizations` (Tenant wird beim
  Device-Code-Login automatisch anhand des Kontos ermittelt). Damit reicht:

  ```bash
  psa login
  ```

  **Falls das nicht funktioniert** (z.B. weil euer Tenant Conditional-Access-
  Richtlinien mit App-Whitelist hat oder der Admin keine Fremd-Apps erlaubt),
  braucht ihr eine eigene App Registration:
  1. Azure Portal → Azure AD → App registrations → New registration
     - Supported account types: Single tenant
     - Redirect URI: keiner nötig (Device Code Flow)
  2. Authentication → "Allow public client flows" = **Yes**
  3. API permissions → Add a permission → APIs my organization uses →
     "Dynamics CRM" (bzw. der Name eurer Organisation, wenn dort schon
     Application-User-Setup existiert) → Delegated permissions →
     `user_impersonation` → hinzufügen → Admin consent erteilen.
  4. `Application (client) ID` und `Directory (tenant) ID` notieren und setzen:

     ```bash
     psa config set tenantId <TENANT_ID>
     psa config set clientId <APP_CLIENT_ID>
     ```

## Wie die Anmeldung (Auth) funktioniert

Das Tool nutzt den **OAuth 2.0 Device Code Flow** (Microsoft Entra ID /
Azure AD) über die Library `msal`. Ablauf im Detail:

1. `psa login` bzw. jeder Befehl, der Dataverse anspricht, ruft
   `get_access_token()` in `src/psa_tool/auth.py` auf.
2. Ist bereits ein gültiges (oder per Refresh-Token erneuerbares) Token im
   lokalen Cache (`~/.psa-tool/token_cache.json`), wird es **silent**
   wiederverwendet – kein erneuter Login nötig.
3. Andernfalls startet MSAL den Device-Code-Flow: Es wird ein Code angezeigt,
   du öffnest `https://login.microsoft.com/device` im Browser, gibst den Code
   ein und meldest dich mit deinem normalen O365-Konto an (inkl. MFA, falls
   aktiv). Passwort/MFA werden **nie** vom CLI-Tool selbst abgefragt oder
   gespeichert – das läuft komplett über die Microsoft-Login-Seite.
4. Nach erfolgreichem Login liefert Azure AD ein Access Token (Scope:
   `<environmentUrl>/.default`) sowie ein Refresh Token.
   Beides wird als JSON in `~/.psa-tool/token_cache.json` mit Datei-Rechten
   `0600` (nur dein Benutzer kann lesen) abgelegt – unverschlüsselt, aber
   durch die Dateiberechtigung vor anderen lokalen Benutzern geschützt.
5. Jeder Dataverse-Aufruf (`dataverse_client.py`) hängt das Access Token als
   `Authorization: Bearer <token>`-Header an.

**Zwei Bausteine, die man dabei braucht:**

| Baustein | Wofür | Standardwert in diesem Tool |
|---|---|---|
| `clientId` | Identifiziert die *App* (nicht dich persönlich) gegenüber Azure AD | Microsofts öffentliche Sample-App `51f81489-12ee-4a9e-aaae-a2591f45987d` aus der offiziellen Dataverse-OAuth-Doku – keine eigene Registrierung nötig |
| `tenantId` | Sagt Azure AD, in welchem Firma-Tenant sich der Benutzer anmelden soll | Standardmäßig leer → Authority `organizations` (Tenant wird beim Login automatisch anhand deines Kontos erkannt). Kann aber gezielt gesetzt werden: `psa config set tenantId <TENANT_ID>` |

Da eine `clientId` technisch fest zu Azure AD gehören muss (kann nicht "leer"
sein), aber keine eigene App Registration nötig ist, reicht euch faktisch
nur die `tenantId` zum Setzen – alles andere (Sample-`clientId`,
Authority) ist bereits vorkonfiguriert.

```bash
psa config set tenantId <EURE_TENANT_ID>
psa login
```

`psa logout` entfernt die lokal gecachten Konten/Tokens wieder
(`app.remove_account(...)` in `auth.py`), danach ist beim nächsten Aufruf
wieder ein Device-Code-Login nötig.

## Installation

### Option A: Aus dem geklonten Repo (fuer Entwicklung/Anpassungen)

Mit mise (empfohlen, pinnt Python + uv automatisch über `mise.toml`):

```bash
mise trust
mise install
uv sync
source .venv/bin/activate
```

Ohne mise, wenn `uv` bereits installiert ist:

```bash
uv sync
source .venv/bin/activate
```

Danach steht der Befehl `psa` im aktivierten venv zur Verfügung
(`uv run psa ...` funktioniert auch ohne vorheriges Aktivieren).

### Option B: Global via mise + pipx direkt aus dem Git-Repo (fuer reine Nutzung)

Ohne Klonen, direkt installiert und global im PATH verfügbar
([mise](https://mise.jdx.dev/) mit dem `pipx`-Backend):

```bash
mise use -g pipx:git+https://github.com/klab365/psa-tool.git
```

Danach steht `psa` global zur Verfügung (kein Aktivieren eines venv nötig).
Update auf eine neuere Version des Tools:

```bash
mise install -f pipx:git+https://github.com/klab365/psa-tool.git
```

## Konfiguration

**`environmentUrl` muss gesetzt werden** (eure Dataverse/PSA-Org-URL, z.B.
`https://<eureorg>.crm4.dynamics.com` - zu finden z.B. in den Dynamics-365-
Admin-Einstellungen oder in der Browser-Adresszeile eures PSA-Web-Tools):

```bash
psa config set environmentUrl https://<eureorg>.crm4.dynamics.com
psa config show
```

Optional, nur falls eigene App Registration genutzt wird (siehe oben):

```bash
psa config set tenantId <TENANT_ID>
psa config set clientId <APP_CLIENT_ID>
```

Konfiguration liegt lokal in `~/.psa-tool/config.json`, Token-Cache in
`~/.psa-tool/token_cache.json`, Zeiteinträge in `~/.psa-tool/psa.sqlite3`.

### Feld-Mapping prüfen/anpassen

Die Standardwerte gehen von der Standard-Entität `msdyn_timeentry` aus. Da
Project Operations je nach Version/Customizing abweichende Feldnamen haben
kann, gibt es Discovery-Befehle:

```bash
psa login                              # einmalig anmelden - ermittelt automatisch die eigene
                                        # resourceId, falls eindeutig zuordenbar (siehe unten)
psa discover find timeentry            # passende Entität(en) finden
psa discover entity msdyn_timeentry    # alle Felder auflisten
psa discover find project              # Projekt-/Task-Entitäten finden
psa discover entity msdyn_project
psa discover entity msdyn_projecttask
psa discover myresource                 # eigene bookableresource-ID manuell prüfen/ermitteln
psa discover myprojects                 # eigene Projekt-Team-Zugehörigkeit testen
```

Danach ggf. anpassen, z.B.:

```bash
psa config set mapping.durationUnit hours       # falls Dataverse Stunden statt Minuten erwartet
psa config set mapping.projectNameField msdyn_projectname
psa config set mapping.taskNameField msdyn_subject
```

### Eigene Resource-ID (bookableresource)

Wird für die Zuordnung "wer hat gebucht" sowie für die Filterung auf eigene
Projekte gebraucht. **`psa login` versucht automatisch**, anhand des
angemeldeten Benutzers eine eindeutige `bookableresource` zu finden und
`resourceId` direkt zu setzen - ist bereits ein Wert gesetzt, wird nichts
überschrieben.

Das klappt nur automatisch, wenn es **genau einen** Treffer gibt. Bei
mehreren oder keinem Treffer bekommst du beim Login einen Hinweis mit den
konkreten Befehlen zum manuellen Setzen, bzw. kannst jederzeit selbst prüfen:

```bash
psa discover myresource
psa config set resourceId <BOOKABLERESOURCE_GUID>
```

### Suche auf eigene Projekte einschränken

`psa add`/`psa edit` schränken die Projekt-Suche standardmäßig auf Projekte
ein, in denen die konfigurierte `resourceId` laut Projekt-Team-Mitgliedschaft
(`msdyn_projectteams`) Mitglied ist - sonst müsste man sich durch alle
Projekte im System wählen, auch die, an denen man gar nicht arbeitet.

```bash
psa discover myprojects   # testen, ob die Team-Mitgliedschaft gefunden wird
```

Funktioniert die Standard-Entität/die Standard-Felder in eurem Tenant nicht,
fällt das Tool automatisch (mit einmaliger Warnung) auf "alle Projekte"
zurück. Anpassen z.B. so:

```bash
psa config set mapping.myProjectsEntitySet <entity_set_name>
psa config set mapping.myProjectsResourceValueField <_feld_value>
psa config set mapping.myProjectsProjectValueField <_feld_value>
# oder ganz abschalten:
psa config set mapping.restrictToMyProjects false
```

## Benutzung

```bash
psa login                 # Device Code Login (einmalig / bei Tokenablauf)
psa pull                  # bestehende Einträge der aktuellen Woche aus Dataverse laden
psa pull 2024-06-03       # Einträge einer bestimmten Woche laden
psa pull --from 2024-06-01 --to 2024-06-30   # freier Zeitraum
psa add                   # Zeiteintrag interaktiv erfassen
psa week                  # aktuelle Woche anzeigen
psa week 2024-06-03       # Woche zu einem bestimmten Datum anzeigen
psa edit 3                # Eintrag #3 bearbeiten
psa remove 3               # Eintrag #3 zum Löschen vormerken
psa list                  # alle lokalen Einträge (inkl. Status)
psa sync --dry-run        # zeigen, was synchronisiert würde
psa sync                  # tatsächlich synchronisieren (Create/Update/Delete)
```

### Pull vs. Sync

- **`psa pull`**: Dataverse → lokal. Holt Einträge, die z.B. direkt im PSA-Web-Tool
  erfasst wurden, in die lokale SQLite-DB (gefiltert nach `resourceId` und
  Zeitraum). Bereits synchronisierte lokale Einträge werden mit dem Remote-Stand
  aktualisiert; remote gelöschte Einträge werden lokal entfernt. Lokale
  Einträge mit offenen Änderungen (`new`/`modified`/`deleted`) werden **nicht**
  überschrieben – stattdessen gibt es eine Konflikt-Warnung.
- **`psa sync`**: lokal → Dataverse. Überträgt offene lokale Änderungen nach
  Dataverse (Create/Update/Delete).

Empfohlener Ablauf: erst `psa pull` (aktuellen Stand holen), dann `psa add`/
`psa edit`, dann `psa sync`.

### Status-Werte pro Eintrag

- `new` – lokal erfasst, noch nie synchronisiert
- `modified` – bereits synchronisiert, danach lokal geändert
- `synced` – aktueller Stand ist in Dataverse vorhanden
- `deleted` – zum Löschen vorgemerkt (wird bei `sync` in Dataverse gelöscht
  und danach lokal entfernt)

Ein fehlgeschlagener Sync-Versuch ändert den Status **nicht** (bleibt z.B.
`new`/`modified`/`deleted`) - nur die Fehlermeldung wird zusätzlich in der
`error`-Spalte gespeichert und bei `psa week`/`psa list` rot angezeigt. So
wird beim nächsten `psa sync` automatisch erneut versucht, ohne dass der
Eintrag "hängen bleibt".

## Projektstruktur

```
src/psa_tool/
  cli.py              # Click-CLI, verdrahtet alle Befehle
  auth.py             # MSAL Device Code Flow + Token-Cache
  config.py           # ~/.psa-tool/config.json inkl. Feld-Mapping
  dataverse_client.py # Dünner Wrapper um die Dataverse Web API (requests)
  db.py               # SQLite-Persistenz für Zeiteinträge
  project_search.py   # Autocomplete-Suche für Projekte/Tasks
  commands/
    entry.py           # add/edit (interaktiv, questionary)
    week.py             # Wochenübersicht
    pull.py              # Dataverse -> lokal (bestehende Einträge laden)
    sync.py             # Create/Update/Delete gegen Dataverse
    discover.py          # Metadaten-Discovery (Entitäten/Felder/eigene Resource)
```

## Bekannte Einschränkungen / nächste Schritte

- Es wird aktuell nur eine grundlegende Feld-Zuordnung unterstützt (Datum,
  Dauer, Beschreibung, Projekt, Task, Resource). Weitere Pflichtfelder eurer
  Time-Entry-Konfiguration (z.B. Rolle/`msdyn_role`, Typ/`msdyn_type`) können
  bei Bedarf im Mapping und in `src/psa_tool/commands/sync.py`
  (`_build_body`) ergänzt werden.
- Konflikterkennung (z.B. wenn der Eintrag in der Zwischenzeit im Web-Tool
  geändert/gelöscht wurde) ist noch nicht implementiert – `sync` überschreibt
  im Zweifel den Remote-Stand bzw. meldet einen Fehler, falls der Datensatz
  nicht mehr existiert.
