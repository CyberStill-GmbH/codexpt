import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import App from './App'

const task = {
  task_id: 'task-1', title: 'Revisar una propuesta', description: 'Contexto local',
  status: 'open', pending_decision: false,
  created_at: '2026-10-10T10:00:00Z', updated_at: '2026-10-10T10:00:00Z',
}
const detail = { task, messages: [], events: [{ sequence: 1, event_type: 'task_created', actor: 'human:cesar', occurred_at: task.created_at, details: { status: 'open' } }], pending_decision: null }
const json = (data: unknown, status = 200) => ({ ok: status < 400, status, json: async () => data })

afterEach(() => vi.unstubAllGlobals())

describe('CodeXPT dashboard smoke', () => {
  it('creates a real review through its API and opens the persisted detail', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
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

  it('shows a recoverable message when the API cannot be reached', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('network down')))
    render(<App />)
    expect(await screen.findByRole('alert')).toBeTruthy()
  })
})
