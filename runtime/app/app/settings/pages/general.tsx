// Settings → General: the server, the updates, and the app's other groups of options.
import { useEffect, useState } from "react"
import { Button, Note, Page, Row, Section, StatusDot } from "../../../sdk/settings/index.ts"
import { useTheme } from "../../../sdk/theme.ts"
import { engine } from "../../engine.ts"
import { GroupSection, openPath, useGroup, useUpdates } from "../shared.tsx"

/** How often the server's state is asked again while General shows. */
const SERVER_EVERY = 2000

interface ServerStatus {
  version?: string
  pid?: number
  tabs?: number
  sessions?: number
  plugins?: number
}

/** `v0.42.0 · 3 tabs · 1 session`. */
export function serverSummary(s: ServerStatus): string {
  const count = (n: number | undefined, one: string) => `${n ?? 0} ${one}${n === 1 ? "" : "s"}`
  return [`v${s.version ?? "?"}`, count(s.tabs, "tab"), count(s.sessions, "session"), count(s.plugins, "plugin")].join(" · ")
}

function ServerSection() {
  const t = useTheme()
  const [status, setStatus] = useState<{ ok?: ServerStatus; error?: string }>()
  const [restarting, setRestarting] = useState(false)
  useEffect(() => {
    let live = true
    const ask = () =>
      (engine.serverStatus() as Promise<ServerStatus>).then(
        (ok) => live && setStatus({ ok }),
        (err: Error) => live && setStatus({ error: String(err?.message ?? err) }),
      )
    ask()
    const timer = setInterval(ask, SERVER_EVERY)
    return () => {
      live = false
      clearInterval(timer)
    }
  }, [])
  const log = engine.serverLog()
  const [title, color, description] = status?.ok
    ? ["Running", t.success, serverSummary(status.ok)]
    : status?.error
      ? ["Not answering", t.error, status.error]
      : ["Asking…", t.faint, ""]
  const restart = () => {
    setRestarting(true)
    ;(engine.serverRestart() as Promise<unknown>)
      .catch((err: Error) => setStatus({ error: String(err?.message ?? err) }))
      .finally(() => setTimeout(() => setRestarting(false), 800))
  }
  return (
    <Section title="Pumi Terminals" id="server">
      <Row
        link="server"
        icon={<StatusDot color={color} />}
        title={title}
        description={description}
        control={
          <>
            <Button icon="arrow_up_right" disabled={!log} onClick={() => log && openPath(log)}>
              Open log
            </Button>
            <Button title="Start the server again from the binaries on disk; every program keeps running" loading={restarting} onClick={restart}>
              {restarting ? "Restarting" : "Restart"}
            </Button>
          </>
        }
      />
    </Section>
  )
}

function UpdatesSection() {
  const { app } = useUpdates()
  const ready = app.status === "ready"
  const busy = app.status === "checking" || app.status === "downloading"
  const options = useGroup("Updates")
  return (
    <GroupSection group="Updates">
      <Row
        link="updates"
        title="Current version"
        description={app.text}
        control={
          ready ? (
            <Button primary onClick={() => engine.restartToUpdate()}>
              Restart to update
            </Button>
          ) : (
            <Button loading={busy} disabled={!app.canCheck} onClick={() => engine.checkForUpdates(false)}>
              Check for updates
            </Button>
          )
        }
      />
      {options.length ? null : <Note>{`${app.channel} channel`}</Note>}
    </GroupSection>
  )
}

export function GeneralPage({ others }: { others: string[] }) {
  return (
    <Page title="General">
      <ServerSection />
      <UpdatesSection />
      <GroupSection group="General" />
      {others.map((group) => (
        <GroupSection key={group} group={group} />
      ))}
    </Page>
  )
}
