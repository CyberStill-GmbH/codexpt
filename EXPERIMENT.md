# CodeXPT — protocolo experimental

## Registro inicial

- Versión: 0.1
- Fecha de inicio: 2026-10-10
- Alcance actual: Fase 1, protocolo y persistencia local.
- Estado: especificación congelada para el PoC inicial.

## Hipótesis

Un buzón estructurado puede reducir el trabajo de coordinación entre dos sesiones sin reducir la calidad de sus revisiones. La Fase 1 todavía no prueba esta hipótesis; solo valida el protocolo y su persistencia.

## Experimento propuesto para una fase posterior

- 12 tareas pequeñas de ingeniería en Rust/TypeScript, en lógica, concurrencia y seguridad.
- Modalidad A: intercambio manual de mensajes.
- Modalidad B: transporte estructurado entre sesiones.
- Mismo flujo: propuesta → revisión → objeción → respuesta → dictamen.
- Los defectos objetivo permanecen desconocidos para las sesiones.
- Criterio exploratorio: detectar al menos tantos defectos reales, reducir al menos 30 % el tiempo activo de César y no aumentar significativamente los falsos positivos.

## Métricas

- Defectos reales identificados y justificación verificable.
- Tiempo activo de César, separado de las esperas.
- Intervenciones manuales por tarea.
- Falsos positivos.
- Tokens solo si la interfaz utilizada los expone de forma fiable; de lo contrario, registrar `no medible`.

## Controles y limitaciones

- Registrar modelo, modo y configuración observables por sesión; anotar cambios o datos no disponibles.
- 12 tareas producen evidencia exploratoria, no una conclusión estadística definitiva.
- La Fase 1 no conecta sesiones y no debe presentarse como evidencia de ahorro de tiempo.
- La identidad de agente es declarativa en el PoC local y no está autenticada.
- César inicia cada ciclo, responde las decisiones pendientes y aprueba el dictamen final.
- Las sesiones no tienen permisos para modificar el repositorio evaluado ni ejecutar herramientas de infraestructura.

## Registro de cambios del protocolo

| Fecha | Decisión | Motivo |
|---|---|---|
| 2026-10-10 | `NEEDS_HUMAN` se modela como decisión pendiente separada del estado de revisión. | Permite pausar y reanudar sin perder el estado previo. |
| 2026-10-10 | Sin web, HTTP, MCP o API durante Fase 1. | Aislar y validar el protocolo antes de integrar transporte. |
| 2026-10-10 | El consumo de tokens no es criterio obligatorio si la interfaz no lo expone. | Evitar mediciones inventadas o comparaciones incompletas. |

## Prueba de comunicación mediante interfaz

- Fecha: 2026-10-10.
- Resultado: completada una ida y vuelta entre la sesión Codex y la conversación ChatGPT indicada por César, usando la interfaz web.
- Mensaje enviado: identificación explícita como Codex, estado verificable del repositorio, siguiente objetivo técnico y pregunta sobre prioridades de endurecimiento.
- Respuesta recuperada: ChatGPT priorizó migraciones/versionado de esquema SQLite antes de autenticación completa, recomendó mantener la regla de aprobación humana y advirtió que `human:cesar` es una etiqueta declarada, no una identidad autenticada.
- Conclusión: se verificó que esta sesión pudo escribir en el hilo indicado y leer su respuesta. Esto demuestra un intercambio mediado por la interfaz gráfica; no demuestra una integración persistente, MCP ni comunicación autónoma entre sesiones.
- Límite: la respuesta propuso registrar una decisión de arquitectura, pero esta prueba solo autorizó registrar el intercambio. La propuesta queda pendiente de revisión y no se trata como aprobación para implementar migraciones u otros cambios.
- Privacidad: el registro omite el URL/ID del chat y cualquier contenido ajeno a esta prueba.
