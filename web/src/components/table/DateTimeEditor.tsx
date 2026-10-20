import { useMemo, useState, type RefObject } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import Popover from "../ui/Popover";
import Button from "../ui/Button";
import { cn } from "../../lib/cn";
import { inputClass } from "../ui/styles";
import { datePart, fromDateText, nowText, timePart, toDateText, withDate, withTime, type DateMode } from "../../lib/dateValue";
import { useT, fmt } from "../../i18n";

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/**
 * Calendar and time field under a date/time cell. The value is also plain text, so
 * anything Postgres accepts (fractions, "infinity") can still be typed.
 */
export default function DateTimeEditor({
  anchorRef,
  mode,
  value,
  nullable,
  onCommit,
  onCancel,
}: {
  anchorRef: RefObject<HTMLElement | null>;
  mode: DateMode;
  value: unknown;
  nullable: boolean;
  onCommit: (isNull: boolean, text: string) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const initial = toDateText(value, mode);
  const [text, setText] = useState(initial);
  const selected = datePart(text);
  const today = new Date();
  const [view, setView] = useState(() => ({
    year: selected?.year ?? today.getFullYear(),
    month: selected?.month ?? today.getMonth() + 1,
  }));

  // Monday-first grid of the shown month, padded with blanks
  const cells = useMemo(() => {
    const first = new Date(Date.UTC(view.year, view.month - 1, 1));
    const offset = (first.getUTCDay() + 6) % 7;
    const days = new Date(Date.UTC(view.year, view.month, 0)).getUTCDate();
    return [...Array(offset).fill(null), ...Array.from({ length: days }, (_, i) => i + 1)];
  }, [view]);

  function shiftMonth(step: number) {
    const index = view.year * 12 + (view.month - 1) + step;
    setView({ year: Math.floor(index / 12), month: (index % 12) + 1 });
  }

  function apply() {
    // Saving the same moment written differently would look like a change
    if (text.trim() === initial.trim()) return onCancel();
    onCommit(false, fromDateText(text, mode));
  }

  const monthName = fmt().date(Date.UTC(view.year, view.month - 1, 1), { dateStyle: undefined, month: "long", year: "numeric", timeZone: "UTC" });
  const isToday = (day: number) => day === today.getDate() && view.month === today.getMonth() + 1 && view.year === today.getFullYear();

  return (
    <Popover open onClose={onCancel} anchorRef={anchorRef} className="w-64 p-2">
      <div
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            e.stopPropagation();
            apply();
          }
        }}
      >
        <div className="flex items-center gap-1.5">
          <input
            autoFocus
            aria-label={t("table.dateTimeEditor.value")}
            value={text}
            placeholder={value === null ? "NULL" : ""}
            onChange={(e) => setText(e.target.value)}
            spellCheck={false}
            className={cn(inputClass, "h-7 px-2 font-mono text-xs placeholder:italic")}
          />
          {mode === "timestamptz" && <span className="shrink-0 text-[11px] font-medium text-muted-foreground">UTC</span>}
        </div>

        {mode !== "time" && (
          <div className="mt-2 select-none">
            <div className="flex items-center justify-between">
              <Button variant="ghost-muted" size="icon-xs" aria-label={t("table.dateTimeEditor.previousMonth")} onClick={() => shiftMonth(-1)}>
                <ChevronLeft />
              </Button>
              <span className="text-[13px] font-medium capitalize">{monthName}</span>
              <Button variant="ghost-muted" size="icon-xs" aria-label={t("table.dateTimeEditor.nextMonth")} onClick={() => shiftMonth(1)}>
                <ChevronRight />
              </Button>
            </div>
            <div className="mt-1 grid grid-cols-7 gap-0.5 text-center">
              {WEEKDAYS.map((d) => (
                <span key={d} className="py-1 text-[10px] font-medium text-muted-foreground">{d}</span>
              ))}
              {cells.map((day, i) =>
                day === null ? (
                  <span key={`blank-${i}`} />
                ) : (
                  <button
                    key={day}
                    type="button"
                    onClick={() => setText((t) => withDate(t || nowText(mode), mode, view.year, view.month, day))}
                    className={cn(
                      "h-7 rounded-md text-xs tabular-nums cursor-pointer hover:bg-accent",
                      selected?.year === view.year && selected.month === view.month && selected.day === day
                        ? "bg-primary text-primary-foreground hover:bg-primary/90"
                        : isToday(day) && "ring-1 ring-inset ring-border",
                    )}
                  >
                    {day}
                  </button>
                ),
              )}
            </div>
          </div>
        )}

        {mode !== "date" && (
          <label className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
            {t("table.dateTimeEditor.time")}
            <input
              aria-label={t("table.dateTimeEditor.time")}
              value={text ? timePart(text, mode) : ""}
              placeholder="HH:MM:SS"
              onChange={(e) => setText((t) => withTime(t || nowText(mode), mode, e.target.value))}
              spellCheck={false}
              className={cn(inputClass, "h-7 w-28 px-2 font-mono text-xs")}
            />
          </label>
        )}

        <div className="mt-2 flex items-center gap-1 border-t border-border pt-2">
          <Button variant="ghost-muted" size="xs" onClick={() => setText(nowText(mode))}>
            {mode === "date" ? t("table.dateTimeEditor.today") : t("table.dateTimeEditor.now")}
          </Button>
          {nullable && (
            <Button variant="ghost-muted" size="xs" onClick={() => onCommit(true, "")}>
              NULL
            </Button>
          )}
          <div className="flex-1" />
          <Button variant="neutral" size="xs" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" size="xs" onClick={apply}>
            {t("table.dateTimeEditor.set")}
          </Button>
        </div>
      </div>
    </Popover>
  );
}
