import type { ComponentProps } from "react";
import { cn } from "../../lib/cn";
import { inputClass } from "./styles";

export default function Input({ className, ...props }: ComponentProps<"input">) {
  return <input className={cn(inputClass, className)} {...props} />;
}
