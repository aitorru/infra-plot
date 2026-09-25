---
name: run-app
description: Arranca infra-plot (servidor Axum + Vite) y compruébalo de verdad: API con curl, editor en el navegador de vista previa. Úsala para "arrancar", "levantar", "probar en la app", hacer capturas o confirmar un cambio visual en 2D/3D.
---

# Arrancar y probar infra-plot

## Modo desarrollo (recarga en caliente)

Lánzalo en segundo plano (Bash con `run_in_background: true`), nunca en primer plano:

```bash
devenv shell -- dev
```

- Antes de lanzarlo, mira si ya hay uno corriendo (`ss -ltn | grep -E ':(31173|31080)\b'`):
  `--strictPort` hace que un segundo `dev` falle. Si está, reutilízalo.
- Vite: http://localhost:31173 (sirve el editor y proxifica `/api`). Escucha en `0.0.0.0` y el
  rango 30000–39999 está abierto, así que el humano puede abrirlo en `http://pi-home:31173`
  por Tailscale.
- API: http://127.0.0.1:31080 (cargo-watch recompila al tocar `crates/`).
- Primera vez: compila el servidor; espera a que responda antes de probar:

```bash
until curl -sf http://127.0.0.1:31080/api/health; do sleep 2; done
```

Evita `devenv up`: abre la TUI de process-compose, poco útil sin terminal interactiva.

Para pararlo, detén la tarea en segundo plano. Si queda algo escuchando:
`ss -ltnp | grep -E '3117[3]|31080'`.

## Modo release (lo que prueba E2E)

```bash
devenv shell -- bash -c 'build && INFRAPLOT_DATA_DIR=$(mktemp -d) target/release/infraplot-server'
```

Sirve `web/dist` y la API en `127.0.0.1:31080`, con un directorio de datos desechable.

## Probar la API

```bash
curl -s localhost:31080/api/health
curl -s -X POST localhost:31080/api/validate -H 'content-type: application/toml' --data-binary @examples/three-tier.toml
curl -s -X PUT localhost:31080/api/diagrams/demo -H 'content-type: application/toml' --data-binary @examples/k8s-platform.toml
curl -s 'localhost:31080/api/diagrams/demo?format=toml'
curl -s localhost:31080/api/diagrams
```

Un diagrama inválido devuelve `422` con `issues: [{path, message}]`. Los ids de la URL son
`[a-z0-9_-]{1,64}`. En dev, los diagramas se guardan en `.devenv/state/data`.

## Probar el editor

Usa las herramientas de vista previa del navegador si están disponibles. URLs útiles:

| URL | Para qué |
| --- | --- |
| `http://localhost:31173/` | Editor vacío (o el autosave de `localStorage["infraplot:autosave"]`) |
| `/d/demo` | Abre el diagrama `demo` del servidor |
| `/?view=3d` | Arranca en 3D |
| `/?embed=1` | Solo el lienzo; espera a `#app[data-ready="true"]` antes de capturar |

Los ejemplos (`examples/*.toml|json`) se abren desde «Open…» (se empaquetan con
`import.meta.glob`). Atajos: `2`/`3` cambian de vista, `F` encaja, `Q`/`E` giran en 3D.

Selectores estables: `[data-testid=canvas-2d]` (SVG; `.layer-nodes [data-id=<id>]`),
`[data-testid=canvas-3d]`, `[data-testid=view-3d]` (expone `data-nodes`, `data-zones`,
`data-edges`, `data-meshes`, `data-azimuth`, `data-zoom`), `palette`, `props`, `library`,
`examples`, `file-input`, `toolbar-3d`.

El 3D necesita WebGL; en un Chromium headless sin GPU puede marcar
`data-webgl="unavailable"` (Playwright lo resuelve con SwiftShader, ver `e2e/playwright.config.ts`).
