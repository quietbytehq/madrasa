import { useState } from "react";
import { api } from "@/api/endpoints";
import type {
  BookBody,
  BookDetail,
  BookView,
  CopyCondition,
  CopyStatus,
  LibraryCopy,
} from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { CopyStatusBadge } from "./common";

const CONDITIONS: CopyCondition[] = ["good", "fair", "poor"];
const EDITABLE_STATUSES: CopyStatus[] = ["available", "lost", "withdrawn"];

/** Books with their copy counts; a dialog per book with its copies. */
export function CatalogueTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const books = useAsync(() => api.libraryBooks({ q: query || undefined }), [query]);
  const [open, setOpen] = useState<string | "new" | null>(null);
  return (
    <Card
      actions={
        manage && (
          <Button size="sm" onClick={() => setOpen("new")} data-testid="add-book">
            {t("lib.catalogue.add")}
          </Button>
        )
      }
    >
      <p className="mb-3 text-xs text-muted">{t("lib.catalogue.hint")}</p>
      <Input
        label={t("action.search")}
        placeholder={t("lib.catalogue.search")}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        className="mb-3 w-72"
      />
      {books.loading && !books.data ? (
        <div className="flex justify-center py-6">
          <Spinner />
        </div>
      ) : (books.data ?? []).length === 0 ? (
        <EmptyState title={t("lib.catalogue.empty")} />
      ) : (
        <Table<BookView>
          rows={books.data ?? []}
          rowKey={(b) => b.id}
          pageSize={25}
          onRowClick={(b) => setOpen(b.id)}
          columns={[
            { key: "title", header: t("lib.book.title"), sortable: true },
            { key: "author", header: t("lib.book.author"), sortable: true },
            { key: "category", header: t("lib.book.category"), sortable: true },
            { key: "copies", header: t("lib.book.copies"), align: "end", sortable: true },
            {
              key: "available",
              header: t("lib.book.available"),
              align: "end",
              render: (b) => (
                <Badge tone={b.available > 0 ? "success" : "neutral"}>{b.available}</Badge>
              ),
            },
            { key: "on_loan", header: t("lib.book.onLoan"), align: "end" },
          ]}
        />
      )}
      {open && (
        <BookDialog
          id={open === "new" ? null : open}
          manage={manage}
          onClose={() => setOpen(null)}
          onChanged={() => void books.reload()}
        />
      )}
    </Card>
  );
}

