import { cn } from "@/lib/cn";
import { useI18n } from "@/i18n";
import { Spinner } from "@/components/ui/Spinner";
// The brand's own files (assets/brand, drawn by assets/brand/tools/build_svg.py).
import markUrl from "../../../../../assets/brand/madrasa-mark.svg";
import logoUrl from "../../../../../assets/brand/madrasa-logo-vertical.svg";
import logoDarkUrl from "../../../../../assets/brand/madrasa-logo-vertical-dark.svg";

/**
 * The Madrasa emblem, in the place of the old "M" square: give it the same size classes.
 * Decorative: the product or school name always stands next to it.
 */
export function BrandMark({ className }: { className?: string }) {
  return (
    <img
      src={markUrl}
      alt=""
      aria-hidden="true"
      draggable={false}
      data-testid="brand-mark"
      className={cn("shrink-0 select-none object-contain", className)}
    />
  );
}

/** The startup screen: the logo (light or dark, following the theme) over a spinner. */
export function BrandSplash() {
  const { t } = useI18n();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-6" data-testid="splash">
      <img
        src={logoUrl}
        alt={t("app.name")}
        draggable={false}
        className="h-32 w-auto select-none dark:hidden"
      />
      <img
        src={logoDarkUrl}
        alt={t("app.name")}
        draggable={false}
        className="hidden h-32 w-auto select-none dark:block"
      />
      <Spinner size="lg" className="text-blue" />
    </div>
  );
}
