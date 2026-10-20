import { useState } from "react";
import { LogOut } from "lucide-react";
import { useSettings } from "../../lib/settings";
import { assistantApi } from "../../lib/api";
import { agentInfo, agentSummary } from "../../lib/assistantAgents";
import Button from "../ui/Button";
import ConfirmDialog from "../ui/ConfirmDialog";
import Dialog from "../ui/Dialog";
import Switch from "../ui/Switch";
import { inputClass } from "../ui/styles";
import AgentPicker from "./AgentPicker";
import { Group, Row, SectionTitle } from "./SettingsRows";

/** How the assistant is run and what it is told by default. */
export default function AssistantSection() {
  const [settings, update] = useSettings();
  const [command, setCommand] = useState(settings.assistantCommand);
  const [log, setLog] = useState<string[] | null>(null);
  const [confirmSignOut, setConfirmSignOut] = useState(false);
  const [signingOut, setSigningOut] = useState(false);
  const agent = settings.assistantAgent;
  const info = agentInfo(agent);
  const [signOutResult, setSignOutResult] = useState<{ ok: boolean; text: string } | null>(null);

  async function signOut() {
    setConfirmSignOut(false);
    setSigningOut(true);
    setSignOutResult(null);
    try {
      await assistantApi.signOut(agent, agent === "custom" ? settings.assistantCommand || undefined : undefined);
      setSignOutResult({ ok: true, text: "Signed out. Open the assistant to sign in again." });
    } catch (error) {
      setSignOutResult({ ok: false, text: typeof error === "string" ? error : error instanceof Error ? error.message : "Couldn't sign out" });
    } finally {
      setSigningOut(false);
    }
  }

  // A new command takes effect by starting the agent again, if it is up
  async function applyCommand(next: string) {
    update({ assistantCommand: next });
    const status = await assistantApi.status().catch(() => null);
    if (status && status.state !== "idle") assistantApi.restart("custom", next || undefined).catch(() => {});
  }

  return (
    <>
      <SectionTitle>AI Assistant</SectionTitle>
      <Row title="Ask your database" hint="A chat that reads your data through an agent on this computer. It stays off until you turn it on.">
        <Switch checked={settings.assistantEnabled} onChange={(v) => update({ assistantEnabled: v })} aria-label="Enable the assistant" />
      </Row>

      <Group title="Agent">
        <div className="space-y-3 border-b border-border py-3.5">
          <AgentPicker value={agent} onChange={(value) => update({ assistantAgent: value })} />
          <p className="text-xs leading-relaxed text-muted-foreground">{agentSummary(info)}</p>
          {agent === "custom" && (
            <label className="block">
              <span className="text-[13px] font-medium">Command</span>
              <input
                className={`${inputClass} mt-1 w-full`}
                value={command}
                placeholder="/path/to/agent --flag"
                aria-label="Agent command"
                onChange={(event) => setCommand(event.target.value)}
                onBlur={() => command.trim() !== settings.assistantCommand && applyCommand(command.trim())}
              />
              <span className="mt-1 block text-xs text-muted-foreground">Any agent that speaks the Agent Client Protocol.</span>
            </label>
          )}
        </div>
        <Row
          title={`Sign out of ${info.name === "your agent" ? "the agent" : info.name}`}
          hint={signOutResult ? <span className={signOutResult.ok ? "text-success-foreground" : "text-destructive-foreground"}>{signOutResult.text}</span> : `Signs out ${info.signOutScope}.`}
        >
          <Button variant="outline" size="sm" disabled={signingOut} onClick={() => setConfirmSignOut(true)}>
            <LogOut />
            {signingOut ? "Signing out…" : "Sign out"}
          </Button>
        </Row>
      </Group>

      <Group title="Queries">
        <Row title="Ask before each query" hint="You can allow reads for one conversation from the chat.">
          <Switch checked={settings.assistantAskBeforeQueries} onChange={(v) => update({ assistantAskBeforeQueries: v })} aria-label="Ask before each query" />
        </Row>
        <Row title="Rows the assistant can see" hint="Per query, from 10 to 1000.">
          <input
            type="number"
            min={10}
            max={1000}
            className={`${inputClass} w-24`}
            value={settings.assistantRowLimit}
            aria-label="Rows the assistant can see"
            onChange={(event) => update({ assistantRowLimit: Number(event.target.value) })}
            onBlur={() => update({ assistantRowLimit: Math.min(1000, Math.max(10, Math.round(settings.assistantRowLimit) || 100)) })}
          />
        </Row>
      </Group>

      <p className="mt-6 text-xs leading-relaxed text-muted-foreground">
        For a saved connection, <span className="font-medium text-foreground">Schema only</span> and notes about the database are in the chat&apos;s options button. Removing a saved
        connection removes DBunny&apos;s folder for it, not what {info.name} kept in {info.folder}.
      </p>
      <p className="mt-3 text-xs text-muted-foreground">
        Agent not starting?{" "}
        <button type="button" className="cursor-pointer text-primary underline underline-offset-2" onClick={async () => setLog(await assistantApi.log().catch(() => []))}>
          Show the agent log
        </button>
      </p>
      <ConfirmDialog
        open={confirmSignOut}
        onClose={() => setConfirmSignOut(false)}
        onConfirm={signOut}
        icon={<LogOut />}
        title={`Sign out of ${info.name}?`}
        description={`This signs out ${info.name} ${info.signOutScope}, and ends the conversations of the assistant. You sign in again from its panel.`}
        confirmLabel="Sign out"
      />
      <Dialog open={log !== null} onClose={() => setLog(null)} title="Assistant log" className="max-w-2xl">
        <pre className="max-h-96 overflow-auto rounded-lg bg-muted px-3 py-2 font-mono text-xs leading-relaxed whitespace-pre-wrap">
          {log && log.length > 0 ? log.join("\n") : "Nothing was logged."}
        </pre>
      </Dialog>
    </>
  );
}
