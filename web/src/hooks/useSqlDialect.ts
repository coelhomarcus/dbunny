import { useConnection } from "../contexts/useConnection";
import { sqlDialectOf } from "../lib/engines";
import type { SqlDialectName } from "../lib/sqlStatements";

/** The SQL dialect of the connected database. */
export function useSqlDialect(): SqlDialectName {
  return sqlDialectOf(useConnection().connectionInfo?.engine);
}
