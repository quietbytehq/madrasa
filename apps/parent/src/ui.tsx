import type { ReactNode } from "react";
import { ArrowLeft, CloudOff, RefreshCw, TriangleAlert } from "lucide-react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/Button";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import type { TFunction, TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import type { AsyncState } from "@/lib/useAsync";
import { HubError, OfflineError } from "./api";
import { isStale } from "./format";

/** What the hub refused, in the reader's language where the app knows the reason. */
const KNOWN: Record<string, TranslationKey> = {
  not_found: "parent.error.gone",
  invalid_credentials: "parent.error.credentials",
  account_locked: "parent.error.locked",
  bad_access_code: "parent.error.code",
  account_exists: "parent.error.accountExists",
  no_class_teacher: "parent.error.noTeacher",
  too_many_messages: "parent.error.tooMany",
  unauthenticated: "parent.error.signedOut",
};

/** A plain sentence for a failed call: what happened and, where it helps, what to do. */
export function errorText(err: unknown, t: TFunction): string {
  if (err instanceof OfflineError) return t("parent.error.offline");
  if (err instanceof HubError) {
    const key = KNOWN[err.code];
    if (key) return t(key);
    // The hub's own sentence (validation messages), when it has one.
    if (err.message) return err.message;
  }
  return t("parent.error.generic");
}

/**
 * Keeps a left-to-right value (a class such as `4-A`) in one piece inside right-to-left
 * text, where it would otherwise be reordered around its dash. Adds nothing when the page
 * reads left to right.
 */
export function useLtr(): (value: string) => string {
  const { dir } = useI18n();
  return dir === "rtl" ? (value) => `\u2066${value}\u2069` : (value) => value;
}

export function isGone(err: unknown): boolean {
  return err instanceof HubError && err.status === 404;
}

/** One screen: a bar on top, the content, and room for a bar at the bottom. */
export function Screen({
  title,
  subtitle,
  back,
  actions,
  footer,
  children,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  /** Where the back arrow leads; no arrow without it. */
  back?: string;
  actions?: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
}) {
  const { t } = useI18n();
  return (
    <div className="mx-auto flex h-full w-full max-w-md flex-col bg-bg">
      <header className="safe-top shrink-0 border-b border-border bg-card">
        <div className="flex min-h-14 items-center gap-2 px-3 py-2">
          {back && (
            <Link
              to={back}
              aria-label={t("action.back")}
              className="-ms-1 flex h-10 w-10 shrink-0 items-center justify-center rounded text-muted hover:bg-hover hover:text-text"
            >
              <ArrowLeft className="h-5 w-5 rtl:rotate-180" aria-hidden />
            </Link>
          )}
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-base font-semibold text-navy">{title}</h1>
            {subtitle && <div className="truncate text-xs text-muted">{subtitle}</div>}
          </div>
          {actions}
        </div>
      </header>
      <main className="scrollbar-thin flex-1 overflow-y-auto px-3 py-3">{children}</main>
      {footer}
    </div>
  );
}

/** A problem, said plainly, with a way to try again. */
export function Problem({
  error,
  onRetry,
  children,
}: {
  error: unknown;
  onRetry?: () => void;
  children?: ReactNode;
}) {
  const { t } = useI18n();
  const offline = error instanceof OfflineError;
  const Icon = offline ? CloudOff : TriangleAlert;
  return (
    <div role="alert" className="flex flex-col items-center gap-3 px-4 py-10 text-center">
      <Icon className="h-8 w-8 text-muted" aria-hidden />
      <p className="text-sm text-text">{errorText(error, t)}</p>
      {children}
      {onRetry && !isGone(error) && (
        <Button
          variant="secondary"
          icon={<RefreshCw className="h-4 w-4" aria-hidden />}
          onClick={onRetry}
        >
          {t("action.retry")}
        </Button>
      )}
    </div>
  );
}

/** Loading, a problem, or the content once it is there. */
export function Loadable<T>({
  state,
  children,
}: {
  state: AsyncState<T>;
  children: (data: T) => ReactNode;
}) {
  const { t } = useI18n();
  if (state.data === null && state.loading) {
    return (
      <div className="flex justify-center py-10" role="status" aria-label={t("app.loading")}>
        <Spinner />
      </div>
    );
  }
  if (state.data === null || (state.error && isGone(state.error))) {
    return (
      <Problem error={state.error} onRetry={() => void state.reload()}>
        {isGone(state.error) && (
          <Link to="/" className="text-sm font-medium text-blue underline">
            {t("parent.children.back")}
          </Link>
        )}
      </Problem>
    );
  }
  return (
    <>
      {state.error !== null && (
        <p role="alert" className="mb-3 rounded bg-warning-bg px-3 py-2 text-xs text-text">
          {errorText(state.error, t)}
        </p>
      )}
      {children(state.data)}
    </>
  );
}

/**
 * "Last updated": when the school last sent its records to the school network. Shown on
 * every screen about a child, with a warning once the school has been quiet for a day.
 */
export function LastUpdated({ at, className }: { at: string | null; className?: string }) {
  const { t, formatDate } = useI18n();
  const stale = isStale(at);
  return (
    <p
      data-testid="last-updated"
      className={cn(
        "rounded px-3 py-2 text-xs",
        stale ? "bg-warning-bg text-text" : "text-muted",
        className,
      )}
    >
      {at
        ? t("parent.updated", {
            when: formatDate(at, { dateStyle: "medium", timeStyle: "short" }),
          })
        : t("parent.updated.never")}
      {stale && at && <span className="block">{t("parent.updated.stale")}</span>}
    </p>
  );
}

/** A titled block of content: the card of the desktop app, sized for a phone. */
export function Panel({
  title,
  aside,
  children,
  className,
}: {
  title?: ReactNode;
  aside?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("rounded-lg border border-border bg-card p-3 shadow-card", className)}>
      {(title || aside) && (
        <div className="mb-2 flex items-center justify-between gap-2">
          {title && <h2 className="text-sm font-semibold text-text">{title}</h2>}
          {aside}
        </div>
      )}
      {children}
    </section>
  );
}
