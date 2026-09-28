# Plan: Migration des PSA-Tools nach Rust

> **Aktualisierung:** Die Rust-Quellen liegen inzwischen in `src/`; die in den
> folgenden ursprünglichen Schritten erwähnte Übergangsbezeichnung `src-rust/`
> ist daher als `src/` zu lesen. Die abgelöste Python-Implementierung liegt in
> `_archive/python/`.

## Ziel
Das PSA-Tool steht als Rust-CLI zur Verfügung und verhält sich für bestehende Nutzer kompatibel: dieselben Befehle, Konfigurationsdatei, lokale SQLite-Datenbank und Dataverse-Synchronisationssemantik.

## Ansatz
Die Rust-Implementierung wird parallel zum bestehenden Python-Programm aufgebaut und erst nach einer Verhaltensvergleichs- und Migrationsphase zur Standardimplementierung. Damit bleiben lokale Daten unter `~/.psa-tool/` erhalten und OAuth-, Dataverse- und interaktive UI-Risiken sind isoliert. Ein Big-Bang-Ersatz ohne Tests wird ausdrücklich vermieden.

## Schritte

1. **Vertrag der bestehenden CLI festschreiben** — Befehle, Optionen, Exit-Codes, Ausgaben und Fehlerfälle als automatisierte Smoke-/Integrationstests dokumentieren.
   - Dateien: neu `tests/`, `README.md`
   - Abdecken: `login`, `logout`, `config`, `add`, `edit`, `remove`, `week`, `list`, `pull`, `sync --dry-run` sowie alle `discover`-Unterbefehle.
   - Hinweis: Dataverse- und Entra-HTTP-Aufrufe über Fixture-/Mock-Server testen; keine realen Zugangsdaten in Tests.

2. **Rust-Projekt und Build-Integration anlegen** — Ein Cargo-Paket neben dem Python-Paket einführen und die Prüfung in lokaler Entwicklung und CI ergänzen.
   - Dateien: neu `Cargo.toml`, `src-rust/`; ändern `mise.toml`, `.github/workflows/ci.yml`, `README.md`
   - Abhängigkeiten voraussichtlich: `clap`, `tokio`, `reqwest` (Rustls), `serde`, `serde_json`, `rusqlite`, `chrono`, `chrono-tz`, `directories`, `thiserror` und ein Terminal-UI-Stack.
   - Entscheidung: Abhängigkeiten vor Einführung auf Lizenz, Wartung und plattformübergreifende Binary-Unterstützung prüfen.

3. **Kompatible Konfiguration und Pfade implementieren** — `~/.psa-tool/config.json` einschließlich Deep-Merge mit den Standardwerten, Mapping und restriktiven Dateirechten in Rust lesen und schreiben.
   - Referenzdateien: `src/psa_tool/config.py`, `src/psa_tool/paths.py`
   - Rust-Module: neu `src-rust/config.rs`, `src-rust/paths.rs`
   - Wichtig: Bestehende JSON-Schlüssel und Standardwerte unverändert lassen; die CLI speichert derzeit auch Werte als Strings, was kompatibel behandelt werden muss.

4. **SQLite-Kompatibilität herstellen** — Das vorhandene Schema öffnen, Migrationen idempotent anwenden und die lokalen Statusübergänge implementieren.
   - Referenzdatei: `src/psa_tool/db.py`
   - Rust-Module: neu `src-rust/db.rs`, `src-rust/model.rs`
   - Abdecken: WAL-Modus, Tabelle `time_entries`, Spalte `entry_status`, UTC-Zeitstempel sowie die Status `new`, `modified`, `synced`, `deleted`.
   - Risiko: Die bestehende Datenbank ist Nutzerdatenbestand; Tests müssen mit einer von Python erzeugten Fixture-Datenbank Lesbarkeit und Schreibkompatibilität nachweisen.

5. **Dataverse-Client implementieren** — Einen typisierten HTTP-Client mit identischen OData-Headern, Fehlern und Create/Read/Update/Delete-Verhalten erstellen.
   - Referenzdatei: `src/psa_tool/dataverse_client.py`
   - Rust-Module: neu `src-rust/dataverse.rs`
   - Abdecken: API-Basis `/api/data/v9.2`, `Prefer`-Header für Repräsentationen und Annotations, JSON-Fehlertexte sowie die tolerierte 404-Antwort beim Löschen.

6. **Entra Device-Code-Authentifizierung entscheiden und umsetzen** — Device-Code-Login, Token-Refresh, Logout und den lokalen Cache implementieren.
   - Referenzdatei: `src/psa_tool/auth.py`
   - Rust-Module: neu `src-rust/auth.rs`
   - Risiko: Das MSAL-Python-Cacheformat ist nicht automatisch mit Rust kompatibel. Entweder wird es bewusst gelesen/geschrieben und mit einem vorhandenen Python-Cache getestet, oder ein versionierter Rust-Cache wird eingeführt und beim ersten Start sicher migriert. Diese Entscheidung vor der Implementierung treffen.
   - Sicherheit: Cache-Dateien mit `0600` anlegen, keine Tokens in Ausgaben, Logs oder Tests schreiben.

