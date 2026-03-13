import { useEffect } from "react";
import { HashRouter, Routes, Route, Navigate } from "react-router";
import { ConnectionProvider } from "./contexts/ConnectionContext";
import ConnectPage from "./pages/ConnectPage";
import DatabaseLayout from "./pages/DatabaseLayout";
import QueryEditor from "./pages/QueryEditor";
import TableView from "./pages/TableView";
import TableStructure from "./pages/TableStructure";
import FunctionView from "./pages/FunctionView";

export default function App() {
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    async function handleKey(e: KeyboardEvent) {
      if (e.ctrlKey && e.shiftKey && e.key === "I") {
        const { invoke } = await import("@tauri-apps/api/core");
        invoke("open_devtools");
      }
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, []);

  return (
    <ConnectionProvider>
      <HashRouter>
        <Routes>
          <Route path="/" element={<ConnectPage />} />
          <Route path="/db" element={<DatabaseLayout />}>
            <Route index element={<Navigate to="query" replace />} />
            <Route path="query" element={<QueryEditor />} />
            <Route path=":schema/:table" element={<TableView />} />
            <Route path=":schema/:table/structure" element={<TableStructure />} />
            <Route path=":schema/function/:name" element={<FunctionView />} />
          </Route>
        </Routes>
      </HashRouter>
    </ConnectionProvider>
  );
}
