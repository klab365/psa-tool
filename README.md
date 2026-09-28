# PSA Tool

Rust-CLI zum Erfassen lokaler Zeiteinträge und zur Synchronisation mit Microsoft
Dynamics 365 Project Operations (Dataverse).

## Entwicklung

```bash
mise trust
mise install
cargo run -- --help
```

Die Binary heißt `psa`:

```bash
cargo run -- config set environmentUrl https://<eureorg>.crm4.dynamics.com
cargo run -- login
cargo run -- week
cargo run -- sync --dry-run
```

Für einen Release-Build:

```bash
cargo build --release
./target/release/psa --help
```

Lokale Nutzerdaten bleiben kompatibel unter `~/.psa-tool/`:

- `config.json` – Konfiguration
- `psa.sqlite3` – Zeiteinträge
- `token_cache.json` – Rust OAuth-Token-Cache (Dateirechte `0600`)

## Qualitätssicherung

Jeder Pull Request führt die Rust-Prüfung aus. Lokal entspricht das:

```bash
mise run check
```

Dies prüft Formatierung, Clippy, Tests, Release-Build und Whitespace im Diff.

## Archiv

Die abgelöste Python-Implementierung inklusive ihrer Abhängigkeiten liegt in
[`_archive/python/`](_archive/python/). Der ursprüngliche Migrationsplan liegt
in [`_archive/docs/work/rust-migration-plan.md`](_archive/docs/work/rust-migration-plan.md).
