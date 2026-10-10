# CodeXPT

[Repositorio](https://github.com/CyberStill-GmbH/codexpt) · [Plan de despliegue](docs/deployment/PLAN.md) · [Reporte de validación](docs/VALIDATION-REPORT.md)

**Release:** `v0.1.0-local-mvp`. Workspace local para registrar propuestas, revisiones, evidencia y decisiones humanas con un historial auditable. El core es Rust + SQLite; el dashboard local de React/TypeScript se sirve desde Rust y no depende de infraestructura hospedada.

## Requisitos

- Rust estable con Cargo.
- Node.js + npm para reconstruir los recursos estáticos del dashboard desde `web/`; no se requiere Node en tiempo de ejecución.

## Uso

```powershell
cargo run -- init
cargo run -- task create --title "Revisar propuesta" --description "Tarea de ejemplo"
cargo run -- serve
```

El dashboard queda en `http://127.0.0.1:4173`. El servidor solo escucha en loopback. Deténlo con `Ctrl+C` al terminar. Los recursos compilados se incluyen en el repositorio y se sirven desde el binario Rust. Para reconstruirlos tras editar la UI: `npm --prefix web ci && npm --prefix web run build`, y luego recompila Rust. Consulta [`docs/OPERATIONS.md`](docs/OPERATIONS.md) y [`docs/SECURITY.md`](docs/SECURITY.md).

La CLI imprime JSON. Copia el `task_id` y úsalo en los siguientes comandos:

```powershell
cargo run -- task transition <TASK_ID> --expect open --to proposed --actor agent:codex
cargo run -- message send <TASK_ID> --sender agent:codex --recipient agent:chatgpt --body "Propuesta inicial"
cargo run -- task transition <TASK_ID> --expect proposed --to under_review --actor agent:chatgpt
cargo run -- task request-decision <TASK_ID> --question "¿Qué opción elegimos?"
cargo run -- task show <TASK_ID>
cargo run -- task answer-decision <TASK_ID> <DECISION_ID> --answer "Opción A"
cargo run -- events <TASK_ID>
cargo run -- message list <TASK_ID>
```

Por defecto la base queda en `.codexpt/state.sqlite3`. El archivo está ignorado por Git. Para usar otra ubicación, pasa `--db <RUTA>` antes del subcomando.

## Protocolo v0.1

- Estados persistidos: `open → proposed → under_review → resolved → approved`.
- Una objeción puede devolver `under_review → proposed` para que se responda y vuelva a revisión.
- `NEEDS_HUMAN` es una solicitud pendiente separada, no un estado que sustituya el estado de la tarea. Mientras haya una decisión pendiente, la tarea conserva su estado y no puede avanzar; solo `human:cesar` puede responderla.
- `approved` requiere aprobación explícita de `human:cesar`.
- Cada transición y mensaje crea un evento secuenciado con actor, hora, correlación y evidencia.
- `message_id` es idempotente: repetir exactamente el mismo mensaje no duplica efectos; reutilizar el identificador con otro contenido falla.
- Referencias de evidencia incluyen localizador y SHA-256 declarado. La herramienta valida el formato, pero no descarga ni verifica el contenido remoto/local en Fase 1.
- Los nombres de actor (`agent:codex`, `agent:chatgpt`, `human:cesar`) son etiquetas declaradas, no identidades autenticadas. No usar el buzón para secretos.
- SQLite activa claves foráneas, WAL, transacciones inmediatas y un timeout de escritura. Las transiciones requieren el estado esperado para rechazar escrituras obsoletas.

## Alcance y límites de v0.1.0-local-mvp

El MVP local valida esquema, transiciones, atomicidad, idempotencia, recuperación e historial y permite operar esas funciones desde el dashboard. No conecta automáticamente sesiones ni demuestra que reduzca el tiempo humano: los mensajes de otros agentes siguen siendo ingresados por una persona. Las etiquetas se pueden suplantar por cualquier proceso con acceso local; no es autenticación. La evaluación comparativa de 12 tareas pertenece a una fase posterior.

## Identity v1 — prototipo en curso

La rama de trabajo agrega autenticación OIDC, sesiones revocables, roles por workspace, invitaciones y una página de acceso CodeXPT. Esto es un prototipo local de la siguiente fase, no una versión desplegable: el proveedor OIDC no está configurado, el callback aún no tiene una prueba end-to-end con un proveedor controlado, y faltan controles/auditoría de producción. El release `v0.1.0-local-mvp` sigue siendo el modo local descrito arriba. No expongas el servidor ni actives registro público.

El Owner inicial se aprovisiona una sola vez mediante una operación explícita del operador, asociada al `issuer` y `subject` exactos del proveedor:

```powershell
cargo run -- --db <RUTA_DE_LA_BASE> identity bootstrap-owner --issuer <ISSUER_OIDC> --subject <SUBJECT_VERIFICADO> --email <CORREO_VERIFICADO> --workspace-name "CodeXPT"
```

Este comando crea el primer usuario y workspace `OWNER`; rechaza una segunda ejecución. Las identidades posteriores requieren invitación con correo verificado. Consulta [`docs/identity-v1/AUTHORIZATION-TEST-PLAN.md`](docs/identity-v1/AUTHORIZATION-TEST-PLAN.md) para las pruebas y los bloqueos de salida cloud.

## Validación local

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
npm --prefix web run test
npm --prefix web run build
npm --prefix web run test:e2e
./scripts/smoke.ps1
```

Arquitectura C4, decisiones y estrategia de verificación: [`docs/architecture/C4.md`](docs/architecture/C4.md), [`docs/adr/`](docs/adr/) y [`docs/TESTING.md`](docs/TESTING.md). El experimento y sus límites están en [`EXPERIMENT.md`](EXPERIMENT.md).
