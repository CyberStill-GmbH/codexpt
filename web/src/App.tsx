import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ArrowDownLeft, ArrowUpRight, Check, ChevronDown, CircleDot, Clock3, FileText, LockKeyhole, LogOut, Plus, Search, Send, ShieldAlert, ShieldCheck, Sparkles, UserRoundX, Users } from 'lucide-react'
import './App.css'

type Status = 'open' | 'proposed' | 'under_review' | 'resolved' | 'approved'
type Task = { task_id: string; title: string; description: string; status: Status; pending_decision: boolean; version: number; created_at: string; updated_at: string }
type Evidence = { label: string; locator: string; sha256: string }
type Message = { message_id: string; sender: string; recipient: string; body: string; evidence_refs: Evidence[]; created_at: string }
type Entry = { sequence: number; event_type: string; actor: string; occurred_at: string; details: Record<string, unknown> }
type Decision = { decision_id: string; question: string; requested_by: string; created_at: string }
type Detail = { task: Task; messages: Message[]; events: Entry[]; pending_decision: Decision | null }
type Member = { user_id: string; email: string | null; role: 'OWNER' | 'EDITOR' | 'VIEWER' }

function useAccessibleDialog(open: boolean, onClose: () => void, dialogRef: { current: HTMLElement | null }) {
  useEffect(() => {
    if (!open) return
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const dialog = dialogRef.current
    const focusable = (): HTMLElement[] => dialog ? Array.from(dialog.querySelectorAll<HTMLElement>('a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])')) : [];
    (dialog?.querySelector<HTMLElement>('[data-dialog-initial-focus]') ?? focusable()[0])?.focus()
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); onClose(); return }
      if (event.key !== 'Tab') return
      const items = [...focusable()]
      if (!items.length) { event.preventDefault(); return }
      const first = items[0]
      const last = items.at(-1)!
      if (event.shiftKey && (document.activeElement === first || !dialog?.contains(document.activeElement))) {
        event.preventDefault(); last.focus()
      } else if (!event.shiftKey && (document.activeElement === last || !dialog?.contains(document.activeElement))) {
        event.preventDefault(); first.focus()
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      if (previousFocus?.isConnected) previousFocus.focus()
    }
  }, [open, onClose, dialogRef])
}

const labels: Record<Status, string> = { open: 'Abierta', proposed: 'Propuesta', under_review: 'En revisión', resolved: 'Resuelta', approved: 'Aprobada' }
const API_BASE = import.meta.env.VITE_API_BASE_URL ?? ''
let activeWorkspaceId: string | null = null
let activeCsrfToken: string | null = null
const api = async <T,>(url: string, init?: RequestInit): Promise<T> => {
  const isMutation = Boolean(init?.method && init.method !== 'GET')
  const response = await fetch(`${API_BASE}${url}`, { credentials: 'include', ...init, headers: {
    ...(init?.body || isMutation ? { 'Content-Type': 'application/json' } : {}),
    ...(activeWorkspaceId && (url.startsWith('/api/tasks') || url.startsWith('/api/workspaces/')) ? { 'X-CodeXPT-Workspace-ID': activeWorkspaceId } : {}),
    ...(isMutation && activeCsrfToken ? { 'X-CSRF-Token': activeCsrfToken } : {}),
    ...init?.headers,
  } })
  const payload = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(payload.error ?? `La solicitud falló (${response.status})`)
  return payload as T
}
const date = (value: string) => new Intl.DateTimeFormat('es-PE', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
const actorLabel = (actor: string, authenticated = false) => {
  if (authenticated && actor.startsWith('human:')) return 'Usuario autenticado'
  return ({ 'agent:codex': 'Codex', 'agent:chatgpt': 'ChatGPT', 'human:cesar': 'César' } as Record<string, string>)[actor] ?? actor
}
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
    <circle cx="7.1" cy="6.7" r="3.2" className="panda-ear" />
    <circle cx="16.9" cy="6.7" r="3.2" className="panda-ear" />
    <path className="panda-face" d="M3.7 12.2c0-5.3 3.5-8.5 8.3-8.5s8.3 3.2 8.3 8.5v2.4c0 4.6-3.6 7.2-8.3 7.2s-8.3-2.6-8.3-7.2z" />
    <ellipse cx="8.7" cy="12.4" rx="2.1" ry="2.8" className="panda-patch" transform="rotate(24 8.7 12.4)" />
    <ellipse cx="15.3" cy="12.4" rx="2.1" ry="2.8" className="panda-patch" transform="rotate(-24 15.3 12.4)" />
    <circle cx="8.8" cy="12.1" r=".55" className="panda-eye" />
    <circle cx="15.2" cy="12.1" r=".55" className="panda-eye" />
    <path className="panda-muzzle" d="M10.2 15.1c.4-.7 1-1 1.8-1s1.4.3 1.8 1c-.1 1.1-.8 1.8-1.8 1.8s-1.7-.7-1.8-1.8z" />
    <path className="panda-mouth" d="M12 16.8v1m0 0c-.6.8-1.3.9-2 .4m2-.4c.6.8 1.3.9 2 .4" />
    <path className="panda-path panda-path-left" d="M5.1 19.2c.3 2.2 2.8 3.6 6.9 3.8" />
    <path className="panda-path panda-path-right" d="M18.9 19.2c-.3 2.2-2.8 3.6-6.9 3.8" />
    <circle cx="12" cy="23" r=".65" className="panda-node" />
  </svg>
}

