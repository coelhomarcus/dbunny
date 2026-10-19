import { useState } from "react";
import { LogOut } from "lucide-react";
import { useSettings } from "../../lib/settings";
import { assistantApi } from "../../lib/api";
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
  const [custom, setCustom] = useState(settings.assistantCommand !== "");
  const [command, setCommand] = useState(settings.assistantCommand);
  const [log, setLog] = useState<string[] | null>(null);
  const [confirmSignOut, setConfirmSignOut] = useState(false);
  const [signingOut, setSigningOut] = useState(false);
  const [signOutResult, setSignOutResult] = useState<{ ok: boolean; text: string } | null>(null);

  async function signOut() {
    setConfirmSignOut(false);
    setSigningOut(true);
    setSignOutResult(null);
    try {
      await assistantApi.signOut(settings.assistantCommand || undefined);
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
    if (status && status.state !== "idle") assistantApi.restart(next || undefined).catch(() => {});
  }

  return (
    <>
      <SectionTitle>AI Assistant</SectionTitle>
      <Row title="Enable the assistant" hint="Turns on the panel and the actions in the query editor. Read what it sends before you do.">
        <Switch checked={settings.assistantEnabled} onChange={(v) => update({ assistantEnabled: v })} aria-label="Enable the assistant" />
      </Row>
      <Row title="Agent" hint="Runs on this computer, with your Claude account.">
        <span className="text-[13px]">Claude Code</span>
      </Row>
      <Row title="Command" hint={custom ? "The command that starts an agent speaking the Agent Client Protocol." : "npx @agentclientprotocol/claude-agent-acp"}>
        <div className="flex w-72 flex-col items-end gap-2">
          <SegmentedTabs<"default" | "custom">
            value={custom ? "custom" : "default"}
            onChange={(value) => {
              setCustom(value === "custom");
              if (value === "default") {
                setCommand("");
                if (settings.assistantCommand !== "") applyCommand("");
              }
            }}
            options={[
              { value: "default", label: "Default" },
              { value: "custom", label: "Custom" },
            ]}
          />
          {custom && (
            <input
              className={inputClass}
              value={command}
              placeholder="/path/to/agent --flag"
              aria-label="Agent command"
              onChange={(event) => setCommand(event.target.value)}
              onBlur={() => command.trim() !== settings.assistantCommand && applyCommand(command.trim())}
            />
          )}
        </div>
      </Row>
      <Row
        title="Claude account"
        hint={
          signOutResult ? (
            <span className={signOutResult.ok ? "text-success-foreground" : "text-destructive-foreground"}>{signOutResult.text}</span>
          ) : (
            "Signs out Claude Code on this computer, not only in DBunny."
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
        Claude Code keeps conversations in ~/.claude/projects. Removing a saved connection removes DBunny&apos;s folder for it, not those files.
      </p>
      <ConfirmDialog
        open={confirmSignOut}
        onClose={() => setConfirmSignOut(false)}
        onConfirm={signOut}
        icon={<LogOut />}
        title="Sign out of Claude?"
        description="This signs out Claude Code on this computer, not only in DBunny, and ends the conversations of the assistant. You sign in again from its panel."
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
