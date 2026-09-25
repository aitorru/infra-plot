---
name: devenv
description: Cómo ejecutar comandos en infra-plot (cargo, pnpm, biome, taplo, playwright) a través de devenv/Nix. Úsala antes de lanzar cualquier herramienta del repo, cuando un comando "no se encuentra" o falla por el entorno, o al tocar devenv.nix / devenv.yaml / perfiles / variables de entorno.
---

# Entorno devenv de infra-plot

Todo el toolchain (Rust stable, Node 24, pnpm, biome, taplo, cargo-deny, cargo-watch, jq)
lo aporta `devenv.nix`. La máquina es NixOS: **no hay cargo/node/pnpm en el PATH global** y
los binarios nativos que baja npm (p. ej. `@biomejs/biome`) no arrancan.

## Regla principal

Ejecuta siempre dentro del shell de devenv, de forma no interactiva:

```bash
devenv shell -- <comando>                 # p. ej. devenv shell -- cargo test -p infraplot-model
devenv shell -- bash -c 'cmd1 && cmd2'    # varios comandos en una sola entrada (más rápido)
```

- Entrar al shell cuesta unos segundos: agrupa comandos con `bash -c` en vez de hacer muchas llamadas.
- No uses `npx biome`/`pnpm exec biome` fuera del shell: `BIOME_BINARY` apunta al biome de nixpkgs.
- No hagas `pnpm install` a mano salvo que cambies dependencias: el shell lo ejecuta al entrar
  (`pnpm.install.enable`). Si cambias `package.json`, usa `pnpm install` dentro del shell y
  commitea `pnpm-lock.yaml`.

## Scripts del shell (definidos en `devenv.nix`)

| Script | Qué hace |
| --- | --- |
| `lint` | `cargo fmt --check`, clippy `-D warnings` (pedantic), `tsc` en web y e2e, `biome ci`, `taplo fmt --check`, `check-playwright-version` |
| `build` | `vite build` de `web/` + `cargo build --release -p infraplot-server` |
| `gen-schema` | Regenera `schema/diagram.schema.json` desde Rust y `web/src/model/generated.ts` |
| `dev` | cargo-watch del servidor + Vite (ver skill `run-app`) |
| `e2e` | `build` + Playwright; solo con el perfil `e2e` (ver skill `e2e`) |
| `check-playwright-version` | `@playwright/test` debe coincidir con `playwright-driver` de nixpkgs |

`enterTest` = `lint`, así que `devenv test` equivale a pasar el lint.

## Perfiles

- Por defecto: mínimo, sin navegadores ni LSP (el disco va justo).
- `--profile e2e`: Chromium de Playwright desde nixpkgs (~1 GB) + fontconfig. Solo para E2E.
- `--profile ide`: rust-analyzer, rust-src y LSP de TypeScript.

## Variables y rutas útiles

- Puertos: Vite `31173`, API `31080` (`INFRAPLOT_BIND=127.0.0.1:31080`), E2E `31090`.
- `INFRAPLOT_DATA_DIR=.devenv/state/data` (diagramas guardados en dev).
- `INFRAPLOT_STATIC_DIR=web/dist`.
- `CARGO_HOME` y el store de pnpm viven en `.devenv/state/` (no en `$HOME`, que es una SD).
- `RUST_LOG=infraplot=debug,tower_http=info`.

## Al modificar `devenv.nix`

- Mantén los comentarios que explican el porqué (disco, NixOS, versiones fijadas).
- Los hooks de git (rustfmt, taplo, biome `--write`) vienen de `git-hooks.hooks`;
  `.pre-commit-config.yaml` es generado y está en `.gitignore`.
- Si añades un script, documéntalo en `enterShell` y en el README («Otros comandos del shell»).
- CI usa `.github/actions/devenv` (Nix + cachix + caché de `.devenv/state` y `target`), así que
  cualquier cosa que funcione con `devenv shell -- …` en local funciona igual en CI.