function Login() {
  const [configured, setConfigured] = useState<boolean | null>(null)
  const [provider, setProvider] = useState('proveedor de identidad')
  const [signedIn, setSignedIn] = useState(false)
  useEffect(() => {
    let active = true
    api<{ configured: boolean; provider?: string }>('/api/auth/status')
      .then(async ({ configured: ready, provider: providerName }) => {
        if (!active) return
        if (providerName) setProvider(providerName)
        setConfigured(ready)
        if (ready) {
          try { await api('/api/auth/me'); if (active) setSignedIn(true) }
          catch { if (active) setSignedIn(false) }
        }
      })
      .catch(() => { if (active) setConfigured(false) })
    return () => { active = false }
  }, [])

  return <main className="login-shell">
    <section className="login-card" aria-labelledby="login-title">
      <a className="brand login-brand" href="/" aria-label="CodeXPT inicio"><BrandMark /><span className="brand-word">CodeXPT<span> / workspace</span></span></a>
      <div className="login-rule" />
      <p className="login-eyebrow"><span className="login-dot" /> COLABORACIÓN TÉCNICA AUDITABLE</p>
      <h1 id="login-title">Las buenas decisiones<br />se revisan en equipo.</h1>
      <p className="login-copy">Un espacio claro para propuestas, revisión entre agentes y decisiones humanas con evidencia.</p>
      <div className="login-auth">
        {signedIn ? <a className="primary-button login-button" href="/">Ir al workspace <ArrowUpRight size={16} /></a> : <a className={`primary-button login-button ${configured === false ? 'disabled' : ''}`} href={configured ? `${API_BASE}/api/auth/login` : undefined} aria-disabled={!configured}>
          <LockKeyhole size={15} /> Continuar con {provider === 'ChatGPT' ? 'ChatGPT' : 'inicio seguro'} <ArrowUpRight size={15} />
        </a>}
        <p className="login-provider">{configured === null ? 'Comprobando conexión de identidad…' : configured ? provider === 'ChatGPT' ? 'OpenAI verifica tu identidad; CodeXPT administra tus permisos y workspace.' : `Identidad verificada por ${provider} · permisos administrados por CodeXPT.` : 'La conexión de identidad se habilitará al completar la configuración del entorno.'}</p>
        <a className="login-invite-link" href="/invite">¿Tienes una invitación? Aceptarla</a>
      </div>
      <div className="login-foot"><ShieldCheck size={15} /><span>Acceso privado, decisiones trazables y roles por workspace.</span></div>
    </section>
    <aside className="login-aside" aria-label="Principios de CodeXPT">
      <div className="aside-top"><span>CODEXPT / PRINCIPIOS</span><span>01 — 03</span></div>
      <div className="aside-content">
        <p className="aside-index">01 / PROPUESTA</p><h2>Las ideas empiezan<br />con evidencia.</h2>
        <div className="aside-line" />
        <p className="aside-index">02 / REVISIÓN</p><h2>El desacuerdo<br />también construye.</h2>
        <div className="aside-line" />
        <p className="aside-index">03 / DECISIÓN</p><h2>La aprobación<br />siempre es humana.</h2>
      </div>
      <div className="aside-bottom"><span>Hecho para pensar en conjunto.</span><span>v0.2 · identity</span></div>
    </aside>
  </main>
}

