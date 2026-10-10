import { useCallback, useEffect, useMemo, useState } from 'react'
import { ArrowDownLeft, ArrowUpRight, Check, ChevronDown, CircleDot, Clock3, FileText, Plus, Search, Send, ShieldAlert, ShieldCheck, SlidersHorizontal, Sparkles } from 'lucide-react'
import './App.css'

type Status = 'open' | 'proposed' | 'under_review' | 'resolved' | 'approved'
type Task = { task_id: string; title: string; description: string; status: Status; pending_decision: boolean; created_at: string; updated_at: string }
type Evidence = { label: string; locator: string; sha256: string }
type Message = { message_id: string; sender: string; recipient: string; body: string; evidence_refs: Evidence[]; created_at: string }
type Entry = { sequence: number; event_type: string; actor: string; occurred_at: string; details: Record<string, unknown> }
type Decision = { decision_id: string; question: string; requested_by: string; created_at: string }
type Detail = { task: Task; messages: Message[]; events: Entry[]; pending_decision: Decision | null }

const labels: Record<Status, string> = { open: 'Abierta', proposed: 'Propuesta', under_review: 'En revisión', resolved: 'Resuelta', approved: 'Aprobada' }
const api = async <T,>(url: string, init?: RequestInit): Promise<T> => {
  const response = await fetch(url, { ...init, headers: { ...(init?.body ? { 'Content-Type': 'application/json' } : {}), ...init?.headers } })
  const payload = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(payload.error ?? `La solicitud falló (${response.status})`)
  return payload as T
}
const date = (value: string) => new Intl.DateTimeFormat('es-PE', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
const actorLabel = (actor: string) => ({ 'agent:codex': 'Codex', 'agent:chatgpt': 'ChatGPT', 'human:cesar': 'César' } as Record<string, string>)[actor] ?? actor
const steps: Status[] = ['open', 'proposed', 'under_review', 'resolved', 'approved']
const eventLabel = (event: Entry) => {
  if (event.event_type === 'task_created') return 'Revisión creada'
  if (event.event_type === 'task_transitioned') return `Estado: ${labels[String(event.details.to) as Status] ?? String(event.details.to)}`
  if (event.event_type === 'human_decision_requested') return `Decisión solicitada: ${String(event.details.question ?? '')}`
  if (event.event_type === 'human_decision_answered') return `Respuesta: ${String(event.details.answer ?? '')}`
  if (event.event_type === 'message_recorded') return 'Mensaje registrado'
  return event.event_type.replaceAll('_', ' ')
}
function BrandMark() {
  return <svg className="brand-mark" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
    <path d="M12 3v18" />
    <circle cx="12" cy="6" r="1.25" />
    <circle cx="12" cy="12" r="1.5" />
    <circle cx="12" cy="18" r="1.25" />
    <path className="mark-cross" d="m9.5 9.5 5 5m0-5-5 5" />
  </svg>
}

function App() {
  const [tasks, setTasks] = useState<Task[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [detail, setDetail] = useState<Detail | null>(null)
  const [filter, setFilter] = useState<'all' | 'pending' | 'approved'>('all')
  const [search, setSearch] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [dialog, setDialog] = useState(false)
  const [body, setBody] = useState('')
  const [sender, setSender] = useState('agent:codex')
  const [recipient, setRecipient] = useState('agent:chatgpt')
  const [question, setQuestion] = useState('')
  const [answer, setAnswer] = useState('')
  const [evidenceLabel, setEvidenceLabel] = useState('')
  const [evidenceLocator, setEvidenceLocator] = useState('')
  const [evidenceHash, setEvidenceHash] = useState('')

  const loadDetail = useCallback(async (id: string) => {
    const next = await api<Detail>(`/api/tasks/${id}`)
    setDetail(next)
    setTasks((current) => current.map((task) => task.task_id === id ? next.task : task))
  }, [])
  const loadTasks = useCallback(async (preferred?: string) => {
    const rows = await api<Task[]>('/api/tasks')
    setTasks(rows)
    const id = preferred ?? selected ?? rows[0]?.task_id ?? null
    setSelected(id)
    if (id) await loadDetail(id); else setDetail(null)
  }, [loadDetail, selected])
  // oxlint-disable-next-line react/set-state-in-effect -- surface an asynchronous initial-load failure.
  useEffect(() => { loadTasks().catch((e: unknown) => setError(e instanceof Error ? e.message : 'No se pudo cargar el espacio.')) }, [loadTasks])
  const filtered = useMemo(() => tasks.filter((task) => {
    const term = `${task.title} ${task.description}`.toLocaleLowerCase()
    return term.includes(search.toLocaleLowerCase()) && (filter === 'all' || (filter === 'pending' ? task.pending_decision : task.status === 'approved'))
  }), [tasks, search, filter])
  const activity = useMemo(() => detail ? [
    ...detail.messages.map((message) => ({ id: message.message_id, date: message.created_at, kind: 'message' as const, actor: message.sender, recipient: message.recipient, text: message.body, evidence: message.evidence_refs })),
    ...detail.events.filter((event) => event.event_type !== 'message_recorded').map((event) => ({ id: `event-${event.sequence}`, date: event.occurred_at, kind: 'event' as const, actor: event.actor, recipient: null, text: eventLabel(event), extra: event.details })),
  ].sort((a, b) => a.date.localeCompare(b.date) || a.id.localeCompare(b.id)) : [], [detail])

  const mutate = async (operation: () => Promise<unknown>, preferred?: string) => {
    setBusy(true); setError('')
    try { const result = await operation(); await loadTasks(typeof result === 'string' ? result : preferred) }
    catch (e) { setError(e instanceof Error ? e.message : 'No se pudo completar la operación.') }
    finally { setBusy(false) }
  }
  const createTask = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    await mutate(async () => {
      const result = await api<{ data: Task }>('/api/tasks', { method: 'POST', body: JSON.stringify({ title: form.get('title'), description: form.get('description') }) })
      setDialog(false)
      return result.data.task_id
    })
  }
  const submitMessage = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!detail || !body.trim()) return
    const fields = [evidenceLabel.trim(), evidenceLocator.trim(), evidenceHash.trim()]
    if (fields.some(Boolean) && fields.some((field) => !field)) { setError('Completa los tres campos de evidencia o déjalos vacíos.'); return }
    if (fields[2] && !/^[a-f0-9]{64}$/i.test(fields[2])) { setError('El SHA-256 debe tener 64 caracteres hexadecimales.'); return }
    const evidence_refs = fields.every(Boolean) ? [{ label: fields[0], locator: fields[1], sha256: fields[2] }] : []
    void mutate(async () => { await api(`/api/tasks/${detail.task.task_id}/messages`, { method: 'POST', body: JSON.stringify({ sender, recipient, body: body.trim(), evidence_refs }) }); setBody(''); setEvidenceLabel(''); setEvidenceLocator(''); setEvidenceHash('') })
  }
  const transition = (next: Status, actor: string) => {
    if (!detail) return
    void mutate(() => api(`/api/tasks/${detail.task.task_id}/transition`, { method: 'POST', body: JSON.stringify({ expected: detail.task.status, next, actor }) }))
  }
  const askHuman = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!detail || !question.trim()) return
    void mutate(async () => { await api(`/api/tasks/${detail.task.task_id}/decisions`, { method: 'POST', body: JSON.stringify({ question: question.trim(), actor: 'agent:codex' }) }); setQuestion('') })
  }
  const answerHuman = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!detail?.pending_decision || !answer.trim()) return
    const decisionId = detail.pending_decision.decision_id
    const taskId = detail.task.task_id
    void mutate(async () => { await api(`/api/tasks/${taskId}/decisions/${decisionId}/answer`, { method: 'POST', body: JSON.stringify({ answer: answer.trim() }) }); setAnswer('') })
  }

  const actions: Record<Status, Array<{ next: Status; actor: string; label: string }>> = {
    open: [{ next: 'proposed', actor: 'agent:codex', label: 'Registrar propuesta' }],
    proposed: [{ next: 'under_review', actor: 'agent:chatgpt', label: 'Iniciar revisión' }],
    under_review: [{ next: 'proposed', actor: 'agent:codex', label: 'Devolver con objeción' }, { next: 'resolved', actor: 'agent:chatgpt', label: 'Marcar como resuelta' }],
    resolved: [{ next: 'approved', actor: 'human:cesar', label: 'Aprobar decisión' }], approved: [],
  }

  return <div className="app-shell">
    <aside className="sidebar" aria-label="Navegación de tareas">
      <a className="brand" href="#inicio" aria-label="CodeXPT, inicio"><BrandMark /><span className="brand-word">Code<span>XPT</span></span></a>
      <div className="local-tag"><span className="local-dot" /> Espacio local</div>
      <div className="side-heading"><span>Revisiones</span><button className="icon-button" type="button" aria-label="Crear revisión" onClick={() => setDialog(true)}><Plus size={17} /></button></div>
      <label className="search-box"><Search size={16} aria-hidden="true" /><input aria-label="Buscar revisiones" placeholder="Buscar" value={search} onChange={(e) => setSearch(e.target.value)} /><kbd>/</kbd></label>
      <div className="filter-tabs" role="group" aria-label="Filtrar revisiones"><button type="button" className={filter === 'all' ? 'selected' : ''} onClick={() => setFilter('all')}>Todas</button><button type="button" className={filter === 'pending' ? 'selected' : ''} onClick={() => setFilter('pending')}>Decisiones</button></div>
      <nav className="task-list" aria-label="Lista de revisiones">
        {filtered.map((task) => <button key={task.task_id} type="button" className={`task-row ${task.task_id === selected ? 'active' : ''}`} onClick={() => { setSelected(task.task_id); loadDetail(task.task_id).catch((e: Error) => setError(e.message)) }}><span className={`status-marker ${task.status}`} aria-hidden="true" /><span className="task-row-copy"><strong>{task.title}</strong><small>{task.pending_decision ? 'Decisión pendiente' : labels[task.status]}</small></span>{task.pending_decision && <span className="pending-mark" aria-label="Decisión pendiente">!</span>}</button>)}
        {!filtered.length && <p className="empty-list">{tasks.length ? 'No hay revisiones con ese filtro.' : 'Aquí aparecerán tus revisiones.'}</p>}
      </nav>
      <div className="sidebar-foot"><div className="security-note"><ShieldCheck size={16} /><span>Solo en este equipo<br /><small>El servidor escucha en loopback</small></span></div><div className="profile-row"><span className="avatar">C</span><span><strong>César</strong><small>Director técnico</small></span><ChevronDown size={15} /></div></div>
    </aside>

    <main className="workspace" id="inicio">
      <header className="topbar"><div className="breadcrumbs"><span>CodeXPT</span><span className="crumb-sep">/</span><strong>{detail?.task.title ?? 'Revisiones'}</strong></div><div className="top-actions"><span className="connection"><span className="local-dot" /> Local</span><button className="quiet-button" type="button" aria-label="Filtrar revisiones aprobadas" onClick={() => setFilter(filter === 'approved' ? 'all' : 'approved')}><SlidersHorizontal size={15} /><span>Filtro</span></button></div></header>
      {error && <div className="error-banner" role="alert"><ShieldAlert size={17} /><span>{error}</span><button type="button" aria-label="Cerrar error" onClick={() => setError('')}>×</button></div>}
      {detail ? <>
        <section className="review-heading"><div className="heading-main"><div className="eyebrow"><FileText size={14} /> Registro de revisión <span>/{detail.task.task_id.slice(0, 8)}</span></div><h1>{detail.task.title}</h1><p>{detail.task.description || 'Sin descripción. Añade contexto a través de la conversación.'}</p></div><span className={`status-pill ${detail.task.status}`}><span className="status-dot" />{labels[detail.task.status]}</span></section>
        {detail.pending_decision && <section className="decision-banner" aria-labelledby="decision-title"><div className="decision-icon"><CircleDot size={20} /></div><div className="decision-question"><span id="decision-title">Tu decisión es necesaria</span><p>{detail.pending_decision.question}</p><small>Solicitada por {detail.pending_decision.requested_by} · {date(detail.pending_decision.created_at)}</small></div><form className="decision-answer" onSubmit={answerHuman}><label className="sr-only" htmlFor="decision-answer">Tu respuesta</label><input id="decision-answer" value={answer} onChange={(e) => setAnswer(e.target.value)} placeholder="Escribe una respuesta" required /><button className="primary-button" type="submit" disabled={busy || !answer.trim()}>Responder</button></form></section>}
        <div className="work-grid">
          <section className="activity-panel" aria-labelledby="activity-title"><div className="section-heading"><div><h2 id="activity-title">Cronología</h2><span>{detail.events.length} eventos · {detail.messages.length} mensajes</span></div><button className="quiet-button compact" type="button" onClick={() => loadDetail(detail.task.task_id)}><Clock3 size={15} /> Actualizar</button></div>
            {activity.length ? <ol className="timeline">{activity.map((item) => <li key={item.id} className={`timeline-entry ${item.kind === 'event' ? 'audit-entry' : 'message-entry'}`}><span className={`timeline-marker ${item.actor.includes('chatgpt') ? 'chatgpt' : item.actor.includes('cesar') ? 'human' : 'codex'}`} aria-hidden="true" /><div className="entry-meta"><strong>{actorLabel(item.actor)}</strong>{item.recipient && <span><ArrowUpRight size={13} /> {actorLabel(item.recipient)}</span>}<time>{date(item.date)}</time></div>{item.kind === 'message' ? <><p className="entry-body">{item.text}</p>{item.evidence.length > 0 && <details className="evidence-list"><summary>{item.evidence.length} referencia{item.evidence.length === 1 ? '' : 's'} de evidencia</summary><ul>{item.evidence.map((ref, index) => <li key={`${ref.sha256}-${index}`}><strong>{ref.label}</strong><span>{ref.locator}</span><code>SHA-256 · {ref.sha256}</code></li>)}</ul></details>}</> : <p className="entry-event">{item.text}{item.extra && typeof item.extra.from === 'string' && <span> · desde {labels[item.extra.from as Status] ?? item.extra.from}</span>}</p>}<div className="entry-provenance"><ShieldAlert size={13} /> Actor declarado · origen no autenticado</div></li>)}</ol> : <div className="empty-conversation"><div className="empty-glyph"><ArrowDownLeft size={19} /></div><strong>El registro empieza aquí</strong><p>Añade una propuesta o respuesta de una sesión participante. CodeXPT registra quién la declara, sin verificar su identidad.</p></div>}
            <form className="composer" onSubmit={submitMessage}><div className="composer-meta"><label htmlFor="message-sender">Autor declarado</label><select id="message-sender" value={sender} onChange={(e) => setSender(e.target.value)}><option value="agent:codex">Codex</option><option value="agent:chatgpt">ChatGPT</option><option value="human:cesar">César</option></select><span className="composer-arrow" aria-hidden="true">→</span><label htmlFor="message-recipient">Para</label><select id="message-recipient" value={recipient} onChange={(e) => setRecipient(e.target.value)}><option value="agent:chatgpt">ChatGPT</option><option value="agent:codex">Codex</option><option value="human:cesar">César</option></select></div><label className="sr-only" htmlFor="message-body">Contenido del mensaje</label><textarea id="message-body" value={body} onChange={(e) => setBody(e.target.value)} placeholder="Registra una propuesta, objeción o respuesta…" rows={3} maxLength={12000} required /><details className="evidence-composer"><summary>Añadir referencia de evidencia (opcional)</summary><div className="evidence-fields"><label htmlFor="evidence-label">Etiqueta<input id="evidence-label" value={evidenceLabel} onChange={(e) => setEvidenceLabel(e.target.value)} maxLength={120} placeholder="Informe de prueba" /></label><label htmlFor="evidence-locator">Localizador<input id="evidence-locator" value={evidenceLocator} onChange={(e) => setEvidenceLocator(e.target.value)} maxLength={2048} placeholder="docs/review.md#caso-1" /></label><label htmlFor="evidence-hash">SHA-256<input id="evidence-hash" value={evidenceHash} onChange={(e) => setEvidenceHash(e.target.value)} maxLength={64} placeholder="64 caracteres hexadecimales" /></label><p>CodeXPT valida el formato del hash; no descarga ni verifica el archivo.</p></div></details><div className="composer-foot"><span>Máximo 12.000 caracteres · No incluyas secretos</span><button className="primary-button" type="submit" disabled={busy || !body.trim()}><Send size={15} /> Registrar mensaje</button></div></form>
          </section>
          <aside className="annotation-rail" aria-label="Contexto y acciones">
            <section className="rail-section state-section"><div className="rail-label">Ciclo de revisión</div><ol className="state-track">{steps.map((step, index) => { const current = steps.indexOf(detail.task.status); return <li key={step} className={`${current === index ? 'current' : ''} ${index < current ? 'complete' : ''}`}><span className="track-mark">{index < current ? <Check size={12} /> : null}</span><span>{labels[step]}</span></li> })}</ol><div className="actions-list">{actions[detail.task.status].map((action) => <button key={action.next} type="button" className={action.next === 'approved' ? 'approve-button' : 'action-button'} disabled={busy || detail.task.pending_decision} onClick={() => transition(action.next, action.actor)}>{action.next === 'approved' ? <ShieldCheck size={15} /> : <ArrowUpRight size={15} />}{action.label}</button>)}</div>{detail.task.pending_decision && <p className="hold-note">El flujo se reanuda cuando respondas.</p>}<p className="identity-warning"><ShieldAlert size={14} /> La aprobación usa una etiqueta declarada, no una identidad verificada.</p></section>
            {!detail.pending_decision && detail.task.status !== 'approved' && <section className="rail-section question-section"><div className="rail-label"><Sparkles size={14} /> Pedir una decisión</div><form onSubmit={askHuman}><label htmlFor="human-question">Pregunta para César</label><textarea id="human-question" value={question} onChange={(e) => setQuestion(e.target.value)} placeholder="¿Qué opción debemos elegir?" rows={3} maxLength={2000} required /><button type="submit" className="action-button" disabled={busy || !question.trim()}>Solicitar respuesta</button></form></section>}
            <section className="rail-section details-section"><div className="rail-label">Anotaciones</div><dl><div><dt>Creada</dt><dd>{date(detail.task.created_at)}</dd></div><div><dt>Actualizada</dt><dd>{date(detail.task.updated_at)}</dd></div><div><dt>Eventos</dt><dd>{detail.events.length} registrados</dd></div></dl></section>
          </aside>
        </div>
      </> : <section className="first-run"><div className="first-run-mark"><BrandMark /></div><h1>El espacio está listo.</h1><p>Crea una revisión para registrar propuestas, respuestas y decisiones humanas en un historial local.</p><button type="button" className="primary-button" onClick={() => setDialog(true)}><Plus size={16} /> Crear revisión</button><div className="first-run-foot"><ShieldCheck size={15} /> Los datos se guardan en este equipo.</div></section>}
    </main>

    {dialog && <div className="dialog-backdrop" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget) setDialog(false) }}><section className="dialog" role="dialog" aria-modal="true" aria-labelledby="new-task-title"><div className="dialog-head"><div><span className="eyebrow">Nuevo registro</span><h2 id="new-task-title">Crear revisión</h2></div><button type="button" className="icon-button" aria-label="Cerrar" onClick={() => setDialog(false)}>×</button></div><form onSubmit={createTask}><label htmlFor="new-title">Título</label><input id="new-title" name="title" required maxLength={160} autoFocus placeholder="Ej. Evaluar diseño de autenticación" /><label htmlFor="new-description">Contexto <span>opcional</span></label><textarea id="new-description" name="description" maxLength={4000} rows={4} placeholder="Objetivo, alcance y criterios de revisión" /><div className="dialog-actions"><button type="button" className="quiet-button" onClick={() => setDialog(false)}>Cancelar</button><button type="submit" className="primary-button" disabled={busy}><Plus size={15} /> Crear revisión</button></div></form></section></div>}
  </div>
}

export default App
