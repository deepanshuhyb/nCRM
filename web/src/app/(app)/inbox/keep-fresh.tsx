"use client";

import { useRouter } from "next/navigation";
import { useEffect } from "react";

/**
 * Ask the server for the page again every few seconds while the tab is
 * visible, so new messages appear without a reload. There is no socket:
 * a studio's inbox is a handful of conversations, and a small request
 * every 12 seconds is simpler to run and to reason about than a
 * connection that has to be kept open and authorised.
 */
export function KeepFresh({ seconds = 12 }: { seconds?: number }) {
  const router = useRouter();
  useEffect(() => {
    const tick = () => {
      if (document.visibilityState === "visible") router.refresh();
    };
    const timer = window.setInterval(tick, seconds * 1000);
    document.addEventListener("visibilitychange", tick);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", tick);
    };
  }, [router, seconds]);
  return null;
}
