import type { SshHostKeyProblem } from "@/types";

/** The SSH host key problem inside a connect error, if that's what the error is. */
export function hostKeyProblem(err: unknown): SshHostKeyProblem | null {
  if (typeof err !== "string" || !err.startsWith("{")) return null;
  try {
    const parsed = JSON.parse(err);
    return parsed?.sshHostKey ?? null;
  } catch {
    return null;
  }
}
