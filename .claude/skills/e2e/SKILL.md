---
name: e2e
description: Ejecuta, escribe o depura los tests Playwright de infra-plot (e2e/tests) y regenera las capturas del README. Úsala al añadir tests E2E, cuando falla el job e2e de CI, o al pedir "actualiza las capturas".
---

# Tests E2E (Playwright)

## Ejecutar

Solo funcionan con el perfil `e2e` (Chromium de nixpkgs, ~1 GB; el script aborta sin él):

```bash
devenv --profile e2e shell -- e2e                  # toda la suite (hace `build` antes)
devenv --profile e2e shell -- e2e editor           # filtra por fichero: e2e/tests/editor.spec.ts
devenv --profile e2e shell -- e2e -g "undo/redo"   # filtra por título
SCREENSHOTS=1 devenv --profile e2e shell -- e2e screenshots   # regenera docs/screenshots/*.png
```

- Los tests corren contra el **binario release** (`target/release/infraplot-server`) sirviendo
  `web/dist` en `127.0.0.1:31090` con un directorio de datos temporal. Si cambias web o
  servidor, el `build` del script los recompila.
- Localmente el reporter es `list`; con fallos quedan trazas en `e2e/test-results/`
  (`trace: retain-on-failure`): ábrelas con `pnpm --filter e2e exec playwright show-trace <zip>`.
- En CI, si falla, el informe HTML se sube como artefacto `playwright-report`
  (`gh run download <run-id> -n playwright-report`).

## Escribir tests

Ficheros por área: `smoke` (API + carga), `editor` (edición 2D, import/export, servidor),
`ui` (paleta, props, menús, exports), `view3d` (vista 3D), `screenshots` (solo con `SCREENSHOTS=1`).

Reutiliza los helpers de `e2e/tests/editor.spec.ts` antes de inventar otros:

- `ready(page, url)` → `goto` + espera `#app[data-ready="true"]`.
- `importFile(page, path)` → `getByTestId("file-input").setInputFiles`.
- `toScreen`, `clickAt`, `dragWorld` → trabajan en coordenadas de mundo leyendo el
  `transform` del `.viewport` del SVG; no uses píxeles de pantalla fijos.
- `autosaved(page)` → el documento actual desde `localStorage["infraplot:autosave"]`;
  es la forma preferida de afirmar el estado del modelo.

Para 3D, afirma sobre los `data-*` de `[data-testid=view-3d]` (`data-nodes`, `data-meshes`,
`data-azimuth`, `data-zoom`, `data-rendered`…) en lugar de comparar píxeles.

Para cargar un ejemplo sin servidor: `page.route` sirviendo el fichero de `examples/` y
`?src=/fixtures/<fichero>` (ver `screenshots.spec.ts`).

Convenciones: títulos en inglés describiendo el comportamiento, `fullyParallel` (los tests no
deben compartir estado; usa ids de diagrama únicos al guardar en el servidor), sin `test.only`
(CI tiene `forbidOnly`). Comprueba tipos con `devenv shell -- pnpm --filter e2e run typecheck`.

## Si un selector no existe

Añade un `data-testid` en `web/src/ui/*` (helper `h(...)` de `ui/dom.ts`) en lugar de usar
selectores CSS frágiles.
