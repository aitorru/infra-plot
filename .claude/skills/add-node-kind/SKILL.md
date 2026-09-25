---
name: add-node-kind
description: Añade (o renombra/elimina) un tipo de nodo o de zona en infra-plot de punta a punta — enum Rust, esquema, tipos TS, catálogo, glifo 2D, modelo 3D, README y tests. Úsala cuando pidan un nuevo icono/tipo como "kafka", "bucket", "vpn", "gateway" o una nueva clase de zona.
---

# Nuevo tipo de nodo / zona

El tipo vive en el modelo Rust y se propaga al resto. TypeScript obliga a cubrirlo en los
`Record<NodeKind, …>`, pero **no** en todos los `switch`: sigue la lista entera.

## Nodo (`NodeKind`)

1. **Rust** — `crates/infraplot-model/src/lib.rs`, enum `NodeKind` (serde `kebab-case`:
   `LoadBalancer` → `"load-balancer"`). Colócalo junto a los de su grupo.
2. **Regenerar** — `devenv shell -- gen-schema` (actualiza `schema/diagram.schema.json` y
   `web/src/model/generated.ts`; ambos se commitean, nunca se editan a mano).
3. **Catálogo** — `web/src/model/catalog.ts`, `NODE_KINDS`: `label`, `color` (pastel en la
   línea de los existentes, estilo Open Color), `group` (`compute | network | data | other`,
   decide la sección de la paleta) y `height` (altura del modelo 3D en unidades de mundo).
4. **Glifo 2D** — `web/src/render2d/glyphs.ts`: nuevo `case` dibujado con roughjs dentro de la
   caja `s × s` (usa `x0`, `y0`, `cx`, `cy`, la unidad `u` y los estilos `base`/`line`).
   Mira los `case` vecinos como referencia de proporciones.
5. **Modelo 3D** — `web/src/render3d/models.ts`: nuevo `case` que devuelva las `Part[]`
   (primitivas `box`, `prism`, `ball`…; `color: "dark"` para detalles). Low-poly, que se
   reconozca en isométrico.
6. **README** — lista «Tipos de nodo» (y el recuento «20 tipos» del bloque TOML).
7. **Ejemplo (opcional)** — úsalo en algún `examples/*.toml` si aporta.

## Zona (`ZoneKind`)

1. Enum `ZoneKind` en `lib.rs` → `gen-schema`.
2. `ZONE_KINDS` en `catalog.ts`: `label`, `color`, `style` por defecto (`solid|dashed|dotted`).
3. Busca `ZONE_KINDS` y `ZoneKind` en `web/src/render2d` y `web/src/render3d` por si algún
   tipo tiene tratamiento especial.
4. README: lista de `kind` en el bloque `[[zones]]`.

## Renombrar o eliminar

Es un cambio incompatible del formato: los diagramas guardados con el nombre viejo dejan de
validar (`deny_unknown_fields` + enum). Pregunta antes; si procede, añade un
`#[serde(alias = "…")]` para seguir aceptando el nombre antiguo y actualiza `examples/`.

## Verificar

```bash
devenv shell -- bash -c 'lint && cargo test --workspace'
```

Luego mira el resultado: arranca la app (skill `run-app`) y comprueba el icono en la paleta,
en el lienzo 2D y en la vista 3D. El test E2E `palette shows rough icons for every kind`
(`e2e/tests/ui.spec.ts`) cubre la paleta automáticamente en CI.
