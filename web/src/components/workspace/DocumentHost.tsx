import { PanelsTopLeft, Plus } from "lucide-react";
import { useDocuments } from "../../contexts/DocumentsContext";
import { useTab } from "../../contexts/workspace";
import QueryPane from "./QueryPane";
import TableView from "../../pages/TableView";
import TableStructure from "../../pages/TableStructure";
import FunctionView from "../../pages/FunctionView";
import ObjectView from "../../pages/ObjectView";
import ActivityView from "../../pages/ActivityView";
import RolesView from "../../pages/RolesView";
import CreateTablePage from "./CreateTablePage";
import Button from "../ui/Button";

/** Mounts every open document and shows only the one in front. */
export default function DocumentHost() {
  const { documents, openDocument } = useDocuments();
  const { isActive } = useTab();
  if (documents.items.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center px-6 text-center select-none">
        <span className="flex size-12 items-center justify-center rounded-2xl bg-muted text-muted-foreground">
          <PanelsTopLeft size={22} />
        </span>
        <p className="mt-4 text-sm font-medium text-foreground">Nothing is open</p>
        <p className="mt-1 max-w-xs text-[13px] leading-relaxed text-muted-foreground">
          Open a table, view, or routine from the explorer, or start a query.
        </p>
        <Button variant="primary" size="sm" className="mt-5" onClick={() => openDocument({ kind: "query" })}>
          <Plus />
          New query
        </Button>
      </div>
    );
  }
  return (
    <>
      {documents.items.map((doc) => {
        const front = doc.id === documents.activeId;
        return (
          <div key={doc.id} className={front ? "h-full min-h-0" : "hidden"} hidden={!front} {...(!front ? { inert: true } : {})}>
            {doc.kind === "query" && (
              <QueryPane documentId={doc.id} initialSql={doc.sql ?? ""} savedId={doc.savedId} title={doc.title} active={front && isActive} />
            )}
            {doc.kind === "data" && doc.schema && doc.name && (
              <TableView documentId={doc.id} schema={doc.schema} name={doc.name} filterRequest={doc.filterRequest} active={front && isActive} />
            )}
            {doc.kind === "structure" && doc.schema && doc.name && <TableStructure schema={doc.schema} name={doc.name} />}
            {(doc.kind === "function" || doc.kind === "procedure") && doc.schema && doc.name && (
              <FunctionView schema={doc.schema} name={doc.name} detail={doc.detail ?? ""} />
            )}
            {(doc.kind === "sequence" || doc.kind === "type" || doc.kind === "index" || doc.kind === "trigger") && doc.schema && doc.name && (
              <ObjectView schema={doc.schema} name={doc.name} kind={doc.kind} />
            )}
            {doc.kind === "activity" && <ActivityView />}
            {doc.kind === "roles" && <RolesView />}
            {doc.kind === "create-table" && doc.schema && <CreateTablePage documentId={doc.id} schema={doc.schema} />}
          </div>
        );
      })}
    </>
  );
}
