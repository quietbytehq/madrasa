import { cn } from "@/lib/cn";
import { useI18n } from "@/i18n";

export interface SpinnerProps {
  size?: "sm" | "md" | "lg";
  className?: string;
  /** Accessible label; defaults to the translated "Loading…". */
  label?: string;
}

const sizeClass = {
  sm: "h-3.5 w-3.5 border-2",
  md: "h-5 w-5 border-2",
  lg: "h-8 w-8 border-[3px]",
};

export function Spinner({ size = "md", className, label }: SpinnerProps) {
  const { t } = useI18n();
  return (
    <span
      role="status"
      aria-label={label ?? t("app.loading")}
      className={cn(
        "inline-block animate-spin rounded-full border-current border-t-transparent",
        sizeClass[size],
        className,
      )}
    />
  );
}
