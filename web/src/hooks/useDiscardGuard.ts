import { useCallback, useState } from "react";

/**
 * Edits and the row selection are tracked by row position, so they only make sense
 * for the rows on screen. Wrap any action that loads different rows (page, sorting,
 * filters, refresh): it clears them first, and asks before throwing away unsaved edits.
 */
export function useDiscardGuard(pendingCount: number, reset: () => void) {
  const [queued, setQueued] = useState<(() => void) | null>(null);

  const guard = useCallback(
    <A extends unknown[]>(action: (...args: A) => unknown) =>
      (...args: A) => {
        const run = () => {
          reset();
          action(...args);
        };
        if (pendingCount > 0) setQueued(() => run);
        else run();
      },
    [pendingCount, reset],
  );

  const confirm = useCallback(() => {
    queued?.();
    setQueued(null);
  }, [queued]);

  const cancel = useCallback(() => setQueued(null), []);

  return { guard, isConfirming: queued !== null, confirm, cancel };
}
