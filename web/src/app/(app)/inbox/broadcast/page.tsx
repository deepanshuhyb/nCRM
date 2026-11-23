import type { Metadata } from "next";

import { NotAllowed } from "@/components/not-allowed";
import { EmptyState, Page, PageHeader, SectionHeader } from "@/components/ui";
import { getMe, getMessagingSettings, getSummary, listBatches, listBranches, listBroadcasts } from "@/lib/api-server";

import { dayLabel } from "../words";
import { BroadcastForm } from "./broadcast-form";

export const metadata: Metadata = { title: "Message a group" };

export default async function BroadcastPage() {
  const me = await getMe();
  const studio = me.studio!;
  if (!studio.capabilities.includes("manage_messaging")) return <NotAllowed what="group messages" />;
  const [branches, batches, settings, sent, summary] = await Promise.all([
    listBranches(),
    listBatches(),
    getMessagingSettings(),
    listBroadcasts(),
    getSummary(),
  ]);

  return (
    <Page narrow>
      <PageHeader
        back={{ href: "/inbox", label: "Inbox" }}
        title="Message a group"
        lead="One WhatsApp message to many people. Each student's main contact gets it once."
      />

      <BroadcastForm
        branches={branches.filter((b) => b.active).map((b) => ({ id: b.id, name: b.name }))}
        classes={batches.map((b) => ({ id: b.id, name: b.name, where: b.branch_name }))}
        live={settings.live}
        connected={settings.connected}
      />

      <section aria-labelledby="sent-title" className="panel flex flex-col gap-3 p-4 sm:p-5">
        <SectionHeader id="sent-title" title="Sent before" aside={sent.length > 0 ? String(sent.length) : undefined} />
        {sent.length === 0 ? (
          <EmptyState title="Nothing sent yet">Messages you send to a group are listed here, with how many arrived.</EmptyState>
        ) : (
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {sent.map((b) => (
              <li key={b.id} className="row flex flex-col gap-1 px-3.5 py-3">
                <p className="m-0 font-semibold">{b.body}</p>
                <p className="caption m-0">
                  To {b.audience_label} · {b.on_date === summary.today ? b.at_time : dayLabel(b.on_date)} · {b.sent_by}
                </p>
                {/* Counts in words: nothing here depends on colour. */}
                <p className="m-0 text-sm tabular-nums">
                  {b.waiting > 0 ? `${b.waiting} still going out. ` : ""}
                  {b.sent} of {b.recipients} sent
                  {b.delivered > 0 ? `, ${b.delivered} delivered` : ""}
                  {b.read > 0 ? `, ${b.read} read` : ""}
                  {b.failed > 0 ? <span className="attn-text font-semibold">{`, ${b.failed} not delivered`}</span> : null}.
                </p>
              </li>
            ))}
          </ul>
        )}
      </section>
    </Page>
  );
}
