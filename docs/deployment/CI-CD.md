# CI/CD y operación de despliegues

**Estado:** pipeline implementado; publicación automática desactivada hasta completar las credenciales y la identidad de producción. El frontend continúa en Vercel. Este flujo publica únicamente el Worker de Cloudflare y sus migraciones D1.

## Flujo

El workflow [`.github/workflows/ci-cd.yml`](../../.github/workflows/ci-cd.yml) ejecuta cuatro validaciones independientes para cada PR hacia `main` y cada push a `main`:

1. **Rust:** formato, tests y Clippy sin advertencias.
2. **Worker/D1:** dependencias bloqueadas por `package-lock.json`, auditoría npm sin vulnerabilidades High/Critical, TypeScript, tests de Worker/D1 y bundle Wrangler en modo dry-run.
3. **Web:** auditoría npm sin vulnerabilidades High/Critical, tests unitarios, proxy, build y E2E del navegador Chromium contra el servidor Rust local de pruebas.
4. **Terraform:** formato, inicialización sin backend y validación del root de Cloudflare. No ejecuta `plan`, `apply` ni `destroy`.

Dependabot revisa semanalmente Actions, Cargo y los dos lockfiles npm; agrupa cambios minor/patch por ecosistema en [`dependabot.yml`](../../.github/dependabot.yml). Cada PR de actualización pasa por las mismas validaciones y no se despliega hasta que se integre a `main`.

El job `Deploy Worker to Cloudflare` depende de las cuatro validaciones. Solo se considera para un push o ejecución manual desde `main`, y solo se habilita cuando el repositorio tiene la variable `CLOUDFLARE_DEPLOY_ENABLED=true`. En ese job se validan los destinos, se aplican las migraciones pendientes de D1 en remoto y se despliega el Worker con Wrangler. Al final se consulta `/api/health/ready`; una respuesta distinta de HTTP 2xx marca el despliegue como fallido.

Los despliegues de producción se serializan para que dos pushes no modifiquen el Worker o D1 simultáneamente. Los PR no reciben credenciales de Cloudflare y nunca despliegan previews con datos de producción.

## Configuración inicial de GitHub

En `CyberStill-GmbH/codexpt`, abre **Settings → Environments** y crea `cloudflare-production`. Añade estos secretos al environment; nunca los guardes en el repositorio, archivos `.env`, Terraform, variables `VITE_*` ni logs:

| Secret del environment | Uso |
|---|---|
| `CLOUDFLARE_API_TOKEN` | API token dedicado a CI/CD, con los permisos mínimos para desplegar el Worker existente y aplicar migraciones a la base D1 de CodeXPT. |
| `PROXY_SECRET` | Secreto aleatorio de al menos 32 caracteres. Wrangler lo configura como secret del Worker; el mismo valor debe estar en Vercel como `CODEXPT_PROXY_SECRET`, solo para runtime de Functions. |
| `BOOTSTRAP_EMAIL` | Correo exacto del primer Owner de CodeXPT. Se usa para bootstrap de la primera cuenta, no es un secreto de OIDC. |
| `BOOTSTRAP_WORKSPACE_NAME` | Nombre del workspace Owner inicial. |

En **Settings → Secrets and variables → Actions → Variables** añade:

| Variable | Valor esperado |
|---|---|
| `CLOUDFLARE_DEPLOY_ENABLED` | `true` únicamente después de completar todos los demás valores y la protección de `main`. Déjala sin crear o en `false` mientras el deploy no esté listo. |
| `CLOUDFLARE_ACCOUNT_ID` | ID de la cuenta Cloudflare propia de CodeXPT. |
| `CODEXPT_API_HOST` | Host generado completo del Worker, por ejemplo `codexpt-api.<subdominio-asignado>.workers.dev`; sin `https://`, puerto ni ruta. No uses el dominio de QAPAQ. |
| `CODEXPT_WEB_ORIGIN` | Origen web canónico actual, por ejemplo `https://codexpt.vercel.app`, sin `/` final ni ruta. Si el frontend cambia de host, actualiza también OIDC y Vercel antes de habilitar deploy. |
| `CODEXPT_OIDC_ISSUER` | Issuer HTTPS de la aplicación de identidad dedicada a CodeXPT. No uses el tenant, cliente ni secretos de QAPAQ. Debe ser compatible con el validador de issuer actual del Worker. |
| `CODEXPT_OIDC_CLIENT_ID` | Client ID público de esa aplicación dedicada; no es un secreto. |

El callback se deriva como `${CODEXPT_WEB_ORIGIN}/api/auth/callback` y el destino de login como `${CODEXPT_WEB_ORIGIN}/`. Registra exactamente ese callback en el proveedor OIDC. La aplicación de identidad debe existir antes de habilitar despliegues con smoke test.

