# infra-plot

Editor web de diagramas de infraestructura: vista 2D estilo sketch y vista 3D isométrica,
reproducibles desde JSON/TOML.

## Desarrollo

Requiere [devenv](https://devenv.sh).

```bash
devenv shell -- dev    # o `devenv up`
```

- Editor (Vite): http://localhost:31173 (escucha en todas las interfaces)
- API (Axum): http://127.0.0.1:31080, proxificada por Vite en `/api`

Otros comandos del shell: `lint`, `build`, `gen-schema`.

### Perfiles

El shell por defecto es mínimo para no llenar el disco:

| Perfil | Añade | Uso |
| --- | --- | --- |
| `e2e` | Chromium de Playwright (~1 GB) | `devenv --profile e2e shell -- e2e` (normalmente solo en CI) |
| `ide` | `rust-analyzer`, `rust-src`, `typescript-language-server` | `devenv --profile ide shell` |

Las cachés de cargo y pnpm viven en `.devenv/state/`, junto al repo.

## Uso del editor

### URLs

| URL | Efecto |
| --- | --- |
| `/d/<id>` | Abre el diagrama `<id>` guardado en el servidor |
| `?view=3d` | Arranca en la vista 3D |
| `?embed=1` | Solo el lienzo, sin barras ni paneles (para capturas o iframes); `#app[data-ready]` indica que ya está dibujado |
| `?src=<url>` | Carga un JSON/TOML desde esa URL (sujeto a CORS) |

### Atajos

| Tecla | Acción |
| --- | --- |
| `V` / `H` | Seleccionar / mover lienzo (o mantener `Espacio`) |
| `R` / `C` / `L` / `T` | Zona / conectar / línea libre / nota |
| `2` / `3` | Vista 2D / 3D |
| `F` | Encajar el diagrama |
| `Supr` | Borrar la selección (y sus edges) |
| `Ctrl+Z` / `Ctrl+Y` | Deshacer / rehacer |
| `Ctrl+D` | Duplicar |
| `Ctrl+S` / `Ctrl+O` | Guardar en el servidor / abrir (servidor y ejemplos) |
| `Esc` | Cancelar el borrador o deseleccionar |

Los ficheros `.json`/`.toml` se importan con «Import…» o arrastrándolos a la ventana. Los
exports SVG y PNG (2×) llevan la fuente Kalam incrustada, así que se ven igual sin conexión.
Los ejemplos de `examples/` aparecen en «Open…».

## CI

`.github/workflows/ci.yml` ejecuta en cada PR:

- `lint`: rustfmt, clippy, tsc, biome, taplo, la versión de Playwright, y comprueba que
  `schema/` y `generated.ts` están al día.
- `audit`: `cargo deny check` y `pnpm audit`.
- `e2e`: Playwright contra el binario release; si falla, sube el informe como artefacto.

`@playwright/test` y `@biomejs/biome` se fijan a las versiones de nixpkgs y se actualizan
junto con `devenv.lock`, no por Dependabot.
