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

export function buildConnectionUrl(fields: ConnectionFields): string {
  const { user, password, host, port, database, ssl, readOnly } = fields;
  const file = (name: string, path: string) => (ssl && path.trim() ? `${name}=${encodeURIComponent(path.trim())}` : false);
  const auth = user
    ? password
      ? `${user}:${encodeURIComponent(password)}@`
      : `${user}@`
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
  return `postgresql://${auth}${host}${portPart}/${database}${query}`;
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
