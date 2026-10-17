import { createContext, useCallback, useContext, useState, type ReactNode } from "react";
import NoticeToast, { type Notice } from "../components/ui/NoticeToast";

const NotifyContext = createContext<(notice: Notice) => void>(() => {});

/** One toast for the tab, so a failure is reported the same way everywhere. */
export function NoticeProvider({ children }: { children: ReactNode }) {
  const [notice, setNotice] = useState<Notice | null>(null);
  const notify = useCallback((next: Notice) => setNotice(next), []);
  const dismiss = useCallback(() => setNotice(null), []);
  return (
    <NotifyContext.Provider value={notify}>
      {children}
      <NoticeToast notice={notice} onDismiss={dismiss} />
    </NotifyContext.Provider>
  );
}

export function useNotify() {
  return useContext(NotifyContext);
}
