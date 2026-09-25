---
name: change-model
description: Cambia el formato de documento de infra-plot (campos o elementos nuevos en Diagram/Zone/Node/Edge/Line/Note, nuevas reglas de validación, cambios en la API /api). Úsala para cualquier cambio en crates/infraplot-model o en lo que el editor guarda/carga. Para nuevos tipos de nodo o zona usa add-node-kind.
---

# Cambiar el modelo de documento

Fuente de verdad: `crates/infraplot-model/src/lib.rs`. El esquema JSON y los tipos TS se
**generan** a partir de él; el cliente valida con Ajv contra ese esquema y el servidor con
serde + `Diagram::validate()`.

```
lib.rs ──export-schema──▶ schema/diagram.schema.json ──json2ts──▶ web/src/model/generated.ts
                                     │                                   │
                                     └──▶ Ajv (web/src/model/doc.ts) ◀───┘
```

## Pasos

1. **Rust** (`lib.rs`):
   - Campos opcionales con `#[serde(default, skip_serializing_if = …)]` para que los ficheros
     existentes sigan siendo válidos y los guardados sigan siendo pequeños.
   - Doc comments `///` en cada campo: acaban como `description` en el esquema.
   - Enums con `#[serde(rename_all = "kebab-case")]`. Structs con `deny_unknown_fields`.
   - Un campo obligatorio nuevo, o cambiar el significado de uno, rompe el formato: pregunta
     antes y valora subir `FORMAT_VERSION`.
2. **Validación semántica**: lo que JSON Schema no puede expresar (referencias entre ids,
   números finitos, colores…) va en `Diagram::validate()` usando el `Validator`
   (`push(path, msg)` con rutas tipo `edges[2].to`). Añade tests `#[cfg(test)]` en el crate
   para cada regla nueva (el crate aún no tiene; crea el módulo si hace falta).
3. **Regenerar**: `devenv shell -- gen-schema`. Commitea `schema/` y `generated.ts`.
4. **Cliente**:
   - `web/src/model/doc.ts`: `Doc`, `normalize`, `prune` (si el campo es colección u opcional).
   - `web/src/state/ops.ts`: añadir/mover/duplicar/renombrar si afecta a elementos o ids
     (p. ej. un nuevo campo que referencia ids debe actualizarse en `renameId` y `deleteElement`).
   - `web/src/ui/props.ts`: control en el panel de propiedades.
   - Renderizado: `web/src/render2d/canvas2d.ts` y `web/src/render3d/scene3d.ts`.
   - Export SVG/PNG en `web/src/ui/export.ts` si cambia lo que se dibuja.
5. **Servidor** (`crates/infraplot-server/src/api.rs`) solo si cambia la API. Errores vía
   `ApiError` (`400` parseo, `422` validación con `issues`). Rutas bajo `/api`.
6. **Documentación**: sección «Formato del fichero» del README (bloque TOML comentado y lista
   de comprobaciones) y, si aplica, la tabla de URLs.
7. **Ejemplos**: `examples/` los usan los tests E2E (round-trip JSON/TOML en `editor.spec.ts`);
   si el campo es relevante, úsalo en alguno.

## Verificar

```bash
devenv shell -- bash -c 'lint && cargo test --workspace && gen-schema && git diff --exit-code -- schema web/src/model/generated.ts'
```

Comprueba además el round-trip real contra la API (skill `run-app`): `PUT` en TOML, `GET`
en JSON y en `?format=toml`, y que un documento inválido devuelve `422` con la ruta correcta.
