import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { getVersion } from "@tauri-apps/api/app";
import type { SavedConnection } from "@/types";
import { useConnection } from "../contexts/useConnection";
import { useWorkspace } from "../contexts/workspace";
import { connectArgsOf, draftFromSaved, draftFromUrl, savedTagOf, type ConnectionDraft } from "../lib/connectionDraft";
import { hostKeyProblem } from "../lib/sshHostKey";
import { api } from "../lib/api";
import { useHostKeyCheck } from "../hooks/useHostKeyCheck";
import ConnectionSidebar from "../components/connect/ConnectionSidebar";
import ConnectionModal from "../components/connect/ConnectionModal";
import ConnectionSummaryCard from "../components/connect/ConnectionSummaryCard";
import ImportUrlDialog from "../components/connect/ImportUrlDialog";
import WelcomePanel from "../components/connect/WelcomePanel";
import { IdleStatusBar } from "../components/layout/StatusBar";

function errorMessage(err: unknown) {
  return typeof err === "string" ? err : err instanceof Error ? err.message : "Connection failed";
}

/**
 * The home page of a tab: saved connections on the left; on the right a welcome, or the summary
 * of the one picked. Creating and editing happen in the connection modal.
 */
export default function ConnectPage() {
  const [connections, setConnections] = useState<SavedConnection[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [modal, setModal] = useState<{ open: boolean; initial: ConnectionDraft | null }>({ open: false, initial: null });
  const [importOpen, setImportOpen] = useState(false);
  // The last connect attempt from the sidebar or the summary, shown on the summary
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [version, setVersion] = useState<string | null>(null);

  const { connect, isConnecting } = useConnection();
  const { sidebarOpen } = useWorkspace();
  const navigate = useNavigate();
  const { withHostKeyCheck, prompt: hostKeyPrompt } = useHostKeyCheck();

  async function loadConnections() {
    try {
      setConnections(await api.getSavedConnections());
    } catch {
      setConnections([]);
    }
  }

  useEffect(() => {
    api.getSavedConnections().then(setConnections).catch(() => setConnections([]));
    getVersion().then(setVersion).catch(() => {});
  }, []);

  const selected = connections?.find((c) => c.id === selectedId) ?? null;

  function select(conn: SavedConnection | null) {
    setSelectedId(conn?.id ?? null);
    setSummaryError(null);
  }

  async function connectSaved(conn: SavedConnection) {
    select(conn);
    const draft = draftFromSaved(conn);
    const attempt = async (trustHostKey?: string) => {
      try {
        await connect(connectArgsOf(draft), savedTagOf(draft), trustHostKey);
        navigate("/db");
      } catch (err) {
        // The SSH server's key is asked about, not shown as an error
        if (hostKeyProblem(err)) throw err;
        setSummaryError(errorMessage(err));
      }
    };
    await withHostKeyCheck(attempt).catch(() => {});
  }

  async function deleteSaved(id: string) {
    try {
      await api.deleteSavedConnection(id);
      if (selectedId === id) select(null);
      await loadConnections();
    } catch {
      // the list is reloaded on the next action
    }
  }

  const openModal = (initial: ConnectionDraft | null) => setModal({ open: true, initial });
  const closeModal = () => setModal({ open: false, initial: null });

  return (
    <div className="h-full flex flex-col">
      <div className="flex flex-1 min-h-0 gap-1.5">
        <ConnectionSidebar
          connections={connections}
          selectedId={selectedId}
          expanded={sidebarOpen}
          onSelect={select}
          onConnect={connectSaved}
          onDelete={deleteSaved}
          onNew={() => openModal(null)}
        />

        <main className="flex min-w-0 flex-1 flex-col rounded-lg border border-border bg-card overflow-hidden">
          {selected ? (
            <ConnectionSummaryCard
              key={selected.id}
              conn={selected}
              connecting={isConnecting}
              error={summaryError}
              onConnect={() => connectSaved(selected)}
              onEdit={() => openModal(draftFromSaved(selected))}
              onDelete={() => deleteSaved(selected.id)}
              onClose={() => select(null)}
            />
          ) : (
            <WelcomePanel
              hasConnections={(connections?.length ?? 0) > 0}
              onNew={() => openModal(null)}
              onOpenUrl={() => setImportOpen(true)}
            />
          )}
        </main>
      </div>

      <IdleStatusBar version={version} />

      {hostKeyPrompt}
      <ImportUrlDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onImport={(url) => {
          const draft = draftFromUrl(url);
          if (!draft) return false;
          setImportOpen(false);
          openModal(draft);
          return true;
        }}
      />
      <ConnectionModal
        open={modal.open}
        initial={modal.initial}
        onClose={closeModal}
        onSaved={async (conn) => {
          closeModal();
          await loadConnections();
          select(conn);
        }}
        onConnected={() => navigate("/db")}
      />
    </div>
  );
}
