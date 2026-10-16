export interface ConnectRequest {
  connectionUrl: string;
}

export interface ConnectResponse {
  sessionId: string;
  database: string;
  host: string;
  port: number;
  serverVersion: string;
  /** Asked for read-only, or the server is a read replica. */
  readOnly: boolean;
}

export interface ConnectionInfo {
  database: string;
  host: string;
  port: number;
  serverVersion: string;
  readOnly: boolean;
}
