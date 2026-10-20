import { useState } from "react";
import { LogOut } from "lucide-react";
import { useSettings } from "../../lib/settings";
import { assistantApi } from "../../lib/api";
import { agentInfo, type AgentKind } from "../../lib/assistantAgents";
import Button from "../ui/Button";
import ConfirmDialog from "../ui/ConfirmDialog";
import Dialog from "../ui/Dialog";
import SegmentedTabs from "../ui/SegmentedTabs";
import Switch from "../ui/Switch";
import { inputClass } from "../ui/styles";
import { Row, SectionTitle } from "./SettingsRows";

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
      <Row title="Enable the assistant" hint="Turns on the panel and the actions in the query editor. Read what it sends before you do.">
        <Switch checked={settings.assistantEnabled} onChange={(v) => update({ assistantEnabled: v })} aria-label="Enable the assistant" />
      </Row>
      <Row title="Agent" hint={agent === "claude" ? "npx @agentclientprotocol/claude-agent-acp" : agent === "codex" ? "npx @agentclientprotocol/codex-acp" : "Runs on this computer, with your own account."}>
        <SegmentedTabs<AgentKind>
          value={agent}
          onChange={(value) => update({ assistantAgent: value })}
          options={[
            { value: "claude", label: "Claude Code" },
            { value: "codex", label: "Codex" },
            { value: "custom", label: "Custom" },
          ]}
        />
      </Row>
      {agent === "custom" && (
        <Row title="Command" hint="The command that starts an agent speaking the Agent Client Protocol.">
          <input
            className={`${inputClass} w-72`}
            value={command}
            placeholder="/path/to/agent --flag"
            aria-label="Agent command"
            onChange={(event) => setCommand(event.target.value)}
            onBlur={() => command.trim() !== settings.assistantCommand && applyCommand(command.trim())}
          />
        </Row>
      )}
      <Row
        title={`${info.account === "its" ? "Agent" : info.account} account`}
        hint={
          signOutResult ? (
            <span className={signOutResult.ok ? "text-success-foreground" : "text-destructive-foreground"}>{signOutResult.text}</span>
          ) : (
            `Signs out ${info.name} ${info.signOutScope}.`
          )
        }
      >
        <Button variant="outline" size="sm" disabled={signingOut} onClick={() => setConfirmSignOut(true)}>
          <LogOut />
          {signingOut ? "Signing out…" : "Sign out"}
        </Button>
      </Row>
      <Row title="Ask before each query" hint="New conversations ask before running each query. You can allow reads for one conversation from the chat.">
        <Switch checked={settings.assistantAskBeforeQueries} onChange={(v) => update({ assistantAskBeforeQueries: v })} aria-label="Ask before each query" />
      </Row>
      <Row title="Rows sent per query" hint="How many rows of a result the assistant sees, from 10 to 1000.">
        <input
          type="number"
          min={10}
          max={1000}
          className={`${inputClass} w-24`}
          value={settings.assistantRowLimit}
          aria-label="Rows sent per query"
          onChange={(event) => update({ assistantRowLimit: Number(event.target.value) })}
          onBlur={() => update({ assistantRowLimit: Math.min(1000, Math.max(10, Math.round(settings.assistantRowLimit) || 100)) })}
        />
      </Row>
      <Row title="Log" hint="What the agent wrote while it ran, for when it doesn't start.">
        <Button variant="outline" size="sm" onClick={async () => setLog(await assistantApi.log().catch(() => []))}>
          Show log
        </Button>
      </Row>
      <p className="mt-4 text-xs text-muted-foreground">
        {info.name} keeps conversations in {info.folder}. Removing a saved connection removes DBunny&apos;s folder for it, not those files.
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
