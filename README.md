# PSA Tool

Rust-CLI zum Erfassen lokaler Zeiteinträge und zur Synchronisation mit Microsoft
Dynamics 365 Project Operations (Dataverse).

## Installation

### Direkt mit mise

Nach einem GitHub Release wird die Binary ohne Klonen des Repositories und ohne
Cargo global über mise installiert:

```bash
mise use -g github:klab365/psa-tool@latest
psa --help
```

Falls der Eintrag bereits in der globalen mise-Konfiguration steht, installiert
folgender Befehl die konfigurierte Version:

```bash
mise install github:klab365/psa-tool@latest
```

Danach die eigene Dataverse-Organisation konfigurieren und anmelden:

```bash
psa config set environmentUrl https://<eureorg>.crm4.dynamics.com
psa login
psa config set mapping.timezone Europe/Zurich # oder eure IANA-Zeitzone
psa config show
```

Danach können Einträge abgerufen und erfasst werden:

```bash
psa pull
psa add
psa week
psa sync --dry-run
```

### Entwicklung

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
