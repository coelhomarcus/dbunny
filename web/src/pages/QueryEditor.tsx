import QueryPane from "../components/workspace/QueryPane";

/** Route stand-in until the workspace host mounts each query itself. */
export default function QueryEditor() {
  return <QueryPane documentId="route-query" initialSql="SELECT 1;" title="Query 1" active />;
}
