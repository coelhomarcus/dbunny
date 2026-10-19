import type { SslMode, TlsSettings } from "@/types";

export interface ConnectionFields {
  host: string;
  port: string;
  database: string;
  user: string;
  password: string;
  ssl: boolean;
  /** Only used when ssl is on. */
  sslMode: SslMode;
  /** Certificate files, used when ssl is on (empty when not set). */
  sslRootCert: string;
  sslCert: string;
  sslKey: string;
  readOnly: boolean;
}

// libpq's way to start every transaction read-only, understood by the backend driver
const READ_ONLY_OPTION = "-c default_transaction_read_only=on";

// libpq modes that turn encryption on (the switch in the form is a single "require SSL")
const SSL_MODES = new Set(["require", "verify-ca", "verify-full"]);

/** A host that is a folder holding the server's Unix socket (libpq's way, e.g. /tmp). */
export const isSocketHost = (host: string) => host.trim().startsWith("/");

export function parseConnectionUrl(url: string): Partial<ConnectionFields> {
  try {
    const parsed = new URL(url);
    return {
      // A socket folder comes percent-encoded: postgresql://user@%2Ftmp/db
      host: decodeURIComponent(parsed.hostname || ""),
      port: parsed.port || "5432",
      database: decodeURIComponent(parsed.pathname.replace(/^\//, "")),
      user: decodeURIComponent(parsed.username || ""),
      password: decodeURIComponent(parsed.password || ""),
      ssl: SSL_MODES.has(parsed.searchParams.get("sslmode") ?? ""),
      sslMode: (["verify-ca", "verify-full"].includes(parsed.searchParams.get("sslmode") ?? "")
        ? parsed.searchParams.get("sslmode")
        : "require") as SslMode,
      sslRootCert: parsed.searchParams.get("sslrootcert") ?? "",
      sslCert: parsed.searchParams.get("sslcert") ?? "",
      sslKey: parsed.searchParams.get("sslkey") ?? "",
      readOnly: (parsed.searchParams.get("options") ?? "").includes("default_transaction_read_only=on"),
    };
  } catch {
    return {};
  }
}

// MySQL's ssl-mode values that turn encryption on, and the mode of the form each one is
const MYSQL_SSL_MODES: Record<string, SslMode> = { REQUIRED: "require", VERIFY_CA: "verify-ca", VERIFY_IDENTITY: "verify-full" };

/**
 * The fields a mysql:// URL carries: `mysql://user:pass@host:3306/db?ssl-mode=REQUIRED|VERIFY_CA|VERIFY_IDENTITY`,
 * with `ssl-ca`, `ssl-cert` and `ssl-key` for the certificate files. Any other ssl-mode (DISABLED,
 * PREFERRED) leaves encryption off. What the URL doesn't carry, such as read-only, is left out.
 */
export function parseMysqlUrl(url: string): Partial<ConnectionFields> {
  try {
    const parsed = new URL(url);
    const mode = MYSQL_SSL_MODES[(parsed.searchParams.get("ssl-mode") ?? "").toUpperCase()];
    return {
      host: decodeURIComponent(parsed.hostname || ""),
      port: parsed.port || "3306",
      database: decodeURIComponent(parsed.pathname.replace(/^\//, "")),
      user: decodeURIComponent(parsed.username || ""),
      password: decodeURIComponent(parsed.password || ""),
      ssl: mode !== undefined,
      sslMode: mode ?? "require",
      sslRootCert: parsed.searchParams.get("ssl-ca") ?? "",
      sslCert: parsed.searchParams.get("ssl-cert") ?? "",
      sslKey: parsed.searchParams.get("ssl-key") ?? "",
    };
  } catch {
    return {};
  }
}

export function buildConnectionUrl(fields: ConnectionFields): string {
  const { user, password, port, database, readOnly } = fields;
  const socket = isSocketHost(fields.host);
  // Nothing is encrypted over a local socket
  const ssl = fields.ssl && !socket;
  const host = socket ? encodeURIComponent(fields.host.trim()) : fields.host;
  const file = (name: string, path: string) => (ssl && path.trim() ? `${name}=${encodeURIComponent(path.trim())}` : false);
  // Names are encoded too: a "#", "?" or "/" in them would otherwise end that part of the URL
  const name = encodeURIComponent(user);
  const auth = user
    ? password
      ? `${name}:${encodeURIComponent(password)}@`
      : `${name}@`
    : "";
  const portPart = port ? `:${port}` : "";
  const params = [
    ssl && `sslmode=${fields.sslMode || "require"}`,
    file("sslrootcert", fields.sslRootCert),
    file("sslcert", fields.sslCert),
    file("sslkey", fields.sslKey),
    readOnly && `options=${encodeURIComponent(READ_ONLY_OPTION)}`,
  ].filter(Boolean);
  const query = params.length ? `?${params.join("&")}` : "";
  return `postgresql://${auth}${host}${portPart}/${encodeURIComponent(database)}${query}`;
}

/** The form's SSL fields as saved with a connection (null when SSL is off). */
export function tlsSettingsOf(fields: ConnectionFields): TlsSettings | null {
  if (!fields.ssl) return null;
  return { mode: fields.sslMode, rootCert: fields.sslRootCert.trim(), cert: fields.sslCert.trim(), key: fields.sslKey.trim() };
}

/** A saved connection's SSL settings as form fields. */
export function tlsFieldsOf(tls: TlsSettings | null | undefined): Pick<ConnectionFields, "sslMode" | "sslRootCert" | "sslCert" | "sslKey"> {
  return { sslMode: tls?.mode ?? "require", sslRootCert: tls?.rootCert ?? "", sslCert: tls?.cert ?? "", sslKey: tls?.key ?? "" };
}