En **Settings → Branches** crea una regla para `main` que exija pull request y los cuatro checks: `Rust quality`, `Cloudflare Worker checks`, `Web quality and E2E` y `Cloudflare Terraform validation`. Evita bypass de esos checks salvo recuperación documentada. El deploy ocurre tras el push a `main` cuando todas las dependencias terminan correctamente.

## Permisos del token Cloudflare

Usa un API token separado del login Wrangler de una persona. Limítalo a la cuenta de CodeXPT y a los recursos de CodeXPT que permita Cloudflare. El primer despliegue necesita crear el Worker, por lo que requiere permisos de creación de Workers; una vez creado, reduce el token a Editor del Worker `codexpt-api`. Las migraciones remotas necesitan permiso de escritura D1 sobre la base asignada. No incluyas permisos DNS, zonas, dominios, Pages, billing ni otras cuentas: el flujo no administra DNS ni despliega el frontend.

Cloudflare indica que los entornos no interactivos de Wrangler requieren API token e ID de cuenta; su rol Editor despliega un Worker existente, y el acceso D1 se necesita aparte para consultar/modificar directamente la base. [Guía oficial de GitHub Actions para Workers](https://developers.cloudflare.com/workers/ci-cd/external-cicd/github-actions/), [roles de Workers](https://developers.cloudflare.com/workers/authorization/workers/) y [permisos de API](https://developers.cloudflare.com/fundamentals/api/reference/permissions/).

## Vercel y límites del alcance

El workflow **no** despliega Vercel. Vercel conserva el frontend y sus funciones proxy. Antes del primer login hospedado, configura en el proyecto Vercel estas variables de runtime del servidor y vuelve a desplegar frontend:

- `CODEXPT_CLOUDFLARE_ORIGIN=https://${CODEXPT_API_HOST}`
- `CODEXPT_CLOUDFLARE_HOST=${CODEXPT_API_HOST}`
- `CODEXPT_PROXY_SECRET` con el mismo valor del secret `PROXY_SECRET` de Cloudflare.

Confirma estos nombres contra [`web/api/[...path].js`](<../../web/api/[...path].js>). No uses prefijo `VITE_`: el secreto no puede llegar al bundle del navegador. Si luego se mueve el frontend a Cloudflare Pages, será un cambio separado con su propio build, deploy, origen OIDC y pruebas; no cambies `jappi.net.pe` ni nameservers desde este pipeline.

El health check valida backend, D1 y los campos OIDC requeridos; no demuestra por sí solo que el login de extremo a extremo, autorización por roles o el frontend estén listos para usuarios. Conserva las puertas de aceptación descritas en el [plan de despliegue](PLAN.md) antes de anunciar el producto como producción.

## Migraciones y recuperación

- Crea una migración nueva con `npx wrangler d1 migrations create codexpt-production <descripcion>` y súbela junto con el cambio compatible del Worker.
- Usa cambios **expand-and-contract**: primero aditivos y compatibles hacia atrás; no renombres ni elimines columnas/tablas en el mismo release que deja de usarlas.
- Después de los tests, CI aplica migraciones remotas antes de publicar el nuevo Worker. Wrangler captura un backup al aplicar migraciones en CI; aun así, revisa el SQL y la migración pendiente en el PR. [Referencia oficial de migraciones D1](https://developers.cloudflare.com/d1/wrangler-commands/).
- Si la migración o el deploy falla, el job se detiene y no se anuncia el release como correcto. Investiga el log de Actions, revisa estado de migración y readiness antes de reintentar. El redeploy del código anterior no revierte automáticamente cambios de esquema o datos.
- Si el smoke test falla después de subir una versión, la versión ya fue publicada. Restaura una versión compatible del Worker desde Cloudflare Deployments/Wrangler; si el cambio D1 fue incompatible, usa el backup y el procedimiento de recuperación de D1. Registra el incidente y la referencia del commit.
- No ejecutes `terraform apply` desde este workflow: el estado Terraform no está conectado a un backend remoto y la D1 existente todavía necesita adopción/importación segura documentada en [`infra/terraform-cloudflare/README.md`](../../infra/terraform-cloudflare/README.md).

## Validación y lectura del resultado

- Un PR muestra los cuatro checks de calidad; ninguno tiene credenciales de despliegue.
- En push a `main`, el job de deploy aparecerá como **skipped** hasta que `CLOUDFLARE_DEPLOY_ENABLED=true`; esto es intencional y no equivale a que el Worker esté desplegado.
- Con el interruptor activo y configuración completa, Actions aplica migraciones, publica el Worker y verifica readiness. El commit queda asociado al run de GitHub.
- El flujo no puede activar publicación por sí mismo desde este checkout: primero hay que crear el environment, agregar el token/variables, comprobar OIDC y exigir los checks de `main`.
