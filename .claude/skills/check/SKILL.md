---
name: check
description: Reproduce en local los jobs de CI de infra-plot (lint, esquema al día, audit y opcionalmente E2E) y arregla lo que falle. Úsala antes de commitear, antes de abrir un PR, o cuando pidan "verificar", "pasar el lint", "que pase CI".
---

# Verificar como CI

CI (`.github/workflows/ci.yml`) tiene tres jobs: `lint`, `audit` y `e2e`. Reprodúcelos en este
orden y para en el primero que falle.

## 1. Lint + esquema (siempre, ~25 s)

```bash
devenv shell -- bash -c 'lint && gen-schema && git diff --exit-code -- schema web/src/model/generated.ts'
```

Si falla:

- **rustfmt / biome / taplo**: aplica el formato en vez de editar a mano:
  `devenv shell -- bash -c 'cargo fmt --all && pnpm exec biome check --write . && taplo fmt'`.
  Vuelve a revisar el diff: `biome check --write` también aplica fixes seguros del linter.
- **clippy**: está en `pedantic` con `-D warnings`. Arregla el código; no añadas `#[allow]`
  salvo que el lint sea claramente inadecuado y con un comentario del porqué.
- **tsc**: `pnpm -r run typecheck` cubre `web` y `e2e`. `generated.ts` no se edita a mano.
- **diff en `schema/` o `generated.ts`**: se cambió el modelo Rust sin regenerar. Commitea los
  ficheros regenerados junto con el cambio de Rust.
- **check-playwright-version**: ver skill `update-pins`.

## 2. Tests de Rust (si tocaste `crates/`)

```bash
devenv shell -- cargo test --workspace --locked
```

## 3. Audit (si tocaste dependencias: `Cargo.toml`, `package.json`, lockfiles)

```bash
devenv shell -- bash -c 'cargo deny check && pnpm audit --audit-level moderate'
```

La política está en `deny.toml`.

## 4. E2E (si tocaste `web/`, `crates/infraplot-server` o `e2e/`)

Pesado (~1 GB de navegadores la primera vez). Pregunta antes de lanzarlo si no está claro que
el usuario lo quiera; si no, indica que lo cubrirá CI. Detalles en la skill `e2e`.

```bash
devenv --profile e2e shell -- e2e
```

## Informe

Resume qué pasos se ejecutaron, cuáles pasaron y cuáles se saltaron (y por qué). Si algo
falla y no lo arreglas, pega la salida relevante.
