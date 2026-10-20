import { useT } from "../../i18n";
/**
 * The loading mark of the app: twelve spokes that fade into a tail and turn in steps. It is drawn in
 * the current text color, so it takes the color of what it sits in (white on a filled button, muted in
 * a quiet line); size it like an icon, with `size` or a `size-*` class.
 */
export default function Spinner({ size = 16, className }: { size?: number; className?: string }) {
  const t = useT();
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" role="status" aria-label={t("ui.spinner.loading")} className={className}>
      <g className="spinner-spokes">
        {[0.14, 0.29, 0.43, 0.57, 0.71, 0.86, 1].map((opacity, index) => (
          <rect key={index} x="11" y="1" width="2" height="5" rx="1" opacity={opacity} transform={index === 0 ? undefined : `rotate(${index * 30} 12 12)`} />
        ))}
      </g>
    </svg>
  );
}