function InviteAcceptance() {
  const [configured, setConfigured] = useState<boolean | null>(null)
  const [signedIn, setSignedIn] = useState(false)
  const [token, setToken] = useState('')
  const [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let active = true
    api<{ configured: boolean }>('/api/auth/status').then(async ({ configured: ready }) => {
      if (!active) return
      setConfigured(ready)
      if (!ready) return
      try {
        const session = await api<{ csrf_token: string }>('/api/auth/me')
        activeCsrfToken = session.csrf_token
        if (active) setSignedIn(true)
      } catch { if (active) setSignedIn(false) }
    }).catch(() => { if (active) setConfigured(false) })
    return () => { active = false }
  }, [])

  const accept = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!token.trim()) return
    setBusy(true); setMessage('')
    try {
      const result = await api<{ workspace_id: string }>('/api/invites/accept', { method: 'POST', body: JSON.stringify({ token: token.trim() }) })
      activeWorkspaceId = result.workspace_id
      setToken('')
      window.location.assign('/')
    } catch (error) { setMessage(error instanceof Error ? error.message : 'No se pudo aceptar la invitación.') }
    finally { setBusy(false) }
  }

  return <main className="login-shell invite-shell">
    <section className="login-card" aria-labelledby="invite-title">
      <a className="brand login-brand" href="/login" aria-label="CodeXPT acceso"><BrandMark /><span className="brand-word">CodeXPT<span> / invitación</span></span></a>
      <div className="login-rule" />
      <p className="login-eyebrow"><span className="login-dot" /> ACCESO POR INVITACIÓN</p>
      <h1 id="invite-title">Entra a un espacio<br />de trabajo compartido.</h1>
      {!configured ? <><p className="login-copy">{configured === null ? 'Comprobando conexión de identidad…' : 'La conexión de identidad aún no está configurada.'}</p><a className="login-invite-link invite-return-link" href="/login">Volver al inicio seguro</a></> : !signedIn ? <div className="login-auth"><p className="login-copy">Primero verifica tu identidad; luego podrás aceptar el código enviado por quien administra el workspace.</p><a className="primary-button login-button" href="/login">Continuar al inicio seguro <ArrowUpRight size={15} /></a></div> : <form className="invite-form" onSubmit={accept}>
        <p className="login-copy">Confirma que iniciaste sesión con el correo al que se envió la invitación y pega el código recibido.</p>
        <label htmlFor="invite-token">Código de invitación</label>
        <input id="invite-token" value={token} onChange={(event) => setToken(event.target.value)} autoComplete="off" spellCheck={false} maxLength={128} required />
        {message && <p className="invite-error" role="alert">{message}</p>}
        <button className="primary-button login-button" type="submit" disabled={busy}>{busy ? 'Validando…' : 'Aceptar invitación'} <ArrowUpRight size={15} /></button>
      </form>}
      <div className="login-foot"><ShieldCheck size={15} /><span>El código es de un solo uso y caduca a los siete días.</span></div>
    </section>
      <aside className="login-aside invite-aside"><div className="aside-top"><span>CODEXPT / WORKSPACE</span><span>PRIVATE</span></div><div className="aside-content"><p className="aside-index">IDENTIDAD VERIFICADA</p><h2>Tu acceso depende<br />de una invitación activa<br />y tu correo verificado.</h2><div className="aside-line" /><p className="aside-index">PERMISOS POR ROL</p><h2>El Owner administra<br />quién puede participar.</h2></div><div className="aside-bottom"><span>Decisiones humanas, evidencia visible.</span><span>CodeXPT</span></div></aside>
  </main>
}