function BookDialog({
  id,
  manage,
  onClose,
  onChanged,
}: {
  id: string | null;
  manage: boolean;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const detail = useAsync(() => (id ? api.libraryBook(id) : Promise.resolve(null)), [id]);
  const d: BookDetail | null = detail.data;
  const [form, setForm] = useState<BookBody>({
    title: "",
    author: "",
    publisher: "",
    isbn: "",
    category: "",
    language: "",
    subject: "",
    year: null,
  });
  const [numbers, setNumbers] = useState("");
  const [shelf, setShelf] = useState("");
  const [loaded, setLoaded] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (d && loaded !== d.id) {
    setLoaded(d.id);
    setForm({
      title: d.title,
      author: d.author,
      publisher: d.publisher,
      isbn: d.isbn,
      category: d.category,
      language: d.language,
      subject: d.subject,
      year: d.year,
    });
  }
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const accessionList = () =>
    numbers
      .split(/[\n,;]+/)
      .map((s) => s.trim())
      .filter(Boolean);
  const save = async () => {
    setBusy(true);
    try {
      if (id) {
        detail.setData(await api.updateBook(id, form));
      } else {
        await api.createBook({ ...form, accession_nos: accessionList(), shelf });
        onClose();
      }
      toast.push({ kind: "success", title: t("lib.book.saved") });
      onChanged();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const addCopies = async () => {
    if (!id) return;
    const list = accessionList();
    if (list.length === 0) return;
    setBusy(true);
    try {
      detail.setData(await api.addCopies(id, list, shelf));
      toast.push({ kind: "success", title: t("lib.copies.added", { n: list.length }) });
      setNumbers("");
      onChanged();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const updateCopy = async (
    c: LibraryCopy,
    patch: Partial<{ shelf: string; condition: CopyCondition; status: CopyStatus }>,
  ) => {
    setBusy(true);
    try {
      await api.updateCopy(c.id, {
        shelf: c.shelf,
        condition: c.condition,
        status: c.status,
        ...patch,
      });
      await detail.reload();
      onChanged();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!id || !window.confirm(t("lib.book.deleteConfirm"))) return;
    setBusy(true);
    try {
      await api.deleteBook(id);
      onChanged();
      onClose();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const field = (k: keyof BookBody, label: string) => (
    <Input
      key={k}
      label={label}
      disabled={!manage}
      value={(form[k] as string | number | null) ?? ""}
      onChange={(e) =>
        setForm({
          ...form,
          [k]:
            k === "year" ? (e.target.value === "" ? null : Number(e.target.value)) : e.target.value,
        })
      }
      type={k === "year" ? "number" : "text"}
    />
  );
  return (
    <Dialog
      open
      onClose={onClose}
      title={id ? `${t("lib.book.edit")}: ${d?.title ?? ""}` : t("lib.catalogue.add")}
      size="lg"
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          <div>
            {manage && id && (
              <Button variant="danger" size="sm" disabled={busy} onClick={() => void remove()}>
                {t("action.delete")}
              </Button>
            )}
          </div>
          <div className="flex gap-2">
            <Button variant="secondary" onClick={onClose}>
              {t("action.close")}
            </Button>
            {manage && (
              <Button loading={busy} disabled={!form.title.trim()} onClick={() => void save()}>
                {t("action.save")}
              </Button>
            )}
          </div>
        </div>
      }
    >
      {id && detail.loading && !d ? (
        <Spinner />
      ) : (
        <div className="flex flex-col gap-4 text-sm">
          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            <div className="col-span-2">{field("title", t("lib.book.title"))}</div>
            {field("author", t("lib.book.author"))}
            {field("publisher", t("lib.book.publisher"))}
            {field("isbn", t("lib.book.isbn"))}
            {field("category", t("lib.book.category"))}
            {field("language", t("lib.book.language"))}
            {field("subject", t("lib.book.subject"))}
            {field("year", t("lib.book.year"))}
          </div>
          {manage && (
            <div className="flex flex-wrap items-end gap-3">
              <label className="flex flex-col text-xs font-medium text-muted">
                {t("lib.copies.numbers")}
                <textarea
                  value={numbers}
                  onChange={(e) => setNumbers(e.target.value)}
                  rows={2}
                  className="mt-1 w-64 rounded border border-border bg-card px-2 py-1 text-sm text-text"
                  data-testid="accession-numbers"
                />
              </label>
              <Input
                label={t("lib.copies.shelf")}
                value={shelf}
                onChange={(e) => setShelf(e.target.value)}
                className="w-32"
              />
              {id && (
                <Button
                  size="sm"
                  variant="secondary"
                  loading={busy}
                  onClick={() => void addCopies()}
                >
                  {t("lib.copies.add")}
                </Button>
              )}
            </div>
          )}
          {d && d.copy_list.length > 0 && (
            <Table<LibraryCopy>
              rows={d.copy_list}
              rowKey={(c) => c.id}
              pageSize={0}
              columns={[
                { key: "accession_no", header: t("lib.desk.accession") },
                {
                  key: "shelf",
                  header: t("lib.copies.shelf"),
                  render: (c) =>
                    manage ? (
                      <input
                        aria-label={`${c.accession_no}: ${t("lib.copies.shelf")}`}
                        defaultValue={c.shelf}
                        onBlur={(e) =>
                          e.target.value !== c.shelf &&
                          void updateCopy(c, { shelf: e.target.value })
                        }
                        className="h-7 w-24 rounded border border-border bg-card px-2 text-xs"
                      />
                    ) : (
                      c.shelf
                    ),
                },
                {
                  key: "condition",
                  header: t("lib.copy.condition"),
                  render: (c) =>
                    manage ? (
                      <Select
                        aria-label={`${c.accession_no}: ${t("lib.copy.condition")}`}
                        value={c.condition}
                        onChange={(e) =>
                          void updateCopy(c, { condition: e.target.value as CopyCondition })
                        }
                        options={CONDITIONS.map((x) => ({
                          value: x,
                          label: t(`lib.copy.condition.${x}`),
                        }))}
                        className="w-28"
                      />
                    ) : (
                      t(`lib.copy.condition.${c.condition}`)
                    ),
                },
                {
                  key: "status",
                  header: t("lib.desk.status"),
                  render: (c) =>
                    manage && c.status !== "on_loan" ? (
                      <Select
                        aria-label={`${c.accession_no}: ${t("lib.desk.status")}`}
                        value={c.status}
                        onChange={(e) =>
                          void updateCopy(c, { status: e.target.value as CopyStatus })
                        }
                        options={EDITABLE_STATUSES.map((x) => ({
                          value: x,
                          label: t(`lib.copy.status.${x}`),
                        }))}
                        className="w-32"
                      />
                    ) : (
                      <CopyStatusBadge status={c.status} />
                    ),
                },
              ]}
            />
          )}
        </div>
      )}
    </Dialog>
  );
}
