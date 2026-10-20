import { useState } from "react";
import { LogOut } from "lucide-react";
import { useSettings } from "../../lib/settings";
import { assistantApi } from "../../lib/api";
import { parseRowLimit } from "../../lib/assistantPrefs";
import { agentInfo, agentSummary } from "../../lib/assistantAgents";
import Button from "../ui/Button";
import ConfirmDialog from "../ui/ConfirmDialog";
import Dialog from "../ui/Dialog";
import Switch from "../ui/Switch";
import { inputClass } from "../ui/styles";
import AgentPicker from "./AgentPicker";
import { Group, Row, SectionTitle } from "./SettingsRows";
import { useT } from "../../i18n";
import { Rich } from "../../i18n/Rich";
import { errorText } from "../../lib/appError";

/** How the assistant is run and what it is told by default. */
export default function AssistantSection() {
  const t = useT();
  const [settings, update] = useSettings();
  const [command, setCommand] = useState(settings.assistantCommand);
  const [rowsText, setRowsText] = useState(String(settings.assistantRowLimit));
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
      setSignOutResult({ ok: true, text: t("assistant.assistantSection.signedOutOpenTheAssistant") });
    } catch (error) {
      setSignOutResult({ ok: false, text: errorText(error, t("assistant.assistantSection.signOutFailed")) });
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
      <SectionTitle>{t("assistant.assistantSection.aiAssistant")}</SectionTitle>
      <Row title={t("assistant.assistantSection.askYourDatabase")} hint={t("assistant.assistantSection.aChatThatReadsYour")}>
        <Switch checked={settings.assistantEnabled} onChange={(v) => update({ assistantEnabled: v })} aria-label={t("assistant.assistantSection.enableTheAssistant")} />
      </Row>

      <Group title={t("assistant.assistantSection.agent")}>
        <div className="space-y-3 border-b border-border py-3.5">
          <AgentPicker value={agent} onChange={(value) => update({ assistantAgent: value })} />
          <p className="text-xs leading-relaxed text-muted-foreground">{agentSummary(info)}</p>
          {agent === "custom" && (
            <label className="block">
              <span className="text-[13px] font-medium">{t("assistant.assistantSection.command")}</span>
              <input
                className={`${inputClass} mt-1 w-full`}
                value={command}
                placeholder={t("assistant.assistantSection.pathToAgentFlag")}
                aria-label={t("assistant.assistantSection.agentCommand")}
                onChange={(event) => setCommand(event.target.value)}
                onBlur={() => command.trim() !== settings.assistantCommand && applyCommand(command.trim())}
              />
              <span className="mt-1 block text-xs text-muted-foreground">{t("assistant.assistantSection.anyAgentThatSpeaksThe")}</span>
            </label>
          )}
        </div>
        <Row
          title={agent === "custom" ? t("assistant.assistantSection.signOutOfTheAgent") : t("assistant.assistantSection.signOutOf", { name: info.name })}
          hint={signOutResult ? <span className={signOutResult.ok ? "text-success-foreground" : "text-destructive-foreground"}>{signOutResult.text}</span> : t("assistant.assistantSection.signsOut", { scope: info.signOutScope })}
        >
          <Button variant="outline" size="sm" disabled={signingOut} onClick={() => setConfirmSignOut(true)}>
            <LogOut />
            {signingOut ? t("assistant.assistantSection.signingOut") : t("assistant.assistantSection.signOut")}
          </Button>
        </Row>
      </Group>

      <Group title={t("assistant.assistantSection.queries")}>
        <Row title={t("assistant.assistantSection.askBeforeEachQuery")} hint={t("assistant.assistantSection.youCanAllowReadsFor")}>
          <Switch checked={settings.assistantAskBeforeQueries} onChange={(v) => update({ assistantAskBeforeQueries: v })} aria-label={t("assistant.assistantSection.askBeforeEachQuery")} />
        </Row>
        <Row title={t("assistant.assistantSection.rowsTheAssistantCanSee")} hint={t("assistant.assistantSection.perQueryFrom10To")}>
          <input
            type="number"
            min={10}
            max={1000}
            className={`${inputClass} w-24`}
            value={rowsText}
            aria-label={t("assistant.assistantSection.rowsTheAssistantCanSee")}
            onChange={(event) => setRowsText(event.target.value)}
            onBlur={() => {
              // Only what is finished is kept: a cleared or half-typed number does not reach the settings
              const rows = parseRowLimit(rowsText);
              update({ assistantRowLimit: rows });
              setRowsText(String(rows));
            }}
          />
        </Row>
      </Group>

      <p className="mt-6 text-xs leading-relaxed text-muted-foreground">
        <Rich
          id="assistant.assistantSection.savedNote"
          params={{ schemaOnly: t("assistant.assistantSection.schemaOnly"), name: info.name, folder: info.folder }}
          tags={{ strong: (text) => <span className="font-medium text-foreground">{text}</span> }}
        />
      </p>
      <p className="mt-3 text-xs text-muted-foreground">
        {t("assistant.assistantSection.notStarting")}{" "}
        <button type="button" className="cursor-pointer text-primary underline underline-offset-2" onClick={async () => setLog(await assistantApi.log().catch(() => []))}>
          {t("assistant.assistantSection.showTheAgentLog")}
        </button>
      </p>
      <ConfirmDialog
        open={confirmSignOut}
        onClose={() => setConfirmSignOut(false)}
        onConfirm={signOut}
        icon={<LogOut />}
        title={agent === "custom" ? t("assistant.assistantSection.signOutConfirmCustom") : t("assistant.assistantSection.signOutConfirm", { name: info.name })}
        description={t("assistant.assistantSection.signOutNote", { name: info.name, scope: info.signOutScope })}
        confirmLabel={t("assistant.assistantSection.signOut")}
      />
      <Dialog open={log !== null} onClose={() => setLog(null)} title={t("assistant.assistantSection.assistantLog")} className="max-w-2xl">
        <pre className="max-h-96 overflow-auto rounded-lg bg-muted px-3 py-2 font-mono text-xs leading-relaxed whitespace-pre-wrap">
          {log && log.length > 0 ? log.join("\n") : t("assistant.assistantStatusScreen.nothingLogged")}
        </pre>
      </Dialog>
    </>
  );
}
