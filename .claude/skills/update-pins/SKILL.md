---
name: update-pins
description: Actualiza devenv.lock / nixpkgs y mantiene sincronizadas las versiones fijadas a nixpkgs (@playwright/test y @biomejs/biome), o gestiona PRs de Dependabot. Úsala cuando falle check-playwright-version, al pedir "actualizar dependencias/devenv/nixpkgs", o con un bump de Playwright/Biome.
---

# Actualizar dependencias fijadas a nixpkgs

Dos paquetes npm **deben** coincidir con nixpkgs y no los actualiza Dependabot:

| npm | nixpkgs | Por qué |
| --- | --- | --- |
| `@playwright/test` (`e2e/package.json`) | `playwright-driver` | Los navegadores vienen de nixpkgs; versión distinta = protocolo incompatible. Lo comprueba `check-playwright-version` (parte de `lint`). |
| `@biomejs/biome` (`package.json` raíz) | `biome` | El binario usado es el de nixpkgs (`BIOME_BINARY`); el paquete npm solo aporta API/esquema de configuración. |

Las demás dependencias (cargo, npm, actions) las actualiza Dependabot (`.github/dependabot.yml`).

## Actualizar devenv / nixpkgs

```bash
devenv update
devenv shell -- bash -c 'echo "playwright: $PLAYWRIGHT_NIX_VERSION"; biome --version'
```

Fija las versiones exactas (sin `^`) en los `package.json` y reinstala:

```bash
devenv shell -- bash -c 'pnpm install && lint'
```

Commitea juntos `devenv.lock`, los `package.json` y `pnpm-lock.yaml`. Si Biome sube de versión:
actualiza también la URL de `$schema` en `biome.json` si procede, y ejecuta
`pnpm exec biome migrate --write` si avisa de configuración obsoleta (p. ej. `recommended` →
`preset`); revisa el diff que aplique `biome check --write .`.

Si Rust stable nuevo trae lints de clippy nuevos, arréglalos en el mismo PR.

El E2E es el que realmente valida el nuevo Chromium: `devenv --profile e2e shell -- e2e`
(o deja que lo haga CI).

## PRs de Dependabot

`.github/dependabot.yml` ya ignora `@playwright/test` y `@biomejs/biome`; mantén esos
`ignore` si reorganizas el fichero. Los bumps agrupados de cargo/npm se validan con la skill
`check` (incluido `audit`); si tocan `three`/`@types/three`, `roughjs` o `vite`, prueba también
la app a mano (skill `run-app`) o el E2E, porque ahí es donde suelen romper.
