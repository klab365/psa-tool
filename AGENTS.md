# Beitrags- und Agentenregeln

## Abhängigkeiten und Versionen

- Jede im Quellcode direkt importierte Drittanbieterbibliothek muss als direkte
  Abhängigkeit in `pyproject.toml` stehen. Transitive Abhängigkeiten sind keine
  stabile API-Garantie.
- Nach Änderungen an Abhängigkeiten `uv lock` ausführen und `uv.lock` committen.
- Die Paketversion ist die einzige Quelle in `pyproject.toml`.
- Versionen werden ausschließlich über Release Please erhöht. In Feature- und
  Bugfix-PRs weder `project.version` noch `.release-please-manifest.json`
  manuell ändern.
- Commit- und Squash-Merge-Titel folgen Conventional Commits, damit Release
  Please die korrekte SemVer-Version ermittelt:
  - `fix:` erhöht Patch.
  - `feat:` erhöht Minor.
  - `feat!:` oder ein `BREAKING CHANGE:`-Footer erhöht Major.

## Vor dem Commit

Bei jeder Änderung muss die vollständige Prüfung erfolgreich sein:

```bash
mise run check
```

Der Task prüft Lockfile-Konsistenz, kompiliert den Quellcode, baut Source-
Distribution und Wheel und prüft den Git-Diff auf Leerraumfehler.

## Pull Requests

- Änderungen in einem thematischen Branch umsetzen.
- Commit-Nachrichten kurz und aussagekräftig formulieren.
- Pull Requests nennen die ausgeführten Prüfungen und relevante
  Konfigurations-/Versionsänderungen.
