import { useCallback, useState } from "react";
import { hostKeyProblem } from "../lib/sshHostKey";
import HostKeyPrompt, { type HostKeyRequest } from "../components/connect/HostKeyPrompt";

/**
 * Runs a connection attempt; if the SSH server's key needs the user's say, asks, and runs it
 * again with the fingerprint they accepted. Render `prompt` where the question should appear.
 */
export function useHostKeyCheck() {
  const [request, setRequest] = useState<HostKeyRequest | null>(null);

  const withHostKeyCheck = useCallback(async (attempt: (trustHostKey?: string) => Promise<void>) => {
    try {
      await attempt();
    } catch (err) {
      const problem = hostKeyProblem(err);
      if (!problem) throw err;
      setRequest({
        problem,
        retry: () => {
          setRequest(null);
          attempt(problem.fingerprint).catch(() => {});
        },
      });
    }
  }, []);

  const prompt = <HostKeyPrompt request={request} onClose={() => setRequest(null)} />;
  return { withHostKeyCheck, prompt };
}
