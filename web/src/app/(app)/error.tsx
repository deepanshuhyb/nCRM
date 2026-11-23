"use client";

/** Shown when a page fails to load. Never shows the underlying error. */
export default function AppError({ reset }: { error: Error & { digest?: string }; reset: () => void }) {
  return (
    <section className="panel mx-auto max-w-[520px] p-7" role="alert">
      <h1 className="h-section">This page didn&apos;t load.</h1>
      <p className="soft mt-2 mb-5">Something went wrong on our side. Try again in a moment.</p>
      <button type="button" className="btn btn-primary" onClick={reset}>
        Try again
      </button>
    </section>
  );
}
