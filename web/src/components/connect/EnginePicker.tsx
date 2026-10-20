import { ChevronRight } from "lucide-react";
import { ENGINE_CARDS, type EngineCardId } from "../../lib/engines";
import { cn } from "../../lib/cn";
import EngineLogo from "./engineLogos";
import { useT } from "../../i18n";

/** First step of a new connection: which database it is. Choosing one moves on. */
export default function EnginePicker({ onPick }: { onPick: (engine: EngineCardId) => void }) {
  const t = useT();
  return (
    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
      {ENGINE_CARDS.map((card) => (
        <button
          key={card.id}
          type="button"
          disabled={!card.available}
          aria-label={card.available ? card.label : t("connect.enginePicker.comingSoonLabel", { label: card.label })}
          onClick={() => onPick(card.id)}
          className={cn(
            "group flex items-center gap-3 rounded-xl border border-border bg-card p-3 text-left outline-none transition-colors",
            "focus-visible:ring-2 focus-visible:ring-ring/50",
            card.available ? "cursor-pointer hover:border-ring/40 hover:bg-card-hover" : "cursor-not-allowed opacity-60",
          )}
        >
          <EngineLogo engine={card.id} />
          <div className="min-w-0 flex-1">
            <div className="text-[13px] font-semibold">{card.label}</div>
            <div className="text-xs text-muted-foreground">{card.available ? t(card.kindKey) : t("connect.enginePicker.comingSoon")}</div>
          </div>
          {card.available && (
            <ChevronRight size={15} className="shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5" />
          )}
        </button>
      ))}
    </div>
  );
}
