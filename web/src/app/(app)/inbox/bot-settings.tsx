"use client";

import { useRouter } from "next/navigation";
import { useState, useTransition } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import type { BotMode, MessagingSettings } from "@/lib/types";

const MODES: { value: BotMode; label: string; detail: string }[] = [
  { value: "menu", label: "Menu", detail: "A fixed menu: classes, fees, classes left, can't come, talk to the team. The same answer every time." },
  { value: "ai", label: "AI", detail: "Answers in its own words, from the same facts the menu shows. It can't change anything, and hands over when unsure." },
  { value: "off", label: "Off", detail: "Nothing is answered automatically. Every message waits for a person." },
];

/** How the bot answers. Owners and managers choose; front desk can see. */
export function BotSettings({ settings, canChange }: { settings: MessagingSettings; canChange: boolean }) {
  const router = useRouter();
  const [mode, setMode] = useState(settings.bot_mode);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const current = MODES.find((m) => m.value === mode)!;

  async function choose(next: BotMode) {
    if (next === mode) return;
    const before = mode;
    setMode(next);
    setError(null);
    try {
      await apiSend<MessagingSettings>("PUT", "/messaging/settings", { bot_mode: next });
      startTransition(() => router.refresh());
    } catch (err) {
      setMode(before);
      setError(err instanceof ApiError ? err.message : "Couldn't save that. Try again.");
    }
  }

  return (
    <details className="panel disclosure">
      <summary className="flex min-h-[56px] items-center gap-2 px-5 font-semibold">
        The bot
        <span className="tag tag-quiet">{current.label}</span>
      </summary>
      <div className="flex flex-col gap-3 px-5 pb-5">
        <div className="segmented self-start" role="group" aria-label="How the bot answers">
          {MODES.map((m) => (
            <button key={m.value} type="button" aria-pressed={mode === m.value} disabled={!canChange || pending} onClick={() => choose(m.value)}>
              {m.label}
            </button>
          ))}
        </div>
        <p className="soft m-0 text-sm">{current.detail}</p>
        {mode === "ai" && !settings.ai_available ? (
          <p className="attn-text m-0 text-sm font-semibold">This server has no AI key yet, so the menu answers for now.</p>
        ) : null}
        {mode === "ai" ? (
          <p className="caption m-0">
            In AI mode the person&apos;s message, their students&apos; names, classes and fees, and the timetable are sent to the AI provider to write each reply.
          </p>
        ) : null}
        {!canChange ? <p className="caption m-0">The owner or a manager can change this.</p> : null}
        <p role="alert" className="field-error m-0 empty:hidden">
          {error}
        </p>
      </div>
    </details>
  );
}
