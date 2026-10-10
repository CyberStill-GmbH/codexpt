# CodeXPT

Fase 1 del PoC para intercambiar propuestas estructuradas y decisiones auditables entre sesiones de agentes. Esta versión es una CLI local en Rust con SQLite: no tiene interfaz web, servidor HTTP, MCP, conexión a modelos ni acceso a otros repositorios.

## Requisitos

- Rust estable con Cargo.

## Uso

```powershell
cargo run -- init
cargo run -- task create --title "Revisar propuesta" --description "Tarea de ejemplo"
```

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

## Límites de Fase 1

El PoC valida esquema, transiciones, atomicidad, idempotencia, recuperación e historial. No demuestra que conecte sesiones ni que reduzca el tiempo humano: sin una integración de transporte, los mensajes siguen siendo ingresados por la persona. La evaluación comparativa de 12 tareas pertenece a una fase posterior.

## Validación local

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
```
