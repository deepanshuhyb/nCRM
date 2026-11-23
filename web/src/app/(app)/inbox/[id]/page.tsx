import type { Metadata } from "next";

import { NotAllowed } from "@/components/not-allowed";
import { Travels } from "@/components/travel";
import { Page, PageHeader } from "@/components/ui";
import { getMe, getSummary, getThread } from "@/lib/api-server";
import { formatPhone } from "@/lib/format";
import type { ChatMessage } from "@/lib/types";

import { KeepFresh } from "../keep-fresh";
import { dayLabel, whoIs } from "../words";
import { HandBack, ReplyForm } from "./reply-form";

export const metadata: Metadata = { title: "Conversation" };

const STATUS: Record<ChatMessage["status"], string> = {
  received: "",
  queued: "Sending…",
  sending: "Sending…",
  sent: "Sent",
  delivered: "Delivered",
  read: "Read",
  failed: "Not delivered",
};

export default async function ThreadPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const me = await getMe();
  const studio = me.studio!;
  if (!studio.capabilities.includes("use_inbox")) return <NotAllowed what="the inbox" />;
  const [thread, summary] = await Promise.all([getThread(id), getSummary()]);
  const who = whoIs(thread);

  // Messages grouped under the day they were sent, in the studio's time.
  const days: { day: string; messages: ChatMessage[] }[] = [];
  for (const m of thread.messages) {
    const day = m.on_date;
    if (days.length === 0 || days[days.length - 1].day !== day) days.push({ day, messages: [] });
    days[days.length - 1].messages.push(m);
  }

  return (
    <Page narrow>
      <KeepFresh />
      <PageHeader
        back={{ href: "/inbox", label: "Inbox" }}
        title={
          <Travels kind="chat" id={thread.id}>
            <h1 className="h-page">{who}</h1>
          </Travels>
        }
        lead={
          thread.contact_name
            ? `${thread.students ? `${thread.students} · ` : ""}${formatPhone(thread.phone)}`
            : `Not in your records${thread.profile_name ? `. On WhatsApp they go by "${thread.profile_name}"` : ""}.`
        }
      />

      {thread.needs_staff || thread.bot_paused ? (
        <div className={`panel flex flex-wrap items-center justify-between gap-3 p-4 ${thread.needs_staff ? "panel-attn" : ""}`}>
          <p className="m-0 text-sm font-semibold">
            {thread.needs_staff ? "The bot passed this to you and is staying quiet." : "You're answering here, so the bot is staying quiet."}
          </p>
          <HandBack id={thread.id} />
        </div>
      ) : null}

      <section aria-label="Messages" className="panel flex flex-col gap-4 p-4 sm:p-5">
        {days.map((d) => (
          <div key={d.day} className="flex flex-col gap-2">
            <p className="caption m-0 text-center font-semibold">{d.day === summary.today ? "Today" : dayLabel(d.day)}</p>
            <ol className="m-0 flex list-none flex-col gap-2 p-0">
              {d.messages.map((m) => (
                <li key={m.id} className="bubble" data-from={m.direction === "in" ? "them" : m.author === "staff" ? "us" : "bot"}>
                  <p className="m-0 whitespace-pre-wrap break-words">{m.body}</p>
                  <p className="bubble-meta m-0">
                    {m.direction === "out" ? `${m.author === "bot" ? "Bot" : m.author === "system" ? "Reminder" : (m.staff_name ?? "Team")} · ` : ""}
                    {m.at_time}
                    {STATUS[m.status] ? ` · ${STATUS[m.status]}` : ""}
                  </p>
                </li>
              ))}
            </ol>
          </div>
        ))}
      </section>

      <ReplyForm id={thread.id} canReply={thread.can_reply} who={who} />
    </Page>
  );
}
