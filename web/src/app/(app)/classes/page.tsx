import type { Metadata } from "next";
import Link from "next/link";

import { Timetable } from "@/components/timetable";
import { Page } from "@/components/ui";
import { getMe, getSummary, listBatches, listBranches } from "@/lib/api-server";
import { isoWeekday, scheduleSummary } from "@/lib/format";

export const metadata: Metadata = { title: "Classes" };

export default async function ClassesPage({ searchParams }: PageProps<"/classes">) {
  const params = await searchParams;
  const branchParam = typeof params.branch === "string" ? params.branch : undefined;
  const showArchived = params.archived === "1";

  const [me, branches, summary] = await Promise.all([getMe(), listBranches(), getSummary()]);
  const studio = me.studio!;
  const can = new Set(studio.capabilities);
  const branch = branches.find((b) => b.id === branchParam);
  const [running, everything] = await Promise.all([
    listBatches({ branchId: branch?.id }),
    showArchived ? listBatches({ branchId: branch?.id, archived: true }) : Promise.resolve(null),
  ]);
  const archived = everything?.filter((b) => !b.active) ?? [];
  const scheduled = running.filter((b) => b.slots.length > 0);
  const full = running.filter((b) => b.capacity !== null && b.enrolled >= b.capacity).length;
  const places = running.reduce((n, b) => n + b.enrolled, 0);

  const href = (next: { branch?: string | null; archived?: boolean }) => {
    const sp = new URLSearchParams();
    const b = next.branch === undefined ? branch?.id : next.branch;
    if (b) sp.set("branch", b);
    if (next.archived ?? showArchived) sp.set("archived", "1");
    const qs = sp.toString();
    return `/classes${qs ? `?${qs}` : ""}`;
  };

  return (
    <Page>
      <div className="page-head">
        <div>
          <h1 className="h-page">Classes</h1>
          <p className="lead">
            {running.length === 0
              ? "No classes yet."
              : `${running.length} ${running.length === 1 ? "class" : "classes"}, ${places} ${places === 1 ? "student" : "students"} in them${
                  full > 0 ? `. ${full} ${full === 1 ? "is" : "are"} full.` : "."
                }`}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          {can.has("manage_students") ? (
            <Link href="/classes/plans" className="btn btn-ghost">
              Prices and packs
            </Link>
          ) : null}
          {can.has("manage_classes") ? (
            <Link href="/classes/new" className="btn btn-primary">
              Add a class
            </Link>
          ) : null}
        </div>
      </div>

      {branches.length > 1 ? (
        <nav aria-label="Branch" className="segmented self-start">
          <Link href={href({ branch: null })} aria-current={!branch ? "page" : undefined}>
            All branches
          </Link>
          {branches
            .filter((b) => b.active)
            .map((b) => (
              <Link key={b.id} href={href({ branch: b.id })} aria-current={branch?.id === b.id ? "page" : undefined}>
                {b.name}
              </Link>
            ))}
        </nav>
      ) : null}

      {scheduled.length === 0 ? (
        <section className="panel p-7">
          <p className="m-0 text-base font-semibold">
            {can.has("manage_classes")
              ? "Add your first class: its days, times, room and teacher."
              : "No classes here yet. An owner or manager sets them up."}
          </p>
          {can.has("manage_classes") ? (
            <Link href="/classes/new" className="btn btn-primary mt-4">
              Add a class
            </Link>
          ) : null}
        </section>
      ) : (
        <section aria-label="This week's timetable" className="panel p-3 sm:p-4">
          <Timetable
            batches={scheduled}
            todayWeekday={isoWeekday(summary.today)}
            myStaffId={studio.staff_id}
            showBranch={!branch && branches.length > 1}
          />
        </section>
      )}

      <div>
        <Link href={href({ archived: !showArchived })} className="btn btn-quiet px-0">
          {showArchived ? "Hide archived classes" : "Show archived classes"}
        </Link>
        {showArchived ? (
          archived.length === 0 ? (
            <p className="muted m-0">No archived classes.</p>
          ) : (
            <ul className="panel m-0 mt-2 flex list-none flex-col gap-1.5 p-2">
              {archived.map((b) => (
                <li key={b.id}>
                  <Link href={`/classes/${b.id}`} className="row list-row">
                    <span className="list-row-title min-w-0 flex-1">{b.name}</span>
                    <span className="muted text-[13px]">{scheduleSummary(b.slots)}</span>
                    <span className="tag tag-quiet">Archived</span>
                  </Link>
                </li>
              ))}
            </ul>
          )
        ) : null}
      </div>
    </Page>
  );
}
