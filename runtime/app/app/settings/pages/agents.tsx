// Settings → Agents: each agent pumi knows and its hook (how it tells pumi it is working, waiting
// on you or done), installed, brought up to date or removed with a click; then the agents' options.
import { useCallback, useEffect, useState } from "react"
import { confirm } from "../../../sdk/dialogs.ts"
import { Button, Note, Page, Row, Section, StatusDot, useSettings } from "../../../sdk/settings/index.ts"
import { useTheme } from "../../../sdk/theme.ts"
import { AGENTS as AGENTS_PLUGIN } from "../pages.ts"
import { hookState, installHooks, listAgents, removeHooks, type SkillAgent } from "../skills.ts"
import { GroupedOptions } from "../shared.tsx"

function useAgentHooks(): { list?: SkillAgent[]; error?: string; reload(): void } {
  const [got, setGot] = useState<{ list?: SkillAgent[]; error?: string }>({})
  const reload = useCallback(() => {
    listAgents().then(
      (list) => setGot({ list }),
      (err: Error) => setGot((was) => ({ ...was, error: String(err?.message ?? err) })),
    )
  }, [])
  useEffect(reload, [reload])
  return { ...got, reload }
}

function AgentRow({ a, busy, run }: { a: SkillAgent; busy: string | undefined; run(id: string, doing: () => Promise<void>, done: string): void }) {
  const t = useTheme()
  const { text, action } = hookState(a)
  const color = a.hook === "installed" ? t.success : a.hook === "outdated" ? t.warning : t.faint
  const remove = async () => {
    const yes = await confirm({ title: `Remove ${a.label}'s hook?`, confirm: "Remove", danger: true })
    if (yes) run(a.id, () => removeHooks([a.id]), "Removed.")
  }
  const mine = busy === a.id
  return (
    <Row
      link={`agent-${a.id}`}
      testId={`agent-${a.id}`}
      icon={<StatusDot color={color} />}
      title={a.label}
      aside={text}
      control={
        <>
          {action ? (
            <Button primary={action === "update" || a.available} loading={mine} disabled={!!busy} onClick={() => run(a.id, () => installHooks([a.id]), `Done. Restart ${a.label} to apply.`)} testId={`agent-${action}-${a.id}`}>
              {action === "update" ? "Update" : "Install"}
            </Button>
          ) : null}
          {a.hook === "installed" || a.hook === "outdated" ? <Button ghost icon="close" title="Remove" disabled={!!busy} onClick={remove} testId={`agent-remove-${a.id}`} /> : null}
        </>
      }
    />
  )
}

export function AgentsPage() {
  const settings = useSettings()
  const { list, error, reload } = useAgentHooks()
  const [busy, setBusy] = useState<string>()
  const [status, setStatus] = useState<{ text: string; error?: boolean }>()
  const run = (id: string, doing: () => Promise<void>, done: string) => {
    setBusy(id)
    setStatus(undefined)
    doing()
      .then(
        () => setStatus({ text: done }),
        (err: Error) => setStatus({ text: String(err?.message ?? err), error: true }),
      )
      .finally(() => {
        setBusy(undefined)
        reload()
      })
  }
  // Those here first, then the rest; each kind as `pumi skill` lists them.
  const agents = [...(list ?? [])].sort((a, b) => Number(b.available) - Number(a.available))
  const due = agents.filter((a) => a.available && (a.hook === "not_installed" || a.hook === "outdated"))
  const options = (settings?.decls ?? []).filter((d) => d.plugin === AGENTS_PLUGIN)
  return (
    <Page title="Agents">
      <Section>
        <Row
          link="agent-hooks"
          title="Hooks"
          description={!list ? (error ?? "Loading…") : due.length ? `${due.length} to install or update` : "Up to date"}
          control={
            <>
              <Button ghost icon="rotate_cw" title="Refresh" disabled={!!busy} onClick={() => (setStatus(undefined), reload())} />
              <Button primary disabled={!due.length || !!busy} loading={busy === "*"} onClick={() => run("*", () => installHooks(due.map((a) => a.id)), "Done. Restart agents to apply.")} testId="agent-install-all">
                Install all
              </Button>
            </>
          }
        />
      </Section>
      {status ? <Note tone={status.error ? "error" : undefined}>{status.text}</Note> : null}
      <Section title="Agents">
        {agents.map((a) => (
          <AgentRow key={a.id} a={a} busy={busy} run={run} />
        ))}
      </Section>
      {options.length ? <GroupedOptions decls={options} own={AGENTS_PLUGIN} /> : null}
    </Page>
  )
}
