import Link from "next/link";

/**
 * Shown inside the signed-in shell when a page calls notFound(): a link to a
 * student, class or session that doesn't exist in this studio (or belongs to
 * another one; the API answers both the same way).
 *
 * The shell already provides <main>, so this must not add a second one. The
 * top-level not-found.tsx stays for addresses that match no page at all.
 */
export default function NotFound() {
  return (
    <section className="panel mx-auto mt-10 max-w-[520px] p-7">
      <h1 className="h-section">That page doesn&apos;t exist.</h1>
      <p className="soft mt-2 mb-5">It may have been removed, or the link is wrong.</p>
      <Link href="/" className="btn btn-primary">
        Go to home
      </Link>
    </section>
  );
}
