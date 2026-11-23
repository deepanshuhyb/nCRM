"use client";

import { useRouter } from "next/navigation";
import { useEffect, useRef, useState, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import type { Me } from "@/lib/types";

const RESEND_AFTER_SECONDS = 30;

/**
 * Two steps on one card: phone number, then the 6-digit code.
 * `next` has already been checked by `safeNextPath` on the server.
 */
export function SignInForm({ next }: { next: string }) {
  const router = useRouter();
  const [step, setStep] = useState<"phone" | "code">("phone");
  const [phone, setPhone] = useState("");
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [resendIn, setResendIn] = useState(0);
  const codeRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (resendIn <= 0) return;
    const t = setTimeout(() => setResendIn((s) => s - 1), 1000);
    return () => clearTimeout(t);
  }, [resendIn]);

  useEffect(() => {
    if (step === "code") codeRef.current?.focus();
  }, [step]);

  async function sendCode(e?: FormEvent) {
    e?.preventDefault();
    setError(null);
    if (phone.replace(/\D/g, "").length < 10) {
      setError("Enter your 10-digit mobile number.");
      return;
    }
    setBusy(true);
    try {
      await apiSend("POST", "/auth/code", { phone });
      setStep("code");
      setCode("");
      setResendIn(RESEND_AFTER_SECONDS);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Something went wrong. Try again.");
    } finally {
      setBusy(false);
    }
  }

  async function verify(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (!/^\d{6}$/.test(code)) {
      setError("The code has 6 digits.");
      return;
    }
    setBusy(true);
    try {
      const me = await apiSend<Me>("POST", "/auth/verify", { phone, code });
      const destination = me.studio ? next : `/choose-studio${next === "/" ? "" : `?next=${encodeURIComponent(next)}`}`;
      router.replace(destination);
      router.refresh();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Something went wrong. Try again.");
      setBusy(false);
    }
  }

  return (
    <section className="float p-6 sm:p-8" aria-labelledby="sign-in-title">
      <div>
        <h1 id="sign-in-title" className="h-page">
          {step === "phone" ? "Sign in to your studio." : "Check WhatsApp."}
        </h1>
        <p className="soft mt-3 mb-6 text-base">
          {step === "phone"
            ? "We'll send a 6-digit code to the number your studio has for you."
            : `If ${phone} belongs to a studio, a code is on its way. It works for 10 minutes.`}
        </p>
      </div>

      {step === "phone" ? (
        <form onSubmit={sendCode} noValidate className="flex flex-col gap-4">
          <div>
            <label htmlFor="phone" className="field-label">
              Mobile number
            </label>
            <input
              id="phone"
              name="phone"
              type="tel"
              inputMode="tel"
              autoComplete="tel"
              placeholder="98765 43210"
              className="field"
              value={phone}
              onChange={(e) => setPhone(e.target.value)}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? "sign-in-error" : undefined}
              maxLength={20}
              required
            />
          </div>
          <ErrorLine message={error} />
          <button type="submit" className="btn btn-primary w-full" disabled={busy}>
            {busy ? "Sending code…" : "Send code"}
          </button>
        </form>
      ) : (
        <form onSubmit={verify} noValidate className="flex flex-col gap-4">
          <div>
            <label htmlFor="code" className="field-label">
              6-digit code
            </label>
            <input
              id="code"
              ref={codeRef}
              name="code"
              type="text"
              inputMode="numeric"
              autoComplete="one-time-code"
              pattern="\d{6}"
              maxLength={6}
              className="field field-code"
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? "sign-in-error" : undefined}
              required
            />
          </div>
          <ErrorLine message={error} />
          <button type="submit" className="btn btn-primary w-full" disabled={busy}>
            {busy ? "Checking…" : "Sign in"}
          </button>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <button
              type="button"
              className="btn btn-quiet"
              onClick={() => {
                setStep("phone");
                setError(null);
              }}
            >
              Use another number
            </button>
            <button
              type="button"
              className="btn btn-quiet"
              disabled={busy || resendIn > 0}
              onClick={() => sendCode()}
            >
              {resendIn > 0 ? `Send again in ${resendIn}s` : "Send a new code"}
            </button>
          </div>
        </form>
      )}
    </section>
  );
}

function ErrorLine({ message }: { message: string | null }) {
  return (
    <p id="sign-in-error" role="alert" aria-live="assertive" className="field-error m-0 min-h-[1.2em]">
      {message}
    </p>
  );
}
