# Beitrags- und Agentenregeln

## Rust-Projekt

- Produktiver Rust-Code liegt in `src/`; CLI-Integrationstests liegen in
  `tests/`.
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

## Architektur: CQRS mit `medi-rs`

- Neue Anwendungsfälle als Commands oder Queries in einem thematischen Modul
  unter `src/commands/` implementieren. Das Modul enthält Nachrichtentyp,
  Handler und sein `medi_module!`-Manifest.
- `src/commands/mod.rs` komponiert die Manifeste zum gemeinsamen
  `AppMediator`. Gemeinsame, clonebare Abhängigkeiten gehören in
  `AppContext` und werden als Mediator-Resource injiziert.
- Handler enthalten die Anwendungslogik und rufen technische Module wie
  `db`, `auth` und `dataverse` auf. `src/main.rs` bleibt auf Clap-Parsing,
  die Abbildung auf Commands/Queries und die Darstellung der Ergebnisse
  beschränkt.
- Für externe oder persistente Abhängigkeiten bevorzugt Traits mit
  `Arc<dyn ... + Send + Sync>` als Resource verwenden, damit Handler mit
  Fakes getestet werden können. Das Command-Routing von `medi-rs` bleibt
  dabei statisch.

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
