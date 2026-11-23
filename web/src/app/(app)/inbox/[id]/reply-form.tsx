"use client";

import { useRouter } from "next/navigation";
import { useEffect, useRef, useState, useTransition, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";

/** Write back as the studio. Sending also tells the bot to stay out for a while. */
export function ReplyForm({ id, canReply, who }: { id: string; canReply: boolean; who: string }) {
  const router = useRouter();
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [, startTransition] = useTransition();
  const end = useRef<HTMLDivElement>(null);

  // Open on the newest message, like any chat.
  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, []);

  async function send(e: FormEvent) {
    e.preventDefault();
    if (!text.trim()) return setError("Write a message first.");
    setBusy(true);
    setError(null);
    try {
      await apiSend("POST", `/inbox/${id}/reply`, { text });
      setText("");
      startTransition(() => router.refresh());
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Couldn't send that. Try again.");
    } finally {
      setBusy(false);
    }
  }

  if (!canReply) {
    return (
      <div ref={end} className="panel p-4">
        <p className="m-0 text-sm font-semibold">You can&apos;t reply here right now.</p>
        <p className="soft m-0 mt-1 text-sm">
          WhatsApp only delivers a reply within 24 hours of the person&apos;s last message. When {who} writes again, you can answer.
        </p>
      </div>
    );
  }

  return (
    <form onSubmit={send} noValidate className="panel flex flex-col gap-2 p-4">
      <label htmlFor="reply-text" className="field-label">
        Reply to {who}
      </label>
      <textarea
        id="reply-text"
        className="field min-h-[88px]"
        maxLength={1000}
        value={text}
        placeholder="Write a message"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          // Ctrl or Cmd + Enter sends, as in most chat tools.
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) e.currentTarget.form?.requestSubmit();
        }}
      />
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p role="alert" className="field-error m-0 min-h-[1.2em]">
          {error}
        </p>
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "Sending…" : "Send on WhatsApp"}
        </button>
      </div>
      <div ref={end} />
    </form>
  );
}

/** Let the bot answer in this conversation again. */
export function HandBack({ id }: { id: string }) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handBack() {
    setBusy(true);
    setError(null);
    try {
      await apiSend("POST", `/inbox/${id}/hand-back`);
      router.refresh();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Couldn't do that. Try again.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <span className="flex flex-wrap items-center gap-2">
      {error ? (
        <span role="alert" className="field-error">
          {error}
        </span>
      ) : null}
      <button type="button" className="btn btn-ghost" disabled={busy} onClick={handBack}>
        {busy ? "Handing back…" : "Hand back to the bot"}
      </button>
    </span>
  );
}
