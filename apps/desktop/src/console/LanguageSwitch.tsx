import { LOCALES, useI18n } from "@/i18n";
import { cn } from "@/lib/cn";

/** English / اردو. Shown on every console screen, signed in or not. */
export function LanguageSwitch({ className }: { className?: string }) {
  const { t, locale, setLocale } = useI18n();
  return (
    <div role="group" aria-label={t("topbar.language")} className={cn("flex gap-1", className)}>
      {LOCALES.map((l) => (
        <button
          key={l}
          type="button"
          aria-pressed={locale === l}
          onClick={() => setLocale(l)}
          className={cn(
            "rounded px-2 py-1 text-xs",
            locale === l ? "bg-selection font-semibold text-blue" : "text-muted hover:bg-hover",
          )}
        >
          {t(`lang.${l}`)}
        </button>
      ))}
    </div>
  );
}
