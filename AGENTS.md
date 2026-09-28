# Beitrags- und Agentenregeln

## Rust-Projekt

- Produktiver Rust-Code liegt in `src/`; CLI-Integrationstests liegen in
  `tests/`. Die Python-Implementierung unter `_archive/python/` ist nur ein
  Archiv und wird nicht erweitert.
- Jede direkt importierte Drittanbieterbibliothek gehört als direkte
  Abhängigkeit in `Cargo.toml`. Testbibliotheken gehören nach
  `[dev-dependencies]`.
- Nach Änderungen an Abhängigkeiten `Cargo.lock` aktualisieren und committen.
- `Cargo.toml` ist die einzige Quelle der Paketversion. Versionen werden nur
  durch Release Please geändert; weder `package.version` noch
  `.release-please-manifest.json` in Feature- oder Bugfix-PRs manuell ändern.
- Formatierung und Lints benötigen die Rustup-Komponenten `rustfmt` und
  `clippy`. Die CI installiert sie explizit für den in `mise.toml` definierten
  Rust-Toolchain.

## Vor dem Commit

Bei jeder Änderung muss die vollständige Prüfung erfolgreich sein:

```bash
mise run check
```

Der Task prüft Rust-Formatierung, Clippy mit allen Targets, Tests,
Release-Build und Git-Diff auf Leerraumfehler.

## Pull Requests

- Änderungen in einem thematischen Branch umsetzen.
- Commit- und Squash-Merge-Titel folgen Conventional Commits:
  - `fix:` erhöht Patch.
  - `feat:` erhöht Minor.
  - `feat!:` oder ein `BREAKING CHANGE:`-Footer erhöht Major.
- Pull Requests nennen die ausgeführten Prüfungen sowie relevante
  Abhängigkeits- und Konfigurationsänderungen.
