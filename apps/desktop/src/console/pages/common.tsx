import type { ReactNode } from "react";

export function PageTitle({
  title,
  about,
  actions,
}: {
  title: ReactNode;
  about?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="mb-4 flex flex-wrap items-end justify-between gap-2">
      <div className="min-w-0">
        <h1 className="text-lg font-semibold text-navy">{title}</h1>
        {about && <p className="mt-0.5 max-w-3xl text-xs text-muted rtl:leading-loose">{about}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  );
}
