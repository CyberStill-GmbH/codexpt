# Contribuir a CodeXPT

Gracias por ayudar a mejorar CodeXPT. Antes de proponer un cambio, revisa el [estado del proyecto](README.md#estado-y-próximos-hitos), la [hoja de ruta](ROADMAP.md) y las decisiones existentes en [`docs/adr`](docs/adr/).

## Flujo de trabajo

1. Abre una incidencia o propuesta con el problema que quieres resolver y el resultado esperado.
2. Mantén los cambios enfocados; documenta una decisión en un ADR cuando altere arquitectura, identidad, persistencia o límites de seguridad.
3. Ejecuta las verificaciones pertinentes y describe qué comprobaste en la solicitud de cambios.
4. No incluyas credenciales, datos personales, bases locales, archivos Terraform de estado ni datos de clientes.

## Verificaciones del MVP local

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
npm --prefix web run lint
npm --prefix web run test
npm --prefix web run build
npm --prefix web run test:e2e
```

Si cambias la persistencia, la autenticación o los permisos, actualiza también la documentación y pruebas correspondientes. No presentes el prototipo de identidad como autenticación lista para producción.

## Reportar una vulnerabilidad

No publiques detalles de una vulnerabilidad sin corregir en una incidencia pública. Contacta a los mantenedores por un canal privado de GitHub y comparte únicamente la información necesaria para reproducirla.
