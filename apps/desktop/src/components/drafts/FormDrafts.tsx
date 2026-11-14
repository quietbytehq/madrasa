import { useEffect, useRef, useState } from "react";
import { History } from "lucide-react";
import { OFFLINE_EVENTS } from "@/api/client";
import { Button } from "@/components/ui/Button";
import { useI18n } from "@/i18n";
import {
  deleteDraft,
  fillForm,
  formShape,
  loadDraft,
  readForm,
  sameFields,
  saveDraft,
} from "@/lib/drafts";
import type { Draft } from "@/lib/drafts";

/** How long after a save the form that was just used counts as "the one saved". */
const SAVED_WINDOW_MS = 10_000;
const DEBOUNCE_MS = 500;

interface Offer {
  key: string;
  form: HTMLFormElement;
  draft: Draft;
}

function formTitle(form: HTMLFormElement): string {
  const dialog = form.closest('[role="dialog"]');
  const heading =
    dialog?.querySelector("h1, h2, h3") ??
    form.querySelector("legend, h1, h2, h3") ??
    document.querySelector("main h1");
  return heading?.textContent?.trim() ?? "";
}

/**
 * Autosave every form on screen as an encrypted draft on this PC (ADR 0009), and offer it back
 * when the same person opens the same form again: after a crash, a restart, or a dialog closed
 * by mistake. A form's draft is removed once the change reaches the server or is kept in the
 * LAN outbox.
 *
 * Works on the page as it is, without changes to each form: it listens to typing in any
 * `<form>`. Passwords and fields marked `data-no-draft` are never kept.
 */
export function FormDrafts({ user }: { user: string | null }) {
  const { t, formatDate } = useI18n();
  const [offers, setOffers] = useState<Offer[]>([]);
  const keys = useRef(new WeakMap<HTMLFormElement, string>());
  const timers = useRef(new Map<HTMLFormElement, ReturnType<typeof setTimeout>>());
  const lastUsed = useRef<{ form: HTMLFormElement; at: number } | null>(null);
  const submitted = useRef(new Map<HTMLFormElement, number>());

  useEffect(() => {
    if (!user) return;
    const keyOf = (form: HTMLFormElement): string => {
      let key = keys.current.get(form);
      if (!key) {
        const shape = form.dataset.draftKey ?? formShape(form);
        const same = Array.from(document.forms).filter(
          (f) => (f.dataset.draftKey ?? formShape(f)) === shape,
        );
        key = `${user}|${window.location.pathname}${window.location.hash}|${shape}|${Math.max(0, same.indexOf(form))}`;
        keys.current.set(form, key);
      }
      return key;
    };

    const offer = async (form: HTMLFormElement) => {
      const fields = readForm(form);
      if (fields.length === 0) return;
      const key = keyOf(form);
      const draft = await loadDraft(key);
      if (!draft || !form.isConnected) return;
      if (sameFields(draft.fields, readForm(form))) {
        void deleteDraft(key);
        return;
      }
      setOffers((list) =>
        list.some((o) => o.key === key) ? list : [...list, { key, form, draft }],
      );
    };

    const save = (form: HTMLFormElement) => {
      const fields = readForm(form);
      if (fields.length === 0) return;
      void saveDraft({
        key: keyOf(form),
        user,
        path: `${window.location.pathname}${window.location.hash}`,
        title: formTitle(form),
        at: new Date().toISOString(),
        fields,
      });
    };

    const onInput = (e: Event) => {
      const target = e.target as HTMLElement | null;
      const form = (target as HTMLInputElement | null)?.form ?? target?.closest?.("form");
      if (!(form instanceof HTMLFormElement)) return;
      lastUsed.current = { form, at: Date.now() };
      // Typing over an offered draft means it is not wanted.
      setOffers((list) => list.filter((o) => o.form !== form));
      const prev = timers.current.get(form);
      if (prev) clearTimeout(prev);
      timers.current.set(
        form,
        setTimeout(() => save(form), DEBOUNCE_MS),
      );
    };

    const onSubmit = (e: Event) => {
      if (e.target instanceof HTMLFormElement) submitted.current.set(e.target, Date.now());
    };

    // A change reached the server (or the outbox): the form just used is done with.
    const onSaved = () => {
      const now = Date.now();
      const done = new Set<HTMLFormElement>();
      for (const [form, at] of submitted.current) if (now - at < SAVED_WINDOW_MS) done.add(form);
      if (lastUsed.current && now - lastUsed.current.at < SAVED_WINDOW_MS)
        done.add(lastUsed.current.form);
      for (const form of done) {
        const timer = timers.current.get(form);
        if (timer) clearTimeout(timer);
        timers.current.delete(form);
        submitted.current.delete(form);
        void deleteDraft(keyOf(form));
      }
      setOffers((list) => list.filter((o) => !done.has(o.form)));
    };

    const seen = new WeakSet<HTMLFormElement>();
    const scan = () => {
      for (const form of Array.from(document.forms)) {
        if (seen.has(form)) continue;
        seen.add(form);
        void offer(form);
      }
      setOffers((list) => list.filter((o) => o.form.isConnected));
    };
    const observer = new MutationObserver(() => scan());
    observer.observe(document.body, { childList: true, subtree: true });
    scan();

    document.addEventListener("input", onInput, true);
    document.addEventListener("change", onInput, true);
    document.addEventListener("submit", onSubmit, true);
    window.addEventListener(OFFLINE_EVENTS.saved, onSaved);
    window.addEventListener(OFFLINE_EVENTS.queued, onSaved);
    const pending = timers.current;
    return () => {
      observer.disconnect();
      document.removeEventListener("input", onInput, true);
      document.removeEventListener("change", onInput, true);
      document.removeEventListener("submit", onSubmit, true);
      window.removeEventListener(OFFLINE_EVENTS.saved, onSaved);
      window.removeEventListener(OFFLINE_EVENTS.queued, onSaved);
      for (const timer of pending.values()) clearTimeout(timer);
      pending.clear();
    };
  }, [user]);

  if (offers.length === 0) return null;
  return (
    <div
      className="pointer-events-none fixed bottom-3 start-3 z-50 flex w-96 max-w-[calc(100vw-1.5rem)] flex-col gap-2 print:hidden"
      data-testid="draft-offers"
    >
      {offers.map((o) => (
        <div
          key={o.key}
          role="status"
          className="pointer-events-auto rounded-lg border border-border bg-card p-3 text-sm shadow-pop"
          data-testid="draft-offer"
        >
          <div className="flex items-start gap-2">
            <History className="mt-0.5 h-4 w-4 shrink-0 text-blue" aria-hidden />
            <p className="text-text">
              {o.draft.title
                ? t("drafts.offer.titled", {
                    title: o.draft.title,
                    time: formatDate(o.draft.at, { dateStyle: "medium", timeStyle: "short" }),
                  })
                : t("drafts.offer", {
                    time: formatDate(o.draft.at, { dateStyle: "medium", timeStyle: "short" }),
                  })}
            </p>
          </div>
          <div className="mt-2 flex justify-end gap-2">
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                void deleteDraft(o.key);
                setOffers((list) => list.filter((x) => x.key !== o.key));
              }}
            >
              {t("drafts.discard")}
            </Button>
            <Button
              size="sm"
              data-testid="draft-restore"
              onClick={() => {
                fillForm(o.form, o.draft.fields);
                setOffers((list) => list.filter((x) => x.key !== o.key));
              }}
            >
              {t("drafts.restore")}
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
}
