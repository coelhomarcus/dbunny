export interface ConnectionFields {
  host: string;
  port: string;
  database: string;
  user: string;
  password: string;
  ssl: boolean;
}

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
    };
  } catch {
    return {};
  }
}

export function buildConnectionUrl(fields: ConnectionFields): string {
  const { user, password, host, port, database, ssl } = fields;
  const auth = user
    ? password
      ? `${user}:${encodeURIComponent(password)}@`
      : `${user}@`
    : "";
  const portPart = port ? `:${port}` : "";
  const sslPart = ssl ? "?sslmode=require" : "";
  return `postgresql://${auth}${host}${portPart}/${database}${sslPart}`;
}