function Dashboard() {
  const [tasks, setTasks] = useState<Task[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [detail, setDetail] = useState<Detail | null>(null)
  const [filter, setFilter] = useState<'all' | 'pending' | 'approved'>('all')
  const [search, setSearch] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [statusMessage, setStatusMessage] = useState('')
  const [dialog, setDialog] = useState(false)
  const [membersDialog, setMembersDialog] = useState(false)
  const newTaskDialogRef = useRef<HTMLElement>(null)
  const membersDialogRef = useRef<HTMLElement>(null)
  const messageComposerRef = useRef<HTMLTextAreaElement>(null)
  const decisionAnswerRef = useRef<HTMLInputElement>(null)
  const closeTaskDialog = useCallback(() => setDialog(false), [])
  const closeMembersDialog = useCallback(() => setMembersDialog(false), [])
  useAccessibleDialog(dialog, closeTaskDialog, newTaskDialogRef)
  useAccessibleDialog(membersDialog, closeMembersDialog, membersDialogRef)
  const [members, setMembers] = useState<Member[]>([])
  const [inviteEmail, setInviteEmail] = useState('')
  const [inviteRole, setInviteRole] = useState<'EDITOR' | 'VIEWER'>('EDITOR')
  const [inviteToken, setInviteToken] = useState('')
  const [inviteError, setInviteError] = useState('')
  const [memberError, setMemberError] = useState('')
  const [confirmingRemoval, setConfirmingRemoval] = useState<string | null>(null)
  const [hosted, setHosted] = useState(false)
  const [workspaceRole, setWorkspaceRole] = useState('')
  const [profileEmail, setProfileEmail] = useState('')
  const [body, setBody] = useState('')
  const [sender, setSender] = useState('agent:codex')
  const [recipient, setRecipient] = useState('agent:chatgpt')
  const [question, setQuestion] = useState('')
  const [answer, setAnswer] = useState('')
  const [evidenceLabel, setEvidenceLabel] = useState('')
  const [evidenceLocator, setEvidenceLocator] = useState('')
  const [evidenceHash, setEvidenceHash] = useState('')

  useEffect(() => {
    const onShortcut = (event: KeyboardEvent) => {
      if (event.key !== '/' || event.ctrlKey || event.metaKey || event.altKey || event.target instanceof HTMLElement && /^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName)) return
      const input = [...document.querySelectorAll<HTMLInputElement>('[data-review-search]')].find((candidate) => candidate.getClientRects().length > 0)
      if (input) { event.preventDefault(); input.focus() }
    }
    window.addEventListener('keydown', onShortcut)
    return () => window.removeEventListener('keydown', onShortcut)
  }, [])


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
  useEffect(() => {
    let active = true
    api<{ configured: boolean }>('/api/auth/status')
      .then(async ({ configured }) => {
        if (configured) {
          try {
            const session = await api<{ csrf_token: string; email?: string }>('/api/auth/me')
            activeCsrfToken = session.csrf_token
            if (active) { setHosted(true); setProfileEmail(session.email ?? '') }
            const workspaces = await api<Array<{ workspace_id: string; name: string; role: string }>>('/api/workspaces')
            if (!workspaces.length) {
              if (active) setError('Tu acceso ya está verificado. Un Owner debe invitarte a un workspace antes de que puedas usar la consola.')
              return
            }
            activeWorkspaceId = workspaces[0].workspace_id
            if (active) setWorkspaceRole(workspaces[0].role)
          }
          catch { if (active) window.location.replace('/login'); return }
        }
        else if (active) setHosted(false)
        if (active) loadTasks().catch((e: unknown) => setError(e instanceof Error ? e.message : 'No se pudo cargar el espacio.'))
      })
      .catch((e: unknown) => { if (active) setError(e instanceof Error ? e.message : 'No se pudo cargar el espacio.') })
    return () => { active = false }
  }, [loadTasks])
  const filtered = useMemo(() => tasks.filter((task) => {
    const term = `${task.title} ${task.description}`.toLocaleLowerCase()
    return term.includes(search.toLocaleLowerCase()) && (filter === 'all' || (filter === 'pending' ? task.pending_decision : task.status === 'approved'))
  }), [tasks, search, filter])
  const activity = useMemo(() => detail ? [
    ...detail.messages.map((message) => {
      const source = detail.events.find((event) => event.event_type === 'message_recorded' && event.details.message_id === message.message_id)
      return { id: message.message_id, date: message.created_at, kind: 'message' as const, actor: message.sender, recipient: message.recipient, text: message.body, evidence: message.evidence_refs, verified: Boolean(source?.details.verified_human) }
    }),
    ...detail.events.filter((event) => event.event_type !== 'message_recorded').map((event) => ({ id: `event-${event.sequence}`, date: event.occurred_at, kind: 'event' as const, actor: event.actor, recipient: null, text: eventLabel(event), extra: event.details, verified: Boolean(event.details.verified_human) })),
  ].sort((a, b) => a.date.localeCompare(b.date) || a.id.localeCompare(b.id)) : [], [detail])

  const mutate = async (operation: () => Promise<unknown>, preferred?: string) => {
    setBusy(true); setError(''); setStatusMessage('')
    try { const result = await operation(); await loadTasks(typeof result === 'string' ? result : preferred); setStatusMessage('Cambios guardados en el historial.') }
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
    void mutate(async () => { await api(`/api/tasks/${detail.task.task_id}/messages`, { method: 'POST', body: JSON.stringify({ sender: hosted ? 'human:authenticated-session' : sender, recipient, body: body.trim(), evidence_refs }) }); setBody(''); setEvidenceLabel(''); setEvidenceLocator(''); setEvidenceHash('') })
  }
  const transition = (next: Status, actor: string) => {
    if (!detail) return
    void mutate(() => api(`/api/tasks/${detail.task.task_id}/transition`, { method: 'POST', body: JSON.stringify({ expected: detail.task.status, next, actor, expected_version: detail.task.version }) }))
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

  const openMembers = async () => {
    if (!activeWorkspaceId) return
    setInviteError(''); setMemberError(''); setInviteToken(''); setConfirmingRemoval(null); setMembersDialog(true)
    try { setMembers(await api<Member[]>(`/api/workspaces/${activeWorkspaceId}/members`)) }
    catch (e) { setMemberError(e instanceof Error ? e.message : 'No se pudieron cargar los miembros.') }
  }
  const changeMemberRole = async (member: Member, role: 'EDITOR' | 'VIEWER') => {
    if (!activeWorkspaceId || member.role === role) return
    setBusy(true); setMemberError('')
    try {
      await api(`/api/workspaces/${activeWorkspaceId}/members/${member.user_id}`, { method: 'PATCH', body: JSON.stringify({ role }) })
      setMembers((current) => current.map((item) => item.user_id === member.user_id ? { ...item, role } : item))
      setStatusMessage(`Rol actualizado para ${member.email ?? 'el miembro'}.`)
    } catch (e) { setMemberError(e instanceof Error ? e.message : 'No se pudo cambiar el rol.') }
    finally { setBusy(false) }
  }
  const revokeMember = async (member: Member) => {
    if (!activeWorkspaceId) return
    setBusy(true); setMemberError('')
    try {
      await api(`/api/workspaces/${activeWorkspaceId}/members/${member.user_id}`, { method: 'DELETE' })
      setMembers((current) => current.filter((item) => item.user_id !== member.user_id))
      setConfirmingRemoval(null)
      setStatusMessage(`Se revocó el acceso de ${member.email ?? 'el miembro'}.`)
    } catch (e) { setMemberError(e instanceof Error ? e.message : 'No se pudo revocar el acceso.') }
    finally { setBusy(false) }
  }
  const createInvite = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!activeWorkspaceId || !inviteEmail.trim()) return
    setBusy(true); setInviteError('')
    try {
      const result = await api<{ invite_id: string; raw_token: string; expires_at: string }>(`/api/workspaces/${activeWorkspaceId}/invites`, { method: 'POST', body: JSON.stringify({ email: inviteEmail.trim(), role: inviteRole }) })
      setInviteToken(result.raw_token); setInviteEmail('')
    } catch (e) { setInviteError(e instanceof Error ? e.message : 'No se pudo crear la invitación.') }
    finally { setBusy(false) }
  }

  const actions: Record<Status, Array<{ next: Status; actor: string; label: string }>> = {
    open: [{ next: 'proposed', actor: 'agent:codex', label: 'Registrar propuesta' }],
    proposed: [{ next: 'under_review', actor: 'agent:chatgpt', label: 'Iniciar revisión' }],
    under_review: [{ next: 'proposed', actor: 'agent:codex', label: 'Devolver con objeción' }, { next: 'resolved', actor: 'agent:chatgpt', label: 'Marcar como resuelta' }],
    resolved: [{ next: 'approved', actor: 'human:cesar', label: 'Aprobar decisión' }], approved: [],
  }
  const canCollaborate = !hosted || workspaceRole === 'OWNER' || workspaceRole === 'EDITOR'
  const canApprove = !hosted || workspaceRole === 'OWNER'
  const focusNextAction = () => {
    const target = detail?.pending_decision ? decisionAnswerRef.current : messageComposerRef.current
    target?.scrollIntoView({ behavior: 'smooth', block: 'center' })
    target?.focus({ preventScroll: true })
  }
  const mobileActionLabel = detail?.pending_decision ? 'Responder decisión' : detail?.task.status === 'approved' ? 'Añadir comentario' : 'Responder'

  return <div className="app-shell">
    <aside className="sidebar" aria-label="Navegación de tareas">
      <a className="brand" href="#inicio" aria-label="CodeXPT, inicio"><BrandMark /><span className="brand-word">Code<span>XPT</span></span></a>
      <div className="local-tag"><span className="local-dot" /> {hosted ? `Espacio privado · ${workspaceRole || 'miembro'}` : 'Espacio local'}</div>
      <div className="side-heading"><span>Revisiones</span>{canCollaborate && <button className="icon-button" type="button" aria-label="Crear revisión" onClick={() => setDialog(true)}><Plus size={17} /></button>}</div>
      <label className="search-box desktop-review-search"><Search size={16} aria-hidden="true" /><input data-review-search type="search" aria-label="Buscar revisiones" placeholder="Buscar" value={search} onChange={(e) => setSearch(e.target.value)} /><kbd>/</kbd></label>
      <div className="filter-tabs desktop-review-filters" role="group" aria-label="Filtrar revisiones"><button type="button" aria-pressed={filter === 'all'} className={filter === 'all' ? 'selected' : ''} onClick={() => setFilter('all')}>Todas</button><button type="button" aria-pressed={filter === 'pending'} className={filter === 'pending' ? 'selected' : ''} onClick={() => setFilter('pending')}>Pendientes</button><button type="button" aria-pressed={filter === 'approved'} className={filter === 'approved' ? 'selected' : ''} onClick={() => setFilter('approved')}>Aprobadas</button></div>
      <div className="mobile-review-controls">
        <label className="search-box mobile-review-search"><Search size={16} aria-hidden="true" /><input data-review-search type="search" aria-label="Buscar revisiones" placeholder="Buscar revisiones" value={search} onChange={(e) => setSearch(e.target.value)} /></label>
        <div className="filter-tabs" role="group" aria-label="Filtrar revisiones"><button type="button" aria-pressed={filter === 'all'} className={filter === 'all' ? 'selected' : ''} onClick={() => setFilter('all')}>Todas</button><button type="button" aria-pressed={filter === 'pending'} className={filter === 'pending' ? 'selected' : ''} onClick={() => setFilter('pending')}>Pendientes</button><button type="button" aria-pressed={filter === 'approved'} className={filter === 'approved' ? 'selected' : ''} onClick={() => setFilter('approved')}>Aprobadas</button></div>
      </div>
      <nav className="task-list" aria-label="Lista de revisiones">
        {filtered.map((task) => <button key={task.task_id} type="button" className={`task-row ${task.task_id === selected ? 'active' : ''}`} onClick={() => { setSelected(task.task_id); loadDetail(task.task_id).catch((e: Error) => setError(e.message)) }}><span className={`status-marker ${task.status}`} aria-hidden="true" /><span className="task-row-copy"><strong>{task.title}</strong><small>{task.pending_decision ? 'Decisión pendiente' : labels[task.status]}</small></span>{task.pending_decision && <span className="pending-mark" aria-label="Decisión pendiente">!</span>}</button>)}
        {!filtered.length && <p className="empty-list">{tasks.length ? 'No hay revisiones con ese filtro.' : 'Aquí aparecerán tus revisiones.'}</p>}
      </nav>
      <div className="sidebar-foot"><div className="security-note"><ShieldCheck size={16} /><span>{hosted ? 'Sesión privada' : 'Solo en este equipo'}<br /><small>{hosted ? 'Acceso por invitación' : 'Servidor en loopback'}</small></span></div><div className="profile-row"><span className="avatar">{hosted ? (profileEmail[0]?.toUpperCase() ?? 'U') : 'C'}</span><span><strong>{hosted ? (profileEmail || 'Usuario') : 'César'}</strong><small>{hosted ? workspaceRole : 'Director técnico'}</small></span>{hosted ? <button className="icon-button logout-button" type="button" aria-label="Cerrar sesión" onClick={() => { void api('/api/auth/logout', { method: 'POST' }).finally(() => { activeCsrfToken = null; activeWorkspaceId = null; window.location.replace('/login') }) }}><LogOut size={14} /></button> : <ChevronDown size={15} />}</div></div>
    </aside>

    <main className="workspace" id="inicio">
      <header className="topbar"><div className="breadcrumbs"><span>CodeXPT</span><span className="crumb-sep">/</span><strong>{detail?.task.title ?? 'Revisiones'}</strong></div><div className="top-actions"><span className="connection"><span className="local-dot" /> {hosted ? 'Privado' : 'Local'}</span>{canCollaborate && detail && <button className="primary-button mobile-composer-action" type="button" onClick={focusNextAction}><Send size={14} /> {mobileActionLabel}</button>}{hosted && workspaceRole === 'OWNER' && <button className="quiet-button" type="button" aria-label="Miembros" onClick={() => void openMembers()}><Users size={15} /><span>Miembros</span></button>}</div></header>
      <p className="sr-only" role="status" aria-live="polite">{statusMessage}</p>
      {error && <div className="error-banner" role="alert"><ShieldAlert size={17} /><span>{error}</span><button type="button" aria-label="Cerrar error" onClick={() => setError('')}>×</button></div>}
      {error.startsWith('Tu acceso ya está verificado.') ? <section className="first-run"><div className="first-run-mark"><BrandMark /></div><h1>Tu workspace aún no está listo.</h1><p>{error}</p><a className="primary-button" href="/invite">Aceptar invitación</a><button type="button" className="quiet-button" onClick={() => { void api('/api/auth/logout', { method: 'POST' }).finally(() => { activeCsrfToken = null; activeWorkspaceId = null; window.location.replace('/login') }) }}>Cerrar sesión</button></section> : detail ? <>
        <section className="review-heading"><div className="heading-main"><div className="eyebrow"><FileText size={14} /> Registro de revisión <span>/{detail.task.task_id.slice(0, 8)}</span></div><h1>{detail.task.title}</h1><p>{detail.task.description || 'Sin descripción. Añade contexto a través de la conversación.'}</p></div><span className={`status-pill ${detail.task.status}`}><span className="status-dot" />{labels[detail.task.status]}</span></section>
        {detail.pending_decision && <section className="decision-banner" aria-labelledby="decision-title"><div className="decision-icon"><CircleDot size={20} /></div><div className="decision-question"><span id="decision-title">Tu decisión es necesaria</span><p>{detail.pending_decision.question}</p><small>Solicitada por {detail.pending_decision.requested_by} · {date(detail.pending_decision.created_at)}</small></div>{canCollaborate ? <form className="decision-answer" onSubmit={answerHuman}><label className="sr-only" htmlFor="decision-answer">Tu respuesta</label><input ref={decisionAnswerRef} id="decision-answer" value={answer} onChange={(e) => setAnswer(e.target.value)} placeholder="Escribe una respuesta" required /><button className="primary-button" type="submit" disabled={busy || !answer.trim()}>Enviar respuesta</button></form> : <p className="read-only-note">Tu rol permite consultar esta decisión, pero no responderla.</p>}</section>}
        <div className="work-grid">
          <section className="activity-panel" aria-labelledby="activity-title"><div className="section-heading"><div><h2 id="activity-title">Cronología</h2><span>{detail.events.filter((event) => event.event_type !== 'message_recorded').length} cambios registrados · {detail.messages.length} mensajes de conversación</span></div><button className="quiet-button compact" type="button" onClick={() => loadDetail(detail.task.task_id)}><Clock3 size={15} /> Actualizar</button></div>
            {activity.length ? <ol className="timeline">{activity.map((item) => <li key={item.id} className={`timeline-entry ${item.kind === 'event' ? 'audit-entry' : 'message-entry'}`}><span className={`timeline-marker ${item.actor.startsWith('human:') || item.actor.includes('cesar') ? 'human' : item.actor.includes('chatgpt') ? 'chatgpt' : 'codex'}`} aria-hidden="true" /><div className="entry-meta"><strong>{actorLabel(item.actor, hosted)}</strong><span className={`entry-kind ${item.kind}`}>{item.kind === 'event' ? 'Registro de auditoría' : 'Mensaje de conversación'}</span>{item.recipient && <span><ArrowUpRight size={13} /> {actorLabel(item.recipient)}</span>}<time>{date(item.date)}</time></div>{item.kind === 'message' ? <><p className="entry-body">{item.text}</p>{item.evidence.length > 0 && <details className="evidence-list"><summary>{item.evidence.length} referencia{item.evidence.length === 1 ? '' : 's'} de evidencia</summary><ul>{item.evidence.map((ref, index) => <li key={`${ref.sha256}-${index}`}><strong>{ref.label}</strong><span>{ref.locator}</span><code>SHA-256 · {ref.sha256}</code></li>)}</ul></details>}</> : <p className="entry-event">{item.text}{item.extra && typeof item.extra.from === 'string' && <span> · desde {labels[item.extra.from as Status] ?? item.extra.from}</span>}</p>}<div className="entry-provenance"><ShieldAlert size={13} />{hosted && item.verified ? 'Sesión autenticada · autor registrado por el servidor' : 'Actor declarado · origen no autenticado'}</div></li>)}</ol> : <div className="empty-conversation"><div className="empty-glyph"><ArrowDownLeft size={19} /></div><strong>El registro empieza aquí</strong><p>Añade una propuesta o respuesta de una sesión participante. CodeXPT registra quién la declara, sin verificar su identidad.</p></div>}
            {canCollaborate ? <form className="composer" onSubmit={submitMessage}><div className="composer-meta">{hosted ? <><span className="composer-meta-label">Autor autenticado</span><span className="authenticated-author"><ShieldCheck size={14} /> {profileEmail || 'Sesión verificada'}</span></> : <><label htmlFor="message-sender">Autor declarado</label><select id="message-sender" value={sender} onChange={(e) => setSender(e.target.value)}><option value="agent:codex">Codex</option><option value="agent:chatgpt">ChatGPT</option><option value="human:cesar">César</option></select></>}<label htmlFor="message-recipient">Para</label><select id="message-recipient" value={recipient} onChange={(e) => setRecipient(e.target.value)}><option value="agent:chatgpt">ChatGPT</option><option value="agent:codex">Codex</option><option value="human:cesar">César</option></select></div><label className="sr-only" htmlFor="message-body">Contenido del mensaje</label><textarea ref={messageComposerRef} id="message-body" value={body} onChange={(e) => setBody(e.target.value)} placeholder="Registra una propuesta, objeción o respuesta…" rows={3} maxLength={12000} required /><details className="evidence-composer"><summary>Añadir referencia de evidencia (opcional)</summary><div className="evidence-fields"><label htmlFor="evidence-label">Etiqueta<input id="evidence-label" value={evidenceLabel} onChange={(e) => setEvidenceLabel(e.target.value)} maxLength={120} placeholder="Informe de prueba" /></label><label htmlFor="evidence-locator">Localizador<input id="evidence-locator" value={evidenceLocator} onChange={(e) => setEvidenceLocator(e.target.value)} maxLength={2048} placeholder="docs/review.md#caso-1" /></label><label htmlFor="evidence-hash">SHA-256<input id="evidence-hash" value={evidenceHash} onChange={(e) => setEvidenceHash(e.target.value)} maxLength={64} placeholder="64 caracteres hexadecimales" /></label><p>CodeXPT valida el formato del hash; no descarga ni verifica el archivo.</p></div></details><div className="composer-foot"><span>Máximo 12.000 caracteres · No incluyas secretos</span><button className="primary-button" type="submit" disabled={busy || !body.trim()}><Send size={15} /> Registrar mensaje</button></div></form> : <p className="read-only-note">Tu rol permite consultar el historial, pero no registrar mensajes.</p>}
          </section>
          <aside className="annotation-rail" aria-label="Contexto y acciones">
            <section className="rail-section state-section"><div className="rail-label">Ciclo de revisión</div><ol className="state-track">{steps.map((step, index) => { const current = steps.indexOf(detail.task.status); return <li key={step} className={`${current === index ? 'current' : ''} ${index < current ? 'complete' : ''}`}><span className="track-mark">{index < current ? <Check size={12} /> : null}</span><span>{labels[step]}</span></li> })}</ol>{canCollaborate && <div className="actions-list">{actions[detail.task.status].filter((action) => action.next !== 'approved' || canApprove).map((action) => <button key={action.next} type="button" className={action.next === 'approved' ? 'approve-button' : 'action-button'} disabled={busy || detail.task.pending_decision} onClick={() => transition(action.next, action.actor)}>{action.next === 'approved' ? <ShieldCheck size={15} /> : <ArrowUpRight size={15} />}{action.label}</button>)}</div>}{detail.task.pending_decision && <p className="hold-note">El flujo se reanuda cuando respondas.</p>}<p className="identity-warning"><ShieldAlert size={14} />{hosted ? canApprove ? 'El servidor verifica tu sesión y permiso Owner antes de aprobar.' : 'Solo un Owner puede aprobar decisiones.' : 'La aprobación usa una etiqueta declarada, no una identidad verificada.'}</p></section>
            {canCollaborate && !detail.pending_decision && detail.task.status !== 'approved' && <section className="rail-section question-section"><div className="rail-label"><Sparkles size={14} /> Pedir una decisión</div><form onSubmit={askHuman}><label htmlFor="human-question">Pregunta para César</label><textarea id="human-question" value={question} onChange={(e) => setQuestion(e.target.value)} placeholder="¿Qué opción debemos elegir?" rows={3} maxLength={2000} required /><button type="submit" className="action-button" disabled={busy || !question.trim()}>Solicitar respuesta</button></form></section>}
            <section className="rail-section details-section"><div className="rail-label">Anotaciones</div><dl><div><dt>Creada</dt><dd>{date(detail.task.created_at)}</dd></div><div><dt>Actualizada</dt><dd>{date(detail.task.updated_at)}</dd></div><div><dt>Eventos</dt><dd>{detail.events.length} registrados</dd></div></dl></section>
          </aside>
        </div>
      </> : <section className="first-run"><div className="first-run-mark"><BrandMark /></div><h1>El espacio está listo.</h1><p>{canCollaborate ? 'Crea una revisión para registrar propuestas, respuestas y decisiones humanas en un historial local.' : 'Tu rol permite consultar el workspace. Pide a un Owner que te habilite para registrar revisiones.'}</p>{canCollaborate && <button type="button" className="primary-button" onClick={() => setDialog(true)}><Plus size={16} /> Crear revisión</button>}<div className="first-run-foot"><ShieldCheck size={15} /> {hosted ? 'Acceso privado por workspace.' : 'Los datos se guardan en este equipo.'}</div></section>}
    </main>

    {dialog && <div className="dialog-backdrop" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget) setDialog(false) }}><section ref={newTaskDialogRef} className="dialog" role="dialog" aria-modal="true" aria-labelledby="new-task-title"><div className="dialog-head"><div><span className="eyebrow">Nuevo registro</span><h2 id="new-task-title">Crear revisión</h2></div><button type="button" className="icon-button" aria-label="Cerrar" onClick={() => setDialog(false)}>×</button></div><form onSubmit={createTask}><label htmlFor="new-title">Título</label><input id="new-title" name="title" data-dialog-initial-focus required maxLength={160} placeholder="Ej. Evaluar diseño de autenticación" /><label htmlFor="new-description">Contexto <span>opcional</span></label><textarea id="new-description" name="description" maxLength={4000} rows={4} placeholder="Objetivo, alcance y criterios de revisión" /><div className="dialog-actions"><button type="button" className="quiet-button" onClick={() => setDialog(false)}>Cancelar</button><button type="submit" className="primary-button" disabled={busy}><Plus size={15} /> Crear revisión</button></div></form></section></div>}
    {membersDialog && <div className="dialog-backdrop" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget) setMembersDialog(false) }}><section ref={membersDialogRef} className="dialog members-dialog" role="dialog" aria-modal="true" aria-labelledby="members-title"><div className="dialog-head"><div><span className="eyebrow">Workspace · Acceso privado</span><h2 id="members-title">Miembros</h2></div><button type="button" className="icon-button" aria-label="Cerrar" onClick={() => setMembersDialog(false)}>×</button></div><p className="members-intro">Administra quién puede colaborar y con qué nivel de acceso.</p>{memberError && <p className="invite-error" role="alert">{memberError}</p>}<ul className="member-list">{members.map((member) => { const isCurrentUser = member.email?.toLocaleLowerCase() === profileEmail.toLocaleLowerCase(); const isOwner = member.role === 'OWNER'; const canManage = !isOwner && !isCurrentUser; return <li key={member.user_id}><span className="member-avatar" aria-hidden="true">{member.email?.[0]?.toUpperCase() ?? 'U'}</span><span className="member-copy"><strong title={member.email ?? undefined}>{member.email ?? 'Correo privado'}{isCurrentUser ? ' · Tú' : ''}</strong><small>{isOwner ? 'Owner · administra el workspace' : member.role === 'EDITOR' ? 'Editor · puede colaborar' : 'Viewer · solo lectura'}</small></span>{canManage && <div className="member-actions">{confirmingRemoval === member.user_id ? <><span className="member-confirm-copy">Revocar acceso a esta persona?</span><button type="button" className="quiet-button member-confirm" disabled={busy} onClick={() => void revokeMember(member)}>Confirmar</button><button type="button" className="quiet-button member-cancel" disabled={busy} onClick={() => setConfirmingRemoval(null)}>Cancelar</button></> : <><label className="sr-only" htmlFor={`member-role-${member.user_id}`}>Rol de {member.email ?? 'miembro'}</label><select id={`member-role-${member.user_id}`} value={member.role} disabled={busy} onChange={(event) => void changeMemberRole(member, event.target.value as 'EDITOR' | 'VIEWER')}><option value="EDITOR">Editor</option><option value="VIEWER">Viewer</option></select><button type="button" className="member-remove" aria-label={`Revocar acceso a ${member.email ?? 'miembro'}`} disabled={busy} onClick={() => setConfirmingRemoval(member.user_id)}><UserRoundX size={15} /></button></>}</div>}</li> })}</ul><form onSubmit={createInvite}><label htmlFor="invite-email">Invitar por correo verificado</label><input id="invite-email" type="email" autoComplete="email" value={inviteEmail} onChange={(e) => setInviteEmail(e.target.value)} maxLength={320} required placeholder="persona@equipo.com" /><label htmlFor="invite-role">Rol inicial</label><select id="invite-role" value={inviteRole} onChange={(e) => setInviteRole(e.target.value as 'EDITOR' | 'VIEWER')}><option value="EDITOR">Editor · puede colaborar</option><option value="VIEWER">Viewer · solo lectura</option></select>{inviteError && <p className="invite-error" role="alert">{inviteError}</p>}{inviteToken && <div className="invite-token"><strong>Comparte este código de un solo uso</strong><code>{inviteToken}</code><small>La persona debe iniciar sesión con el correo invitado y pegar el código en /invite. El código no se volverá a mostrar.</small></div>}<div className="dialog-actions"><button type="button" className="quiet-button" onClick={() => setMembersDialog(false)}>Cerrar</button><button type="submit" className="primary-button" disabled={busy || !inviteEmail.trim()}><Users size={15} /> Crear invitación</button></div></form></section></div>}
  </div>
}

function App() {
  if (window.location.pathname === '/login') return <Login />
  if (window.location.pathname === '/invite') return <InviteAcceptance />
  return <Dashboard />
}

export default App
