import type { Metadata } from "next";
import Link from "next/link";

import { NotAllowed } from "@/components/not-allowed";
import { TravelLink, Travels } from "@/components/travel";
import { Page } from "@/components/ui";
import { getMe, listStudents } from "@/lib/api-server";
import { formatPhone } from "@/lib/format";
import { STATUS_LABEL, type StudentStatus } from "@/lib/types";

export const metadata: Metadata = { title: "Students" };

const FILTERS: { value: StudentStatus | "all"; label: string }[] = [
  { value: "active", label: "Active" },
  { value: "paused", label: "On a break" },
  { value: "left", label: "Left" },
  { value: "all", label: "Everyone" },
];

export default async function StudentsPage({ searchParams }: PageProps<"/students">) {
  const me = await getMe();
  if (!me.studio!.capabilities.includes("manage_students")) return <NotAllowed what="students" />;

  const params = await searchParams;
  const q = typeof params.q === "string" ? params.q.trim().slice(0, 80) : "";
  const rawStatus = typeof params.status === "string" ? params.status : "active";
  const status = FILTERS.some((f) => f.value === rawStatus) ? (rawStatus as StudentStatus | "all") : "active";
  const pageNo = Math.max(1, Math.min(2000, Number.parseInt(String(params.page ?? "1"), 10) || 1));

  const page = await listStudents({
    q: q || undefined,
    status: status === "all" ? undefined : status,
    offset: (pageNo - 1) * 50,
  });
  const lastPage = Math.max(1, Math.ceil(page.total / page.limit));

  const href = (next: { status?: string; page?: number }) => {
    const sp = new URLSearchParams();
    if (q) sp.set("q", q);
    sp.set("status", next.status ?? status);
    if (next.page && next.page > 1) sp.set("page", String(next.page));
    return `/students?${sp.toString()}`;
  };

  return (
    <Page>
      <div className="page-head">
        <div>
          <h1 className="h-page">Students</h1>
          <p className="lead">
            {page.total === 0
              ? q
                ? `No one matches "${q}".`
                : "No students here yet."
              : `${page.total} ${page.total === 1 ? "student" : "students"}${q ? ` matching "${q}"` : ""}.`}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Link href="/students/import" className="btn btn-ghost">
            Import from Excel
          </Link>
          <Link href="/students/new" className="btn btn-primary">
            Add student
          </Link>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <form action="/students" method="get" role="search" className="flex min-w-[260px] flex-1 gap-2">
          <label htmlFor="student-search" className="sr-only">
            Search students
          </label>
          <input
            id="student-search"
            name="q"
            type="search"
            defaultValue={q}
            placeholder="Name or phone number"
            className="field"
            maxLength={80}
            autoComplete="off"
          />
          <input type="hidden" name="status" value={status} />
          <button type="submit" className="btn btn-dark">
            Search
          </button>
        </form>
        <nav aria-label="Filter by status" className="segmented">
          {FILTERS.map((f) => (
            <Link key={f.value} href={href({ status: f.value })} aria-current={f.value === status ? "page" : undefined}>
              {f.label}
            </Link>
          ))}
        </nav>
      </div>

      {page.items.length === 0 ? (
        <section className="panel p-7">
          <p className="m-0 text-base font-semibold">
            {q ? "Try part of the name, or 4 or more digits of a phone number." : "Add your first student, or bring in your list from Excel."}
          </p>
          {q ? null : (
            <Link href="/students/import" className="btn btn-ghost mt-4">
              Import from Excel
            </Link>
          )}
        </section>
      ) : (
        <section aria-label="Student list" className="panel p-2">
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {page.items.map((s) => (
              <li key={s.id}>
                <TravelLink
                  kind="student"
                  id={s.id}
                  label={s.name}
                  href={`/students/${s.id}`}
                  className="row grid min-h-[56px] grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-0.5 px-3.5 py-2.5 no-underline sm:grid-cols-[minmax(0,5fr)_minmax(0,5fr)_minmax(0,3fr)_auto]"
                >
                  <Travels kind="student" id={s.id}>
                    <span className="truncate font-bold">{s.name}</span>
                  </Travels>
                  <span className="caption order-3 col-span-2 truncate sm:order-none sm:col-span-1">
                    {s.primary_contact_name ? (
                      <>
                        {s.primary_contact_name}
                        {s.primary_contact_phone ? ` · ${formatPhone(s.primary_contact_phone)}` : ""}
                      </>
                    ) : (
                      <span className="attn-text font-semibold">No contact yet</span>
                    )}
                  </span>
                  <span className="caption hidden truncate sm:block">{s.branch_name ?? "No branch"}</span>
                  {s.status !== "active" || status === "all" ? (
                    <span className={s.status === "active" ? "tag tag-good" : s.status === "paused" ? "tag tag-quiet" : "tag tag-attn"}>{STATUS_LABEL[s.status]}</span>
                  ) : (
                    <span />
                  )}
                </TravelLink>
              </li>
            ))}
          </ul>
        </section>
      )}

      {lastPage > 1 ? (
        <nav aria-label="Pages" className="flex items-center justify-between gap-3">
          {pageNo > 1 ? (
            <Link href={href({ page: pageNo - 1 })} className="btn btn-ghost">
              Previous
            </Link>
          ) : (
            <span />
          )}
          <span className="muted text-sm">
            Page {pageNo} of {lastPage}
          </span>
          {pageNo < lastPage ? (
            <Link href={href({ page: pageNo + 1 })} className="btn btn-ghost">
              Next
            </Link>
          ) : (
            <span />
          )}
        </nav>
      ) : null}
    </Page>
  );
}
