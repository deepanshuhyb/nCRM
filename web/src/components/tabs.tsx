"use client";

import { useState, useSyncExternalStore, type KeyboardEvent, type ReactNode } from "react";

export type Tab = { id: string; label: string; badge?: ReactNode };

function subscribe(onChange: () => void) {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

/**
 * Tabs over panels the server already rendered: switching is instant and
 * needs no request. Every panel is in the page (the hidden ones with the
 * `hidden` attribute), so forms keep what was typed when you switch.
 *
 * The address's #fragment picks the tab (`/students/…#fees` opens Fees),
 * and choosing a tab updates the fragment, so a reload or a shared link
 * lands on the same tab.
 */
export function Tabs({ tabs, panels, label }: { tabs: Tab[]; panels: ReactNode[]; label: string }) {
  const hash = useSyncExternalStore(
    subscribe,
    () => window.location.hash.slice(1),
    () => "",
  );
  // The fragment is the single source of truth. replaceState doesn't fire
  // "hashchange", so choosing a tab bumps a counter to render again, and
  // the render reads the new fragment.
  const [, rerender] = useState(0);
  const active = tabs.some((t) => t.id === hash) ? hash : tabs[0].id;

  function choose(id: string) {
    window.history.replaceState(window.history.state, "", `#${id}`);
    rerender((n) => n + 1);
  }

  function onKey(e: KeyboardEvent<HTMLDivElement>) {
    const i = tabs.findIndex((t) => t.id === active);
    const next =
      e.key === "ArrowRight" ? (i + 1) % tabs.length : e.key === "ArrowLeft" ? (i - 1 + tabs.length) % tabs.length : e.key === "Home" ? 0 : e.key === "End" ? tabs.length - 1 : -1;
    if (next < 0) return;
    e.preventDefault();
    choose(tabs[next].id);
    document.getElementById(`tab-${tabs[next].id}`)?.focus();
  }

  return (
    <div className="flex flex-col gap-5">
      <div role="tablist" aria-label={label} className="tabs" onKeyDown={onKey}>
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            id={`tab-${t.id}`}
            aria-selected={active === t.id}
            aria-controls={`panel-${t.id}`}
            tabIndex={active === t.id ? 0 : -1}
            onClick={() => choose(t.id)}
          >
            {t.label}
            {t.badge}
          </button>
        ))}
      </div>
      {tabs.map((t, i) => (
        <div key={t.id} role="tabpanel" id={`panel-${t.id}`} aria-labelledby={`tab-${t.id}`} hidden={active !== t.id} className="tab-panel">
          {panels[i]}
        </div>
      ))}
    </div>
  );
}
