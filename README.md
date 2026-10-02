# infra-plot

Editor web de diagramas de infraestructura: vista 2D (limpia o a mano alzada) y vista 3D
isométrica, reproducibles desde JSON/TOML. Temas claro, oscuro, Solarized y NieR. También
hay una [app de escritorio](#app-de-escritorio) nativa en Rust con gpui.

![Editor 2D con el ejemplo event-driven, tema claro y look clean](docs/screenshots/editor-2d.png)

![Vista 3D isométrica del ejemplo k8s-platform](docs/screenshots/view-3d.png)

| Tema oscuro | NieR, look sketch | Vista 3D en Solarized |
| --- | --- | --- |
| ![Editor 2D en tema oscuro](docs/screenshots/editor-2d-dark.png) | ![Look sketch con el tema NieR](docs/screenshots/editor-2d-nier.png) | ![Vista 3D con el tema Solarized](docs/screenshots/view-3d-solarized.png) |

## App de escritorio

`crates/infraplot-desktop` es un editor 2D nativo (Rust + [gpui](https://www.gpui.rs), el
framework de Zed) que abre y guarda los mismos ficheros JSON/TOML que el editor web. La
ventana no tiene decoración del sistema: dibuja su propia barra de título (arrastrar para
mover, doble clic para maximizar, botones de minimizar/maximizar/cerrar) y se redimensiona
desde los bordes. En X11 sin compositor gpui no puede hacer la ventana transparente y vuelve a
la decoración del sistema.

![App de escritorio con el ejemplo k8s-platform](docs/screenshots/desktop.png)

### Instalación

```bash
cargo install --locked --git https://github.com/aitorru/infra-plot infraplot-desktop
infra-plot examples/three-tier.toml    # o sin argumentos para un diagrama vacío
```

Mientras el código viva en `develop`, añade `--branch develop`. El binario se llama
`infra-plot` e incluye iconos, ejemplos y la fuente Inter. Necesita Rust ≥ 1.88 y, en Linux,
estas librerías (fontconfig y xkbcommon para compilar; Wayland o X11 y Vulkan al ejecutar):

| Distribución | Paquetes |
| --- | --- |
| Debian / Ubuntu | `pkg-config libfontconfig-dev libxkbcommon-x11-dev libwayland-dev libvulkan1 mesa-vulkan-drivers` |
| Fedora | `pkgconf fontconfig-devel libxkbcommon-x11-devel wayland-devel vulkan-loader mesa-vulkan-drivers` |
| Arch | `pkgconf fontconfig libxkbcommon-x11 wayland vulkan-icd-loader` (+ el driver Vulkan de tu GPU) |
| NixOS / devenv | `devenv shell -- desktop [fichero]` (el shell trae todo) |

`infra-plot --help` lista las opciones (`--theme light|dark|solarized|nier`; el tema elegido
se recuerda en `~/.config/infra-plot/settings.toml`).

### Uso

- Barra de título: New, Open…, Examples (los cuatro de `examples/`), Save, Save as…, deshacer
  y rehacer, y el tema. Guardar escribe TOML o JSON según la extensión del fichero.
- Paleta (izquierda): herramientas, tipos de zona y los 42 tipos de nodo con sus iconos.
  Clic en un tipo y luego en el lienzo para colocarlo.
- Lienzo: mismas interacciones que la vista 2D web — mover (las zonas arrastran su
  contenido; `Shift` las mueve solas), redimensionar zonas por las esquinas, conectar,
  líneas por arrastre o punto a punto (`Enter`/doble clic para terminar, `Shift` para tramos
  rectos), tiradores de codo y de tramos, ajuste a la rejilla (`Alt` lo desactiva).
  Rueda: desplazar; `Ctrl`+rueda: zoom. `Espacio` o botón central: mover el lienzo.
- Panel de propiedades (derecha): título y descripción del documento, o id, etiqueta,
  tipo, estilo, flechas, ruta, codo, tamaño y color del elemento seleccionado. El id se
  renombra con `Enter` y actualiza los edges.
- Atajos: los de la tabla de [Atajos](#atajos) (`V`/`H`/`R`/`C`/`L`/`T`, `F`, `Supr`,
  `Ctrl+Z`/`Ctrl+Y`, `Ctrl+D`, `Ctrl+S`, `Ctrl+O`, `Esc`), más `Ctrl+Shift+S` (guardar como),
  `Ctrl+N` (nuevo), `Ctrl+=`/`Ctrl+-`/`Ctrl+0` (zoom) y `Ctrl+Q` (salir; pregunta si hay
  cambios sin guardar).

Respecto al editor web, de momento no tiene vista 3D ni export SVG/PNG, y el look `sketch`
se dibuja como `clean` (se conserva en el fichero). La geometría y las operaciones de edición
están en `crates/infraplot-model` (`geometry.rs`, `ops.rs`, `catalog.rs`), espejo de
`web/src/model/geometry.ts` y `web/src/state/ops.ts`; los iconos se generan desde
`icons-clean.ts` con `gen-icons`.

## Desarrollo

Requiere [devenv](https://devenv.sh).

```bash
devenv shell -- dev    # o `devenv up`
```

- Editor (Vite): http://localhost:31173 (escucha en todas las interfaces)
- API (Axum): http://127.0.0.1:31080, proxificada por Vite en `/api`

Otros comandos del shell: `lint`, `build`, `gen-schema`, `gen-icons` (iconos de la app de
escritorio) y `desktop [fichero]` (la app de escritorio).

### Perfiles

El shell por defecto es mínimo para no llenar el disco:

| Perfil | Añade | Uso |
| --- | --- | --- |
| `e2e` | Chromium de Playwright (~1 GB) | `devenv --profile e2e shell -- e2e` (normalmente solo en CI) |
| `ide` | `rust-analyzer`, `rust-src`, `typescript-language-server` | `devenv --profile ide shell` |

Las cachés de cargo y pnpm viven en `.devenv/state/`, junto al repo.

## Formato del fichero

Un diagrama es un documento JSON o TOML con las mismas claves (el esquema completo está en
[`schema/diagram.schema.json`](schema/diagram.schema.json), generado desde
`crates/infraplot-model`). Las coordenadas son del lienzo 2D, con la `y` hacia abajo y
rejilla de 20.

```toml
version = 1                      # obligatorio, siempre 1
title = "Hello infra-plot"       # obligatorio
description = "Opcional"
look = "clean"                   # clean (por defecto, vectorial) · sketch (a mano alzada)

[[zones]]                        # rectángulos: x, y = esquina superior izquierda
id = "vpc"
kind = "vpc"                     # generic · region · vpc · subnet · dmz · availability-zone · account
                                 # security-group · k8s-cluster · namespace · on-prem · data-center
label = "VPC"
x = 120
y = 40
w = 520
h = 260
# style = "dashed"               # solid · dashed · dotted (por defecto, el del tipo)

[[nodes]]                        # x, y = centro del icono
id = "api"
kind = "service"                 # 42 tipos, ver abajo
label = "API"
x = 390
y = 170
meta = { port = "8080" }         # pares clave/valor libres (texto)

[[nodes]]
id = "db"
kind = "database"
x = 560
y = 170

[[edges]]                        # conexiones entre nodos o zonas
id = "e1"
from = "api"
to = "db"
label = "SQL"
style = "dashed"                 # solid (por defecto) · dashed · dotted
arrow = "end"                    # end (por defecto) · start · both · none
route = "orthogonal"             # straight (por defecto) · orthogonal
bend = 0.3                       # solo orthogonal: dónde cae el tramo central, de 0 (from) a 1 (to);
                                 # por defecto 0.5

[[lines]]                        # trazos libres, sin anclar a nada
id = "l1"
points = [[0, 0], [120, 0], [120, 80]]
arrow = "none"                   # por defecto none

[[notes]]                        # texto suelto; x, y = esquina superior izquierda
id = "n1"
x = 380
y = 330
text = "hello!"
size = "m"                       # s · m (por defecto) · l · xl
```

Tipos de nodo, por grupos (así aparecen en la paleta):

| Grupo | Tipos |
| --- | --- |
| Genérico | `component` |
| Cómputo | `service`, `server`, `vm`, `container`, `pod`, `k8s`, `function`, `worker`, `scheduler`, `gpu` |
| Red | `proxy`, `load-balancer`, `api-gateway`, `gateway`, `router`, `switch`, `vpn`, `cdn`, `dns` |
| Datos | `database`, `cache`, `queue`, `stream`, `storage`, `bucket`, `warehouse`, `search` |
| Seguridad | `firewall`, `identity`, `secrets` |
| Operación | `monitoring`, `logging`, `ci-cd`, `registry`, `notification` |
| Clientes | `user`, `client`, `mobile`, `browser`, `internet`, `external` |

Todos los elementos aceptan `color` (`#rgb` o `#rrggbb`); sin él se usa el del tipo. Además del
esquema, se comprueba que:

- los `id` son únicos en todo el documento;
- los `from`/`to` de los edges existen y no apuntan al mismo elemento;
- las zonas tienen ancho y alto positivos, las líneas al menos dos puntos y todas las
  coordenadas son números finitos;
- `bend` está entre 0 y 1.

El editor valida al importar o abrir, y el servidor al guardar: `PUT /api/diagrams/<id>`
(JSON, o TOML con `Content-Type: application/toml`) responde `422` con la lista de
problemas, y `POST /api/validate` solo valida. `GET /api/diagrams/<id>?format=toml`
devuelve el diagrama en TOML.

En [`examples/`](examples) hay cuatro ejemplos (`hello.json`, en `sketch`; `three-tier.toml`,
`k8s-platform.toml` y `event-driven.toml`, que usa los tipos genéricos y codos ajustados), que
también usan los tests E2E.

## Uso del editor

### URLs

| URL | Efecto |
| --- | --- |
| `/d/<id>` | Abre el diagrama `<id>` guardado en el servidor |
| `?view=3d` | Arranca en la vista 3D |
| `?theme=<tema>` | Fuerza un tema (`light`, `dark`, `solarized`, `nier`) sin guardarlo |
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
exports SVG y PNG (2×) usan los colores del tema activo y llevan incrustada la fuente del look
(Inter en `clean`, Kalam en `sketch`), así que se ven igual sin conexión.

### Temas, look y colores

- El selector de la barra superior cambia entre **Light**, **Dark**, **Solarized** y **NieR**;
  se recuerda en el navegador y por defecto sigue al sistema. Afecta a paneles, lienzo 2D,
  escena 3D y exports.
- El **look** es del documento (panel de propiedades sin nada seleccionado): `clean` dibuja
  iconos vectoriales en tiles tintados con Inter; `sketch`, el trazo a mano con rough.js y Kalam.
- Cualquier elemento acepta un color propio: muestras predefinidas, selector libre y `↺` para
  volver al del tipo.

### Líneas en ángulo recto

- Un edge con `route = "orthogonal"` seleccionado muestra un tirador en su tramo central:
  arrastrarlo desplaza el codo (se guarda en `bend`). También se ajusta con el control «bend»
  del panel de propiedades.
- Las líneas libres seleccionadas muestran tiradores en cada vértice y en el centro de cada
  tramo horizontal o vertical; arrastrar un tramo lo mueve en paralelo manteniendo los ángulos
  rectos. Al dibujar, `Shift` fuerza tramos horizontales o verticales.
- `Alt` desactiva el ajuste a la rejilla en todos los arrastres.
Los ejemplos de `examples/` aparecen en «Open…».

### Vista 3D

Vista isométrica con three.js, generada desde el mismo documento que la 2D:

- Cámara ortográfica isométrica. Arrastrar desplaza, botón derecho orbita (con límites),
  la rueda hace zoom; `Q`/`E` giran 90° y `F` encaja el diagrama.
- Las zonas son losas biseladas apiladas según su anidamiento, con borde (sólido, discontinuo
  o punteado según su estilo) y la etiqueta impresa encima.
- Cada tipo de nodo tiene su modelo: en `clean`, formas redondeadas con detalles (bahías,
  puertos, LEDs, pantallas), contorno fino y sombras suaves; en `sketch`, sombreado toon con
  contorno a mano alzada. Luces, fondo y etiquetas siguen el tema.
- Los edges son arcos (o tramos ortogonales, con el mismo `bend` que en 2D) con flechas; los paquetes animados se activan
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

Los tests E2E (Playwright, `e2e/tests/`) levantan el binario release sirviendo `web/dist`
con un directorio de datos temporal. En local necesitan el perfil `e2e`:

```bash
devenv --profile e2e shell -- e2e                            # toda la suite
devenv --profile e2e shell -- e2e editor                     # solo e2e/tests/editor.spec.ts
SCREENSHOTS=1 devenv --profile e2e shell -- e2e screenshots  # regenera docs/screenshots/
```

`@playwright/test` y `@biomejs/biome` se fijan a las versiones de nixpkgs y se actualizan
junto con `devenv.lock`, no por Dependabot.
