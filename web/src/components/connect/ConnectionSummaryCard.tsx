import { useState, type ReactNode } from "react";
import { ArrowRight, Eye, EyeOff, Lock, Pencil, ShieldCheck, Trash2, TriangleAlert, Waypoints, X } from "lucide-react";
import Spinner from "../ui/Spinner";
import type { SavedConnection } from "@/types";
import { DEFAULT_COLOR } from "../../lib/connectionDraft";
import { mask, summaryOf } from "../../lib/connectionSummary";
import Button from "../ui/Button";
import Badge from "../ui/Badge";
import ConfirmDialog from "../ui/ConfirmDialog";
import Tooltip from "../Tooltip";
import { ConnectionIcon } from "./ConnectionSidebar";
import { useT } from "../../i18n";

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-h-8 items-center gap-4 border-b border-border py-1.5 last:border-b-0">
      <dt className="w-20 shrink-0 text-xs text-muted-foreground">{label}</dt>
      <dd className="flex min-w-0 flex-1 items-center gap-1.5 text-[13px]">{children}</dd>
    </div>
  );
}

/** A saved connection picked on the home page: what it points at, and connect, edit or delete it. */
export default function ConnectionSummaryCard({
  conn,
  connecting,
  error,
  onConnect,
  onEdit,
  onDelete,
  onClose,
}: {
  conn: SavedConnection;
  connecting: boolean;
  error: string | null;
  onConnect: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  const t = useT();
  // The address is masked until asked for, like in the status bar
  const [revealed, setRevealed] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const summary = summaryOf(conn);
  const name = conn.name || "Unnamed";

  return (
    <div className="flex flex-1 items-center justify-center overflow-y-auto px-6 py-10">
      <div className="relative w-full max-w-lg rounded-xl border border-border bg-card p-5 shadow-xs/5">
        <Button variant="ghost-muted" size="icon-xs" className="absolute right-3 top-3" aria-label={t("common.close")} onClick={onClose}>
          <X />
        </Button>

        <div className="flex items-center gap-3.5 pr-8">
          <ConnectionIcon color={conn.color || DEFAULT_COLOR} icon={conn.icon} size="lg" />
          <div className="min-w-0">
            <h1 className="truncate text-[17px] font-semibold leading-tight">{name}</h1>
            <p className="mt-0.5 text-xs text-muted-foreground">{summary.engineLabel}</p>
          </div>
        </div>

        <dl className="mt-5">
          <Row label={conn.engine === "sqlite" ? t("connect.connectionSummaryCard.file") : t("connect.connectionSummaryCard.address")}>
            <span className="min-w-0 truncate font-mono text-xs">{revealed ? summary.address : mask(summary.address)}</span>
            <Tooltip label={revealed ? t("connect.connectionSummaryCard.hideAddress") : t("connect.connectionSummaryCard.showAddress")} side="top">
              <Button
                variant="ghost-muted"
                size="icon-xs"
                aria-label={revealed ? t("connect.connectionSummaryCard.hideAddress") : t("connect.connectionSummaryCard.showAddress")}
                onClick={() => setRevealed((v) => !v)}
              >
                {revealed ? <EyeOff /> : <Eye />}
              </Button>
            </Tooltip>
          </Row>
          {conn.engine !== "sqlite" && (
            <Row label={t("connect.connectionSummaryCard.user")}>
              <span className="truncate">{summary.user || <span className="text-muted-foreground">{t("connect.connectionSummaryCard.default")}</span>}</span>
            </Row>
          )}
          {(summary.ssl || summary.sshVia || summary.readOnly) && (
            <Row label={t("connect.connectionSummaryCard.options")}>
              <div className="flex flex-wrap gap-1.5">
                {summary.ssl && (
                  <Badge>
                    <ShieldCheck />
                    SSL
                  </Badge>
                )}
                {summary.sshVia && (
                  <Badge>
                    <Waypoints />
                    SSH · {summary.sshVia}
                  </Badge>
                )}
                {summary.readOnly && (
                  <Badge variant="warning">
                    <Lock />
                    {t("connect.connectionSummaryCard.readOnly")}
                  </Badge>
                )}
              </div>
            </Row>
          )}
        </dl>

        {error && (
          <div
            data-summary-error
            className="mt-4 flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/8 px-3 py-2.5 text-sm text-destructive-foreground"
          >
            <TriangleAlert size={15} className="mt-0.5 shrink-0" />
            <span className="min-w-0 select-text break-words">{error}</span>
          </div>
        )}

        <div className="mt-5 flex items-center gap-2">
          <Button variant="primary" className="group px-4" onClick={onConnect} disabled={connecting}>
            {connecting ? (
              <>
                <Spinner />
                {t("connect.connectionSummaryCard.connecting")}
              </>
            ) : (
              <>
                {t("connect.connectionSummaryCard.connect")}
                <ArrowRight className="transition-transform group-hover:translate-x-0.5" />
              </>
            )}
          </Button>
          <Button variant="outline" onClick={onEdit}>
            <Pencil />
            {t("common.edit")}
          </Button>
          <div className="flex-1" />
          <Button variant="ghost-muted" className="hover:bg-destructive/10 hover:text-destructive-foreground" onClick={() => setConfirmDelete(true)}>
            <Trash2 />
            {t("common.delete")}
          </Button>
        </div>
      </div>

      <ConfirmDialog
        open={confirmDelete}
        onClose={() => setConfirmDelete(false)}
        onConfirm={() => {
          setConfirmDelete(false);
          onDelete();
        }}
        icon={<Trash2 />}
        tone="destructive"
        title={t("connect.connectionSummaryCard.deleteTitle", { name })}
        description={t("connect.connectionSummaryCard.removeNote")}
        cancelLabel={t("common.cancel")}
        confirmLabel={t("common.delete")}
      />
    </div>
  );
}
