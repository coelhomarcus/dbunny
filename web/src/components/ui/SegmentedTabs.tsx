import type { ReactNode } from "react";
import { cn } from "../../lib/cn";
import Tooltip from "../Tooltip";
import { segmentedItemClass, segmentedListClass } from "./styles";

type Option<T extends string> = {
  value: T;
  label: ReactNode;
  disabled?: boolean;
  /** Shown on hover, e.g. why the option is disabled. */
  hint?: string;
};

type SegmentedTabsProps<T extends string> = {
  value: T;
  options: Option<T>[];
  onChange: (value: T) => void;
  /** Stretch the options to fill the available width. */
  fullWidth?: boolean;
  className?: string;
};

/** Pill-style switch between a few views (e.g. Data / Structure). */
export default function SegmentedTabs<T extends string>({
  value,
  options,
  onChange,
  fullWidth,
  className,
}: SegmentedTabsProps<T>) {
  return (
    <div role="tablist" className={cn(segmentedListClass, fullWidth && "w-full", className)}>
      {options.map((option) => {
        const active = option.value === value;
        const button = (
          <button
            key={option.value}
            type="button"
            role="tab"
            aria-selected={active}
            disabled={option.disabled}
            onClick={() => onChange(option.value)}
            className={cn(
              segmentedItemClass(active),
              fullWidth && "flex-1",
              option.disabled && "cursor-not-allowed opacity-50 hover:text-muted-foreground",
            )}
          >
            {option.label}
          </button>
        );
        return option.hint ? (
          <Tooltip key={option.value} label={option.hint} side="bottom" className={cn(fullWidth && "flex flex-1 [&>button]:flex-1")}>
            {button}
          </Tooltip>
        ) : (
          button
        );
      })}
    </div>
  );
}
