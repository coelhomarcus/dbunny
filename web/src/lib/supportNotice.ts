import type { EndOfSupport, Engine } from "@/types";

/** Where the version line whose end-of-support notice was seen is kept, per connection. */
export function supportNoticeKey(savedConnectionId: string | null, info: { engine: Engine; host: string; port: number }): string {
  return `dbunny.endOfSupportSeen.${savedConnectionId ?? `${info.engine}:${info.host}:${info.port}`}`;
}

/** What is kept once the notice was seen: the engine and its line, e.g. "postgres 13". */
export function seenValue(engine: Engine, endOfSupport: EndOfSupport): string {
  return `${engine} ${endOfSupport.line}`;
}

/** The notice shows when the version is past its end of support and this line wasn't seen yet. */
export function shouldShowSupportNotice(endOfSupport: EndOfSupport | null, seen: string | null, engine: Engine): boolean {
  return endOfSupport !== null && seen !== seenValue(engine, endOfSupport);
}

/** The value kept under `key`; null when there is none or the storage can't be read. */
export function readSeen(storage: Pick<Storage, "getItem"> | null, key: string): string | null {
  try {
    return storage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

/** Keeps `value` under `key`; a storage that can't be written only means the notice shows again. */
export function rememberSeen(storage: Pick<Storage, "setItem"> | null, key: string, value: string): void {
  try {
    storage?.setItem(key, value);
  } catch {
    // Nothing else depends on it
  }
}
