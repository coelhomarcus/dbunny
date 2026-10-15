export interface ConnectRequest {
  connectionUrl: string;
}

export interface ConnectResponse {
  sessionId: string;
  database: string;
  host: string;
  port: number;
  serverVersion: string;
}

export interface ConnectionInfo {
  database: string;
  host: string;
  port: number;
  serverVersion: string;
}
