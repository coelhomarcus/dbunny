import type { ReactNode } from "react";
import { cn } from "../../lib/cn";
import { segmentedItemClass, segmentedListClass } from "./styles";

type Option<T extends string> = { value: T; label: ReactNode };

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
        return (
          <button
            key={option.value}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => onChange(option.value)}
            className={cn(segmentedItemClass(active), fullWidth && "flex-1")}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
