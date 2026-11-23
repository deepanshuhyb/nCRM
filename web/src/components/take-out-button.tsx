"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { ApiError, apiSend } from "@/lib/api-client";

/** Take a student out of a class from today, after a second tap. */
export function TakeOutButton({ enrollmentId, name }: { enrollmentId: string; name: string }) {
  const router = useRouter();
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!confirming) {
    return (
      <button type="button" className="btn btn-quiet" onClick={() => setConfirming(true)} aria-label={`Take ${name} out of the class`}>
        Take out
      </button>
    );
  }
  return (
    <span className="flex flex-wrap items-center gap-2">
      <button
        type="button"
        className="btn btn-dark min-h-[44px]"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          setError(null);
          try {
            await apiSend("POST", `/enrollments/${enrollmentId}/end`, {});
            router.refresh();
          } catch (err) {
            setError(err instanceof ApiError ? err.message : "Something went wrong.");
          } finally {
            setBusy(false);
            setConfirming(false);
          }
        }}
      >
        {busy ? "Taking out…" : `Take ${name.split(" ")[0]} out from today`}
      </button>
      <button type="button" className="btn btn-quiet" onClick={() => setConfirming(false)}>
        Keep
      </button>
      {error ? (
        <span role="alert" className="field-error m-0">
          {error}
        </span>
      ) : null}
    </span>
  );
}
