import type { SshHostKeyProblem } from "@/types";

/** The SSH host key problem inside a connect error, if that's what the error is. */
export function hostKeyProblem(err: unknown): SshHostKeyProblem | null {
  // The server sends it as JSON text, which reaches us as the message of the error
  const text = typeof err === "string" ? err : err instanceof Error ? err.message : null;
  if (text === null || !text.startsWith("{")) return null;
  try {
    const parsed = JSON.parse(text);
    return parsed?.sshHostKey ?? null;
  } catch {
    return null;
  }
}
