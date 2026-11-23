import type { Metadata } from "next";

import { NotAllowed } from "@/components/not-allowed";
import { TravelLink, Travels } from "@/components/travel";
import { EmptyState, Page, PageHeader } from "@/components/ui";
import { getMe, getMessagingSettings, getSummary, listInbox } from "@/lib/api-server";
import { formatPhone } from "@/lib/format";

import { BotSettings } from "./bot-settings";
import { KeepFresh } from "./keep-fresh";
import { whenShort, whoIs } from "./words";

export const metadata: Metadata = { title: "Inbox" };

export default async function InboxPage() {
  const me = await getMe();
  const studio = me.studio!;
  if (!studio.capabilities.includes("use_inbox")) return <NotAllowed what="the inbox" />;
  const [inbox, settings, summary] = await Promise.all([listInbox(), getMessagingSettings(), getSummary()]);
  const waiting = inbox.filter((c) => c.needs_staff).length;

  return (
    <Page narrow>
      <KeepFresh />
      <PageHeader
        title="Inbox"
        lead={
          inbox.length === 0
            ? "WhatsApp messages to the studio land here."
            : waiting > 0
              ? `${waiting} ${waiting === 1 ? "conversation is" : "conversations are"} waiting for a person.`
              : "Nobody is waiting. The bot is answering what it can."
        }
      />

      {!settings.connected ? (
        <p className="panel panel-attn m-0 p-4 text-sm">
          <strong>This studio has no WhatsApp number yet.</strong> Whoever runs the server connects one. Until then nothing arrives here.
        </p>
      ) : !settings.live ? (
        <p className="panel m-0 p-4 text-sm">
          <strong>Test mode.</strong> WhatsApp isn&apos;t connected on this server, so replies are saved and shown here but reach nobody.
        </p>
      ) : null}

      <BotSettings settings={settings} canChange={studio.capabilities.includes("manage_messaging")} />

      <section aria-label="Conversations" className="panel flex flex-col gap-1.5 p-3 sm:p-4">
        {inbox.length === 0 ? (
          <EmptyState title="No messages yet">When a parent or student writes to the studio&apos;s number, it shows up here.</EmptyState>
        ) : (
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0" data-live>
            {inbox.map((c) => {
              const who = whoIs(c);
              return (
                <li key={c.id}>
                  <TravelLink
                    kind="chat"
                    id={c.id}
                    label={who}
                    href={`/inbox/${c.id}`}
                    className="row grid min-h-[64px] grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-0.5 px-3.5 py-2.5 no-underline"
                  >
                    <span className="flex min-w-0 items-baseline gap-2">
                      <Travels kind="chat" id={c.id}>
                        <span className={`min-w-0 flex-[0_1_auto] truncate ${c.unread > 0 ? "font-extrabold" : "font-bold"}`}>{who}</span>
                      </Travels>
                      {/* The name keeps its room; this line gives way first. */}
                      <span className="caption min-w-0 flex-[1_20_0%] truncate">{c.students ?? (c.contact_name ? formatPhone(c.phone) : "Not in your records")}</span>
                    </span>
                    <span className="caption tabular-nums" title={c.last_message_at}>
                      {whenShort(c, summary.today)}
                    </span>
                    <span className={`truncate text-sm ${c.unread > 0 ? "font-semibold" : "soft"}`}>{c.preview ?? ""}</span>
                    <span className="flex items-center justify-end gap-1.5">
                      {c.needs_staff ? <span className="tag tag-attn">Needs you</span> : null}
                      {c.unread > 0 ? (
                        <span className="tag tag-dark tabular-nums">
                          {c.unread}
                          <span className="sr-only"> unread</span>
                        </span>
                      ) : null}
                    </span>
                  </TravelLink>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </Page>
  );
}
