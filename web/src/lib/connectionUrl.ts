export interface ConnectionFields {
  host: string;
  port: string;
  database: string;
  user: string;
  password: string;
  ssl: boolean;
  readOnly: boolean;
}

// libpq's way to start every transaction read-only, understood by the backend driver
const READ_ONLY_OPTION = "-c default_transaction_read_only=on";

// libpq modes that turn encryption on (the switch in the form is a single "require SSL")
const SSL_MODES = new Set(["require", "verify-ca", "verify-full"]);

export function parseConnectionUrl(url: string): Partial<ConnectionFields> {
  try {
    const parsed = new URL(url);
    return {
      host: parsed.hostname || "",
      port: parsed.port || "5432",
      database: parsed.pathname.replace(/^\//, "") || "",
      user: parsed.username || "",
      password: decodeURIComponent(parsed.password || ""),
      ssl: SSL_MODES.has(parsed.searchParams.get("sslmode") ?? ""),
      readOnly: (parsed.searchParams.get("options") ?? "").includes("default_transaction_read_only=on"),
    };
  } catch {
    return {};
  }
}

export function buildConnectionUrl(fields: ConnectionFields): string {
  const { user, password, host, port, database, ssl, readOnly } = fields;
  const auth = user
    ? password
      ? `${user}:${encodeURIComponent(password)}@`
      : `${user}@`
    : "";
  const portPart = port ? `:${port}` : "";
  const params = [
    ssl && "sslmode=require",
    readOnly && `options=${encodeURIComponent(READ_ONLY_OPTION)}`,
  ].filter(Boolean);
  const query = params.length ? `?${params.join("&")}` : "";
  return `postgresql://${auth}${host}${portPart}/${database}${query}`;
}
