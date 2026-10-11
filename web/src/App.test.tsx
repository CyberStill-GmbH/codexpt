import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import App from './App'

const task = {
  task_id: 'task-1', title: 'Revisar una propuesta', description: 'Contexto local',
  status: 'open', pending_decision: false, version: 1,
  created_at: '2026-10-10T10:00:00Z', updated_at: '2026-10-10T10:00:00Z',
}
const detail = { task, messages: [], events: [{ sequence: 1, event_type: 'task_created', actor: 'human:cesar', occurred_at: task.created_at, details: { status: 'open' } }], pending_decision: null }
const json = (data: unknown, status = 200) => ({ ok: status < 400, status, json: async () => data })

afterEach(() => { cleanup(); vi.unstubAllGlobals(); window.history.replaceState({}, '', '/') })

describe('CodeXPT dashboard smoke', () => {
  it('shows a startup screen until the workspace request finishes', async () => {
    let resolveStatus!: (response: ReturnType<typeof json>) => void
    const pendingStatus = new Promise<ReturnType<typeof json>>((resolve) => { resolveStatus = resolve })
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url === '/api/auth/status') return pendingStatus
      if (url === '/api/tasks') return json([])
      return json({ error: 'not found' }, 404)
    })
    vi.stubGlobal('fetch', fetchMock)
    render(<App />)

    expect(screen.getByText('Estamos comiendo bambú. Volvemos pronto.')).toBeTruthy()
    expect(screen.getByRole('status').getAttribute('aria-busy')).toBe('true')
    expect(document.querySelector('.startup-art')?.getAttribute('src')).toBe('/not-found.webp')
    resolveStatus(json({ configured: false }))
    expect(await screen.findByRole('heading', { name: 'El espacio está listo.' })).toBeTruthy()
  })

  it('renders a helpful branded 404 instead of the dashboard for unknown routes', () => {
    window.history.pushState({}, '', '/ruta-que-no-existe')
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    render(<App />)

    expect(screen.getByRole('heading', { name: 'Esta ruta no existe.' })).toBeTruthy()
    expect(screen.getByRole('link', { name: 'Volver a revisiones' }).getAttribute('href')).toBe('/')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('changes review content as a single motion-ready surface', async () => {
    const secondTask = { ...task, task_id: 'task-2', title: 'Segunda revisión' }
    const secondDetail = { ...detail, task: secondTask }
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url === '/api/auth/status') return json({ configured: false })
      if (url === '/api/tasks') return json([task, secondTask])
      if (url === `/api/tasks/${task.task_id}`) return json(detail)
      if (url === `/api/tasks/${secondTask.task_id}`) return json(secondDetail)
      return json({ error: 'not found' }, 404)
    })
    vi.stubGlobal('fetch', fetchMock)
    render(<App />)

    expect(await screen.findByRole('heading', { name: task.title })).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: /segunda revisión/i }))
    expect(await screen.findByRole('heading', { name: secondTask.title })).toBeTruthy()
    expect(document.querySelector('.review-screen')?.classList.contains('review-screen')).toBe(true)
  })

  it('creates a real review through its API and opens the persisted detail', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url === '/api/auth/status') return json({ configured: false })
      if (url === '/api/tasks' && init?.method === 'POST') return json({ data: task }, 201)
      if (url === '/api/tasks') return json([task])
      if (url === `/api/tasks/${task.task_id}`) return json(detail)
      return json({ error: 'not found' }, 404)
    })
    vi.stubGlobal('fetch', fetchMock)
    render(<App />)

    fireEvent.click(await screen.findByRole('button', { name: /crear revisión/i }))
    const dialog = within(screen.getByRole('dialog'))
    fireEvent.change(dialog.getByLabelText('Título'), { target: { value: task.title } })
    fireEvent.change(dialog.getByLabelText(/contexto/i), { target: { value: task.description } })
    fireEvent.click(dialog.getByRole('button', { name: /crear revisión/i }))

    expect(await screen.findByRole('heading', { name: task.title })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Cronología' })).toBeTruthy()
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith('/api/tasks', expect.objectContaining({ method: 'POST' })))
    expect(screen.getByText('Revisión creada')).toBeTruthy()
    expect(screen.getByText(/Actor declarado · origen no autenticado/)).toBeTruthy()
  })

  it('does not present an offline production UI as a local dashboard when the API cannot be reached', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('network down')))
    render(<App />)
    expect(await screen.findByRole('heading', { name: 'Estamos comiendo bambú. Volvemos pronto.' })).toBeTruthy()
    expect(screen.getByAltText('Un panda de CodeXPT recorre dos rutas que vuelven a encontrarse.')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Reintentar conexión' })).toBeTruthy()
    expect(screen.queryByText('Espacio local')).toBeNull()
    expect(screen.queryByText('Servidor en loopback')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Crear revisión' })).toBeNull()
  })

  it('creates, copies and explicitly acknowledges a verified context checkpoint', async () => {
    const checkpoint = {
      content: {
        schema: 'codexpt.context-checkpoint', version: '1.0.0', checkpoint_id: 'checkpoint-1', task_id: task.task_id,
        source: 'chatgpt_web', source_completeness: 'user_curated_partial', goal: 'Retomar con decisiones verificables',
        decisions: 'Mantener el techo de coste', completed: '', blocked: '', next_actions: 'Verificar el siguiente gate',
        constraints: 'No incluir secretos', artifacts: [], context_capacity: 'unknown; no ChatGPT web context telemetry is available',
      }, content_sha256: 'a'.repeat(64), status: 'ready' as const, created_by: 'human:cesar', created_at: task.created_at,
      resumed_by: null, resumed_at: null,
    }
    let current: { content: typeof checkpoint.content; content_sha256: string; status: 'ready' | 'resumed'; created_by: string; created_at: string; resumed_by: string | null; resumed_at: string | null } = checkpoint
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url === '/api/auth/status') return json({ configured: false })
      if (url === '/api/tasks') return json([task])
      if (url === `/api/tasks/${task.task_id}`) return json(detail)
      if (url === `/api/tasks/${task.task_id}/checkpoints` && init?.method === 'POST') return json({ data: checkpoint }, 201)
      if (url === `/api/tasks/${task.task_id}/checkpoints`) return json([current])
      if (url.endsWith('/checkpoints/checkpoint-1/resume')) { current = { ...checkpoint, status: 'resumed' as const, resumed_by: 'human:cesar', resumed_at: task.created_at }; return json({ data: current }) }
      return json({ error: 'not found' }, 404)
    })
    vi.stubGlobal('fetch', fetchMock)
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: vi.fn().mockResolvedValue(undefined) } })
    render(<App />)

    await screen.findByRole('heading', { name: task.title })
    expect(screen.getByText('Capacidad restante: desconocida')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Preparar checkpoint manual' }))
    fireEvent.change(await screen.findByLabelText('Objetivo actual'), { target: { value: 'Retomar con decisiones verificables' } })
    fireEvent.change(screen.getByLabelText('Decisiones vigentes'), { target: { value: 'Mantener el techo de coste' } })
    fireEvent.click(screen.getByRole('button', { name: 'Revisé y congelar' }))

    expect(await screen.findByText('Listo para transferir')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Copiar paquete' }))
    expect(await screen.findByText('Paquete copiado. Pégalo manualmente en un nuevo chat del mismo proyecto.')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Confirmar en hilo nuevo' }))
    expect(await screen.findByText('Recepción confirmada')).toBeTruthy()
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(`/api/tasks/${task.task_id}/checkpoints/checkpoint-1/resume`, expect.objectContaining({ method: 'POST' })))
  })

  it('lets the verified workspace Owner create a one-time invitation', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url === '/api/auth/status') return json({ configured: true })
      if (url === '/api/auth/me') return json({ csrf_token: 'csrf-test', email: 'owner@example.test' })
      if (url === '/api/workspaces') return json([{ workspace_id: 'workspace-1', name: 'Alpha', role: 'OWNER' }])
      if (url === '/api/tasks') return json([])
      if (url === '/api/workspaces/workspace-1/members') return json([{ user_id: 'owner-1', email: 'owner@example.test', role: 'OWNER' }])
      if (url === '/api/workspaces/workspace-1/invites' && init?.method === 'POST') return json({ invite_id: 'invite-1', raw_token: 'single-use-invite-code', expires_at: '2026-10-17T10:00:00Z' }, 201)
      return json({ error: 'not found' }, 404)
    })
    vi.stubGlobal('fetch', fetchMock)
    render(<App />)

    fireEvent.click(await screen.findByRole('button', { name: /miembros/i }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Miembros' }))
    fireEvent.change(dialog.getByLabelText(/invitar por correo/i), { target: { value: 'editor@example.test' } })
    fireEvent.click(dialog.getByRole('button', { name: /crear invitación/i }))

    expect(await dialog.findByText('single-use-invite-code')).toBeTruthy()
    const [url, init] = fetchMock.mock.calls.find(([calledUrl, calledInit]) => String(calledUrl).endsWith('/invites') && calledInit?.method === 'POST') ?? []
    expect(url).toBe('/api/workspaces/workspace-1/invites')
    expect(init?.headers).toMatchObject({ 'X-CodeXPT-Workspace-ID': 'workspace-1', 'X-CSRF-Token': 'csrf-test' })
  })

  it('lets the Owner change a member role and revoke that member', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url === '/api/auth/status') return json({ configured: true })
      if (url === '/api/auth/me') return json({ csrf_token: 'csrf-test', email: 'owner@example.test' })
      if (url === '/api/workspaces') return json([{ workspace_id: 'workspace-1', name: 'Alpha', role: 'OWNER' }])
      if (url === '/api/tasks') return json([])
      if (url === '/api/workspaces/workspace-1/members') return json([
        { user_id: 'owner-1', email: 'owner@example.test', role: 'OWNER' },
        { user_id: 'editor-1', email: 'editor@example.test', role: 'EDITOR' },
      ])
      if (url === '/api/workspaces/workspace-1/members/editor-1' && ['PATCH', 'DELETE'].includes(init?.method ?? '')) return json({}, 204)
      return json({ error: 'not found' }, 404)
    })
    vi.stubGlobal('fetch', fetchMock)
    render(<App />)

    fireEvent.click(await screen.findByRole('button', { name: /miembros/i }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Miembros' }))
    expect(dialog.getByText('Owner · administra el workspace')).toBeTruthy()
    expect(dialog.queryByRole('button', { name: /revocar acceso a owner/i })).toBeNull()

    fireEvent.change(dialog.getByLabelText('Rol de editor@example.test'), { target: { value: 'VIEWER' } })
    expect(await dialog.findByText('Viewer · solo lectura')).toBeTruthy()
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith('/api/workspaces/workspace-1/members/editor-1', expect.objectContaining({ method: 'PATCH' })))

    fireEvent.click(dialog.getByRole('button', { name: 'Revocar acceso a editor@example.test' }))
    fireEvent.click(dialog.getByRole('button', { name: 'Confirmar' }))
    await waitFor(() => expect(dialog.queryByText('editor@example.test')).toBeNull())
    const deletion = fetchMock.mock.calls.find(([calledUrl, calledInit]) => String(calledUrl).endsWith('/editor-1') && calledInit?.method === 'DELETE')
    expect(deletion).toBeTruthy()
    expect(deletion?.[1]?.headers).toMatchObject({ 'X-CodeXPT-Workspace-ID': 'workspace-1', 'X-CSRF-Token': 'csrf-test', 'Content-Type': 'application/json' })
  })

  it('keeps a hosted Viewer read-only across creation, messages, decisions, and transitions', async () => {
    const viewerTask = { ...task, status: 'resolved' }
    const viewerDetail = { ...detail, task: viewerTask }
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url === '/api/auth/status') return json({ configured: true })
      if (url === '/api/auth/me') return json({ csrf_token: 'csrf-viewer', email: 'viewer@example.test' })
      if (url === '/api/workspaces') return json([{ workspace_id: 'workspace-1', name: 'Alpha', role: 'VIEWER' }])
      if (url === '/api/tasks') return json([viewerTask])
      if (url === `/api/tasks/${viewerTask.task_id}`) return json(viewerDetail)
      return json({ error: 'not found' }, 404)
    })
    vi.stubGlobal('fetch', fetchMock)
    render(<App />)

    expect(await screen.findByRole('heading', { name: viewerTask.title })).toBeTruthy()
    expect(await screen.findByText('Tu rol permite consultar el historial, pero no registrar mensajes.')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Crear revisión' })).toBeNull()
    expect(screen.queryByLabelText('Contenido del mensaje')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Aprobar decisión' })).toBeNull()
    expect(screen.queryByLabelText('Pregunta para César')).toBeNull()
    expect(screen.getByText('Solo un Owner puede aprobar decisiones.')).toBeTruthy()
  })

  it('shows the authenticated author instead of an editable agent identity in a hosted workspace', async () => {
    const editorTask = { ...task }
    const editorDetail = {
      ...detail,
      task: editorTask,
      events: [{ sequence: 1, event_type: 'task_created', actor: 'human:user-123', occurred_at: task.created_at, details: { status: 'open', verified_human: { session_id: 'session-1' } } }],
    }
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url === '/api/auth/status') return json({ configured: true })
      if (url === '/api/auth/me') return json({ csrf_token: 'csrf-editor', email: 'editor@example.test' })
      if (url === '/api/workspaces') return json([{ workspace_id: 'workspace-1', name: 'Alpha', role: 'EDITOR' }])
      if (url === '/api/tasks') return json([editorTask])
      if (url === `/api/tasks/${editorTask.task_id}`) return json(editorDetail)
      return json({ error: 'not found' }, 404)
    })
    vi.stubGlobal('fetch', fetchMock)
    render(<App />)

    expect((await screen.findAllByText('editor@example.test')).length).toBe(2)
    expect(screen.queryByLabelText('Autor declarado')).toBeNull()
    expect(document.getElementById('message-sender')).toBeNull()
    expect(screen.getByText('Usuario autenticado')).toBeTruthy()
    expect(screen.getByText('Sesión autenticada · autor registrado por el servidor')).toBeTruthy()
  })
})
