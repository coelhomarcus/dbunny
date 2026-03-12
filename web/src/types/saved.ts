export interface SavedConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  database: string;
  user: string;
  password: string;
  ssl: boolean;
  color: string;
  createdAt: string;
  updatedAt: string;
}

export interface SavedConnectionInput {
  id?: string;
  name: string;
  host: string;
  port: number;
  database: string;
  user: string;
  password: string;
  ssl: boolean;
  color: string;
}
