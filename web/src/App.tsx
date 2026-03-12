import { BrowserRouter, Routes, Route, Navigate } from "react-router";
import { ConnectionProvider } from "./contexts/ConnectionContext";
import ConnectPage from "./pages/ConnectPage";
import DatabaseLayout from "./pages/DatabaseLayout";
import QueryEditor from "./pages/QueryEditor";
import TableView from "./pages/TableView";
import TableStructure from "./pages/TableStructure";

export default function App() {
  return (
    <ConnectionProvider>
      <BrowserRouter>
        <Routes>
          <Route path="/" element={<ConnectPage />} />
          <Route path="/db" element={<DatabaseLayout />}>
            <Route index element={<Navigate to="query" replace />} />
            <Route path="query" element={<QueryEditor />} />
            <Route path=":schema/:table" element={<TableView />} />
            <Route path=":schema/:table/structure" element={<TableStructure />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </ConnectionProvider>
  );
}
