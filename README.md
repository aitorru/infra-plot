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
| `Q` / `E` | Vista 3D: girar 90° a izquierda / derecha |
| `P` | Animar paquetes por los edges (3D) |
| `Supr` | Borrar la selección (y sus edges) |
| `Ctrl+Z` / `Ctrl+Y` | Deshacer / rehacer |
| `Ctrl+D` | Duplicar |
| `Ctrl+S` / `Ctrl+O` | Guardar en el servidor / abrir (servidor y ejemplos) |
| `Esc` | Cancelar el borrador o deseleccionar |

Los ficheros `.json`/`.toml` se importan con «Import…» o arrastrándolos a la ventana. Los
exports SVG y PNG (2×) llevan la fuente Kalam incrustada, así que se ven igual sin conexión.
Los ejemplos de `examples/` aparecen en «Open…».

### Vista 3D

Vista isométrica con three.js, generada desde el mismo documento que la 2D:

- Cámara ortográfica isométrica. Arrastrar desplaza, botón derecho orbita (con límites),
  la rueda hace zoom; `Q`/`E` giran 90° y `F` encaja el diagrama.
- Las zonas son losas apiladas según su anidamiento, con borde de tinta (sólido, discontinuo
  o punteado según su estilo) y la etiqueta impresa encima.
- Cada tipo de nodo tiene su modelo low-poly (rack, cilindros de BBDD, heptágono de K8s,
  nube…) con contorno a mano alzada. Las etiquetas usan Kalam.
- Los edges son arcos (o tramos ortogonales) con flechas; los paquetes animados se activan
  o desactivan con `P` y arrancan apagados si el sistema pide movimiento reducido.
- Clic selecciona; arrastrar un nodo o una zona lo mueve sobre el suelo (con su contenido y
  ajustado a la rejilla; `Alt` desactiva el ajuste). También se pueden colocar nodos y
  conectar. Zonas, líneas y notas se dibujan en 2D.
- «PNG» en 3D exporta una foto de la vista actual a 3× (máximo 4096 px).
- Para tests y capturas, `[data-testid=view-3d]` expone `data-nodes`, `data-zones`,
  `data-edges`, `data-meshes`, `data-azimuth`, `data-zoom`…

## CI

`.github/workflows/ci.yml` ejecuta en cada PR:

- `lint`: rustfmt, clippy, tsc, biome, taplo, la versión de Playwright, y comprueba que
  `schema/` y `generated.ts` están al día.
- `audit`: `cargo deny check` y `pnpm audit`.
- `e2e`: Playwright contra el binario release; si falla, sube el informe como artefacto.

`@playwright/test` y `@biomejs/biome` se fijan a las versiones de nixpkgs y se actualizan
junto con `devenv.lock`, no por Dependabot.