7. **Nicht-interaktive Fachlogik portieren** — Pull, Sync, Wochenansicht und Discovery als von der Ausgabe getrennte Services implementieren.
   - Referenzdateien: `src/psa_tool/commands/sync.py`, `src/psa_tool/commands/pull.py`, `src/psa_tool/commands/week.py`, `src/psa_tool/commands/discover.py`, `src/psa_tool/project_search.py`
   - Rust-Module: neu `src-rust/services/{sync,pull,week,discover,search}.rs`
   - Abdecken: Zeitzonen-Umrechnung beim Pull, Konflikte bei offenen lokalen Änderungen, Dauer in Minuten/Stunden, konfigurierbares Feld-Mapping, sichere Projektfilterung und OData-Escaping.

8. **Interaktive Erfassung portieren** — `add` und `edit` mit Projekt-/Task-Autocomplete und denselben Validierungen implementieren.
   - Referenzdatei: `src/psa_tool/commands/entry.py`
   - Rust-Module: neu `src-rust/ui.rs`, `src-rust/commands/entry.rs`
   - Risiko: Interaktive Completion ist der UX-kritischste Teil. Zuerst einen Prototyp gegen einen Mock-Dataverse-Server bauen und Abbruch, leere Treffer, Begrenzung auf 50 Ergebnisse und Netzwerkfehler testen.

9. **CLI und Ausgabekompatibilität zusammensetzen** — Die Befehlsstruktur mit `clap` abbilden, Fehlermeldungen zentral behandeln und die automatische `resourceId`-Ermittlung nach dem Login ergänzen.
   - Referenzdatei: `src/psa_tool/cli.py`
   - Rust-Module: neu `src-rust/main.rs`, `src-rust/commands/*.rs`
   - Abdecken: Deutsche Hilfetexte, `--from`, `--to`, `--dry-run`, Erfolg-/Fehler-Exit-Codes und die vorhandenen `discover`-Unterbefehle.

10. **Paritäts- und Upgrade-Test durchführen** — Python- und Rust-Binary gegen dieselben HTTP-Fixtures und SQLite-Fixtures ausführen und Resultate vergleichen.
    - Dateien: neu `tests/fixtures/`, `tests/compatibility/`, ggf. `docs/work/rust-migration-plan.md`
    - Prüfen: Konfiguration, vorhandene Datenbank, Pull-/Sync-Requests und -Statusänderungen, Zeitzonen, Fehlerfälle und Terminal-Interaktionen.

11. **Distribution umstellen und Python entfernen** — Erst nach erfolgreicher Parität die Rust-Binary veröffentlichen, Installation und Release-Automatisierung umstellen und den Python-Code in einer separaten Breaking-Change-Änderung entfernen.
    - Dateien: ändern `README.md`, `mise.toml`, `.github/workflows/*`, Release-Konfiguration; entfernen später `pyproject.toml`, `uv.lock`, `src/psa_tool/`
    - Hinweis: Die Entfernung erfordert einen Major-Release, da Installationsweg und Artefakt sich ändern.

## Abhängigkeiten zwischen den Schritten
- Schritt 1 ist Voraussetzung für die Paritätsprüfung und sollte vor dem Port beginnen.
- Schritte 3 bis 6 bilden die Grundlage für Schritte 7 bis 9.
- Schritt 8 hängt zusätzlich von Suche und Dataverse-Client aus Schritt 5 und 7 ab.
- Schritt 11 erfolgt erst nach Schritt 10 und sollte nicht mit der initialen Rust-Portierung vermischt werden.

## Nicht enthalten
- Neue Dataverse-Felder oder erweiterte Feldzuordnungen.
- Eine neue Konfliktauflösung über die heutige Warnung bei Pull-Konflikten hinaus.
- Änderungen am Datenmodell oder am Speicherort der Nutzerdaten.
- Gleichzeitige Synchronisierung oder ein grafisches UI.

## Offene Fragen
- Muss ein bereits vorhandener Python-MSAL-Token-Cache ohne erneuten Login weiterverwendbar sein, oder ist ein einmaliger Device-Code-Login akzeptabel?
- Soll die veröffentlichte Rust-Binary `psa` heißen und die Python-Installation unmittelbar ersetzen, oder ist eine Übergangsphase mit einem zweiten Namen erforderlich?
- Welche Zielplattformen müssen Release-Artefakte abdecken (macOS arm64/x86_64, Linux, Windows)?

## Geschätzte Komplexität
**Groß.** Der Codeumfang ist überschaubar, aber persistente Nutzerdaten, OAuth-Token-Caching, Dataverse-spezifische OData-Details und die interaktive Autocomplete-Oberfläche verlangen eine abgesicherte Verhaltensparität.
