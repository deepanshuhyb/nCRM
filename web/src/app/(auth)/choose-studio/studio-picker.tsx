"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import { ROLE_LABEL, type Role } from "@/lib/types";

type Studio = { orgId: string; name: string; role: Role };

export function StudioPicker({
  studios,
  currentOrgId,
  next,
}: {
  studios: Studio[];
  currentOrgId: string | null;
  next: string;
}) {
  const router = useRouter();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function open(orgId: string) {
    setBusy(orgId);
    setError(null);
    try {
      await apiSend("POST", "/auth/studio", { org_id: orgId });
      router.replace(next);
      router.refresh();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Something went wrong. Try again.");
      setBusy(null);
    }
  }

  return (
    <div className="flex flex-col gap-2">
      {studios.map((s) => (
        <button
          key={s.orgId}
          type="button"
          className="row flex min-h-[64px] w-full items-center justify-between gap-3 px-4 text-left"
          style={{ color: "inherit", cursor: "pointer", font: "inherit" }}
          onClick={() => open(s.orgId)}
          disabled={busy !== null}
          aria-current={s.orgId === currentOrgId ? "true" : undefined}
        >
          <span>
            <span className="block text-base font-bold">{s.name}</span>
            <span className="muted text-[13px]">{ROLE_LABEL[s.role]}</span>
          </span>
          <span className="tag tag-quiet">{busy === s.orgId ? "Opening…" : "Open"}</span>
        </button>
      ))}
      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
    </div>
  );
}
